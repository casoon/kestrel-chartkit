//! Typed registry parameter values.
//!
//! [`super::registry::build`]/[`super::registry::build_checked`] accept `HashMap<String, f64>`,
//! which cannot express boolean flags, enum selections, free text, timestamps, timeframes, or
//! price sources. [`ParamValue`] gives each of those a typed representation;
//! [`super::registry::build_typed`] validates them and flattens the numeric-compatible ones down
//! to the existing `f64` parameter map so it can reuse the full validated `build_checked` matching
//! logic unchanged.

use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::model::Source;
use crate::session::SessionConfig;
use crate::timeframe::Timeframe;

/// A single typed registry parameter value.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ParamValue {
    Float(f64),
    Int(i64),
    Bool(bool),
    Enum(String),
    Text(String),
    Timestamp(i64),
    Timeframe(Timeframe),
    /// A trading session window (see [`crate::session::SessionConfig`]).
    Session(SessionConfig),
    /// A provider-neutral instrument/symbol identifier, distinct from free-form [`ParamValue::Text`].
    Symbol(String),
    Source(Source),
}

impl ParamValue {
    /// Flattens this value to `f64` where that is a lossless, well-defined conversion (numeric
    /// and boolean values). Returns `None` for values with no meaningful scalar form (`Enum`,
    /// `Text`, `Timeframe`, `Source`).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            ParamValue::Float(v) => Some(*v),
            ParamValue::Int(v) => Some(*v as f64),
            ParamValue::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
            ParamValue::Timestamp(v) => Some(*v as f64),
            ParamValue::Enum(_)
            | ParamValue::Text(_)
            | ParamValue::Timeframe(_)
            | ParamValue::Session(_)
            | ParamValue::Symbol(_)
            | ParamValue::Source(_) => None,
        }
    }

    /// A short, stable type name used in error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            ParamValue::Float(_) => "float",
            ParamValue::Int(_) => "int",
            ParamValue::Bool(_) => "bool",
            ParamValue::Enum(_) => "enum",
            ParamValue::Text(_) => "text",
            ParamValue::Timestamp(_) => "timestamp",
            ParamValue::Timeframe(_) => "timeframe",
            ParamValue::Session(_) => "session",
            ParamValue::Symbol(_) => "symbol",
            ParamValue::Source(_) => "source",
        }
    }
}

impl From<f64> for ParamValue {
    fn from(value: f64) -> Self {
        ParamValue::Float(value)
    }
}

impl From<bool> for ParamValue {
    fn from(value: bool) -> Self {
        ParamValue::Bool(value)
    }
}

/// A parameter map accepting typed values instead of only `f64`.
pub type TypedParams = HashMap<String, ParamValue>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_f64_conversions() {
        assert_eq!(ParamValue::Float(1.5).as_f64(), Some(1.5));
        assert_eq!(ParamValue::Int(3).as_f64(), Some(3.0));
        assert_eq!(ParamValue::Bool(true).as_f64(), Some(1.0));
        assert_eq!(ParamValue::Bool(false).as_f64(), Some(0.0));
        assert_eq!(ParamValue::Timestamp(1_000).as_f64(), Some(1_000.0));
        assert_eq!(ParamValue::Enum("fast".to_string()).as_f64(), None);
        assert_eq!(ParamValue::Text("note".to_string()).as_f64(), None);
        assert_eq!(ParamValue::Timeframe(Timeframe::Minute(5)).as_f64(), None);
        assert_eq!(ParamValue::Source(Source::Close).as_f64(), None);
        assert_eq!(ParamValue::Session(SessionConfig::default()).as_f64(), None);
        assert_eq!(ParamValue::Symbol("EURUSD".to_string()).as_f64(), None);
    }
}
