//! Generic composition graph for indicators with typed dependencies.
//!
//! [`Indicator::on_bar`](crate::indicator::Indicator::on_bar) only ever sees the raw [`Bar`] — it
//! has no way to consume another indicator's output. `engine::pipeline` wires specific engines
//! together by hand for specific milestones; it is not a reusable graph. [`GraphIndicator`] and
//! [`CompositionGraph`] fill that gap: nodes declare named dependencies on other nodes' outputs,
//! the graph topologically sorts them once, and each bar is pushed through in that order with
//! upstream outputs made available to downstream nodes. Node-local warmup composes with
//! dependency warmup automatically.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::indicator::{Indicator, IndicatorOutput};
use crate::model::Bar;

/// A node in a [`CompositionGraph`]. Unlike [`Indicator`], `compute` also receives the current
/// bar's already-computed outputs of this node's declared [`GraphIndicator::dependencies`].
pub trait GraphIndicator: Send + Sync {
    fn name(&self) -> &str;
    /// Names of other graph nodes whose current-bar output this node reads via `deps` in
    /// [`GraphIndicator::compute`]. Must match node names actually present in the graph.
    fn dependencies(&self) -> &[String];
    fn warmup_period(&self) -> usize {
        0
    }
    fn reset(&mut self);
    fn compute(
        &mut self,
        bar: &Bar,
        deps: &HashMap<String, IndicatorOutput>,
    ) -> Option<IndicatorOutput>;
}

/// Adapts any existing [`Indicator`] into a dependency-free [`GraphIndicator`] leaf node, so the
/// full existing indicator catalog can be used inside a [`CompositionGraph`] unchanged.
pub struct Leaf<I: Indicator> {
    inner: I,
    dependencies: Vec<String>,
}

impl<I: Indicator> Leaf<I> {
    pub fn new(inner: I) -> Self {
        Self {
            inner,
            dependencies: Vec::new(),
        }
    }
}

impl<I: Indicator> GraphIndicator for Leaf<I> {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn dependencies(&self) -> &[String] {
        &self.dependencies
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn reset(&mut self) {
        self.inner.reset()
    }
    fn compute(
        &mut self,
        bar: &Bar,
        _deps: &HashMap<String, IndicatorOutput>,
    ) -> Option<IndicatorOutput> {
        self.inner.on_bar(bar)
    }
}

type ComputeFn = Box<
    dyn FnMut(&Bar, &HashMap<String, IndicatorOutput>) -> Option<IndicatorOutput> + Send + Sync,
>;

/// A [`GraphIndicator`] built from closures, for nodes that genuinely consume other nodes'
/// outputs (e.g. a Chandelier Exit reading an upstream ATR node).
pub struct ComposedNode {
    name: String,
    dependencies: Vec<String>,
    warmup_period: usize,
    compute_fn: ComputeFn,
    reset_fn: Box<dyn FnMut() + Send + Sync>,
}

impl ComposedNode {
    pub fn new(
        name: impl Into<String>,
        dependencies: Vec<String>,
        warmup_period: usize,
        compute_fn: impl FnMut(&Bar, &HashMap<String, IndicatorOutput>) -> Option<IndicatorOutput>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            dependencies,
            warmup_period,
            compute_fn: Box::new(compute_fn),
            reset_fn: Box::new(|| {}),
        }
    }

    /// Registers a callback invoked on [`CompositionGraph::reset`], for composed nodes that
    /// capture their own mutable state in the `compute_fn` closure.
    pub fn with_reset(mut self, reset_fn: impl FnMut() + Send + Sync + 'static) -> Self {
        self.reset_fn = Box::new(reset_fn);
        self
    }
}

impl GraphIndicator for ComposedNode {
    fn name(&self) -> &str {
        &self.name
    }
    fn dependencies(&self) -> &[String] {
        &self.dependencies
    }
    fn warmup_period(&self) -> usize {
        self.warmup_period
    }
    fn reset(&mut self) {
        (self.reset_fn)()
    }
    fn compute(
        &mut self,
        bar: &Bar,
        deps: &HashMap<String, IndicatorOutput>,
    ) -> Option<IndicatorOutput> {
        (self.compute_fn)(bar, deps)
    }
}

/// Error building or running a [`CompositionGraph`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    DuplicateNode(String),
    MissingDependency { node: String, dependency: String },
    CycleDetected,
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GraphError::DuplicateNode(name) => write!(f, "duplicate node name: {}", name),
            GraphError::MissingDependency { node, dependency } => write!(
                f,
                "node '{}' depends on unknown node '{}'",
                node, dependency
            ),
            GraphError::CycleDetected => write!(f, "dependency cycle detected"),
        }
    }
}

impl std::error::Error for GraphError {}

/// A dependency-ordered set of [`GraphIndicator`] nodes, executed together per bar.
pub struct CompositionGraph {
    nodes: HashMap<String, Box<dyn GraphIndicator>>,
    order: Vec<String>,
    effective_warmup: HashMap<String, usize>,
}

impl CompositionGraph {
    /// Builds a graph from `nodes`, computing a topological execution order and each node's
    /// effective warmup (its own `warmup_period` plus the maximum effective warmup of its
    /// dependencies). Fails on duplicate node names, references to unknown dependencies, or a
    /// dependency cycle.
    pub fn build(nodes: Vec<Box<dyn GraphIndicator>>) -> Result<Self, GraphError> {
        let mut by_name = HashMap::with_capacity(nodes.len());
        for node in nodes {
            let name = node.name().to_string();
            if by_name.insert(name.clone(), node).is_some() {
                return Err(GraphError::DuplicateNode(name));
            }
        }

        for (name, node) in &by_name {
            for dep in node.dependencies() {
                if !by_name.contains_key(dep) {
                    return Err(GraphError::MissingDependency {
                        node: name.clone(),
                        dependency: dep.clone(),
                    });
                }
            }
        }

        let order = topological_order(&by_name)?;

        let mut effective_warmup: HashMap<String, usize> = HashMap::with_capacity(order.len());
        for name in &order {
            let node = &by_name[name];
            let dep_warmup = node
                .dependencies()
                .iter()
                .map(|dep| effective_warmup[dep])
                .max()
                .unwrap_or(0);
            effective_warmup.insert(name.clone(), node.warmup_period() + dep_warmup);
        }

        Ok(Self {
            nodes: by_name,
            order,
            effective_warmup,
        })
    }

    /// Execution order computed at [`CompositionGraph::build`] time (dependencies before
    /// dependents).
    pub fn order(&self) -> &[String] {
        &self.order
    }

    /// A node's effective warmup: its own warmup plus the maximum effective warmup among its
    /// dependencies.
    pub fn effective_warmup(&self, name: &str) -> Option<usize> {
        self.effective_warmup.get(name).copied()
    }

    /// Runs one bar through every node in dependency order, making each node's output available
    /// to its dependents within the same call. Returns every node's output for this bar.
    pub fn on_bar(&mut self, bar: &Bar) -> HashMap<String, Option<IndicatorOutput>> {
        let mut outputs: HashMap<String, Option<IndicatorOutput>> =
            HashMap::with_capacity(self.order.len());
        let mut resolved: HashMap<String, IndicatorOutput> = HashMap::new();

        for name in self.order.clone() {
            let node = self.nodes.get_mut(&name).expect("node in order exists");
            let deps: HashMap<String, IndicatorOutput> = node
                .dependencies()
                .iter()
                .filter_map(|dep| resolved.get(dep).map(|o| (dep.clone(), o.clone())))
                .collect();

            let output = node.compute(bar, &deps);
            if let Some(o) = &output {
                resolved.insert(name.clone(), o.clone());
            }
            outputs.insert(name, output);
        }

        outputs
    }

    pub fn reset(&mut self) {
        for node in self.nodes.values_mut() {
            node.reset();
        }
    }
}

/// Kahn's algorithm: repeatedly removes nodes with no unprocessed dependencies. Any nodes left
/// over once no more can be removed are part of a cycle.
fn topological_order(
    nodes: &HashMap<String, Box<dyn GraphIndicator>>,
) -> Result<Vec<String>, GraphError> {
    let mut remaining_deps: HashMap<&str, HashSet<&str>> = nodes
        .iter()
        .map(|(name, node)| {
            (
                name.as_str(),
                node.dependencies().iter().map(|d| d.as_str()).collect(),
            )
        })
        .collect();

    let mut order = Vec::with_capacity(nodes.len());
    loop {
        let ready: Vec<&str> = remaining_deps
            .iter()
            .filter(|(_, deps)| deps.is_empty())
            .map(|(name, _)| *name)
            .collect();

        if ready.is_empty() {
            break;
        }

        let mut ready = ready;
        ready.sort_unstable();
        for name in ready {
            remaining_deps.remove(name);
            order.push(name.to_string());
        }

        for deps in remaining_deps.values_mut() {
            for done in &order {
                deps.remove(done.as_str());
            }
        }
    }

    if order.len() != nodes.len() {
        return Err(GraphError::CycleDetected);
    }

    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicator::atr::Atr;

    fn sample_bars() -> Vec<Bar> {
        (0..20)
            .map(|i| {
                let c = 100.0 + i as f64;
                Bar::new(i * 60, c, c + 2.0, c - 2.0, c, 100.0)
            })
            .collect()
    }

    #[test]
    fn test_graph_orders_dependencies_before_dependents() {
        let atr_leaf = Box::new(Leaf::new(Atr::new(5, 5)));
        let derived = Box::new(ComposedNode::new(
            "double_atr",
            vec!["atr".to_string()],
            0,
            |_bar, deps| deps.get("atr").map(|o| IndicatorOutput::new(o.value * 2.0)),
        ));

        let graph = CompositionGraph::build(vec![atr_leaf, derived]).unwrap();
        assert_eq!(
            graph.order(),
            &["atr".to_string(), "double_atr".to_string()]
        );
        // double_atr's effective warmup includes atr's warmup.
        assert_eq!(
            graph.effective_warmup("double_atr"),
            graph.effective_warmup("atr")
        );
    }

    #[test]
    fn test_graph_propagates_dependency_output_same_bar() {
        let atr_leaf = Box::new(Leaf::new(Atr::new(3, 3)));
        let derived = Box::new(ComposedNode::new(
            "double_atr",
            vec!["atr".to_string()],
            0,
            |_bar, deps| deps.get("atr").map(|o| IndicatorOutput::new(o.value * 2.0)),
        ));

        let mut graph = CompositionGraph::build(vec![atr_leaf, derived]).unwrap();
        let bars = sample_bars();
        let mut last_outputs = HashMap::new();
        for bar in &bars {
            last_outputs = graph.on_bar(bar);
        }

        let atr_value = last_outputs["atr"].as_ref().unwrap().value;
        let derived_value = last_outputs["double_atr"].as_ref().unwrap().value;
        assert!((derived_value - atr_value * 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_graph_rejects_missing_dependency() {
        let derived = Box::new(ComposedNode::new(
            "double_atr",
            vec!["missing_atr".to_string()],
            0,
            |_bar, _deps| None,
        ));
        let err = match CompositionGraph::build(vec![derived]) {
            Err(e) => e,
            Ok(_) => panic!("Expected missing dependency error"),
        };
        assert_eq!(
            err,
            GraphError::MissingDependency {
                node: "double_atr".to_string(),
                dependency: "missing_atr".to_string(),
            }
        );
    }

    #[test]
    fn test_graph_rejects_cycle() {
        let a = Box::new(ComposedNode::new("a", vec!["b".to_string()], 0, |_, _| {
            None
        }));
        let b = Box::new(ComposedNode::new("b", vec!["a".to_string()], 0, |_, _| {
            None
        }));
        let err = match CompositionGraph::build(vec![a, b]) {
            Err(e) => e,
            Ok(_) => panic!("Expected cycle error"),
        };
        assert_eq!(err, GraphError::CycleDetected);
    }

    #[test]
    fn test_graph_rejects_duplicate_node_name() {
        let a1 = Box::new(ComposedNode::new("a", vec![], 0, |_, _| None));
        let a2 = Box::new(ComposedNode::new("a", vec![], 0, |_, _| None));
        let err = match CompositionGraph::build(vec![a1, a2]) {
            Err(e) => e,
            Ok(_) => panic!("Expected duplicate node error"),
        };
        assert_eq!(err, GraphError::DuplicateNode("a".to_string()));
    }
}
