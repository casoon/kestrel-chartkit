//! L2-regularised logistic regression, fitted by Newton–Raphson.
//!
//! A deliberately simple, explainable probability model: one weight per feature plus an
//! intercept, `P(y = 1 | x) = σ(b + w·x)`. Fitted by maximising the penalised log-likelihood
//! `Σ [y log p + (1 − y) log(1 − p)] − λ/2 · |w|²` (the intercept is not penalised) with Newton
//! steps, each solving the `(k + 1) × (k + 1)` system by Gaussian elimination with partial
//! pivoting. Converges in a handful of iterations for the feature counts it is meant for (tens).

#[cfg(feature = "serde")]
use serde::Serialize;

/// A fitted model.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct LogisticRegression {
    pub intercept: f64,
    pub weights: Vec<f64>,
    /// Newton iterations until the largest coefficient change fell below `1e-10`.
    pub iterations: usize,
}

const MAX_ITERATIONS: usize = 100;
const TOLERANCE: f64 = 1e-10;

fn sigmoid(z: f64) -> f64 {
    if z >= 0.0 {
        1.0 / (1.0 + (-z).exp())
    } else {
        let e = z.exp();
        e / (1.0 + e)
    }
}

impl LogisticRegression {
    /// Fits on rows `x` (all of equal length) and labels `y`. `None` for empty or ragged input,
    /// a negative `l2`, or a singular system (e.g. perfectly separable data without penalty).
    #[allow(clippy::needless_range_loop)] // Matrixindizes lesen sich hier klarer.
    pub fn fit(x: &[Vec<f64>], y: &[bool], l2: f64) -> Option<Self> {
        let n = x.len();
        if n == 0 || n != y.len() || l2.is_nan() || l2 < 0.0 {
            return None;
        }
        let k = x[0].len();
        if x.iter().any(|row| row.len() != k) {
            return None;
        }
        let dim = k + 1;
        // beta[0] = intercept, beta[1..] = weights.
        let mut beta = vec![0.0; dim];
        for iteration in 1..=MAX_ITERATIONS {
            let mut gradient = vec![0.0; dim];
            let mut hessian = vec![vec![0.0; dim]; dim];
            for (row, label) in x.iter().zip(y) {
                let z = beta[0] + row.iter().zip(&beta[1..]).map(|(a, b)| a * b).sum::<f64>();
                let p = sigmoid(z);
                let residual = f64::from(u8::from(*label)) - p;
                let weight = p * (1.0 - p);
                let feature = |j: usize| if j == 0 { 1.0 } else { row[j - 1] };
                for a in 0..dim {
                    gradient[a] += residual * feature(a);
                    for b in 0..dim {
                        hessian[a][b] += weight * feature(a) * feature(b);
                    }
                }
            }
            for j in 1..dim {
                gradient[j] -= l2 * beta[j];
                hessian[j][j] += l2;
            }
            let step = solve(hessian, gradient)?;
            let largest = step.iter().fold(0.0_f64, |m, s| m.max(s.abs()));
            for (b, s) in beta.iter_mut().zip(&step) {
                *b += s;
            }
            if largest < TOLERANCE {
                return Some(Self {
                    intercept: beta[0],
                    weights: beta[1..].to_vec(),
                    iterations: iteration,
                });
            }
        }
        Some(Self {
            intercept: beta[0],
            weights: beta[1..].to_vec(),
            iterations: MAX_ITERATIONS,
        })
    }

    /// `P(y = 1 | row)`. A row of the wrong length is treated as missing trailing features (0).
    pub fn predict(&self, row: &[f64]) -> f64 {
        sigmoid(
            self.intercept
                + self
                    .weights
                    .iter()
                    .zip(row)
                    .map(|(w, x)| w * x)
                    .sum::<f64>(),
        )
    }
}

/// Solves `a · s = b` by Gaussian elimination with partial pivoting; `None` if singular.
#[allow(clippy::needless_range_loop)]
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-14 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..n {
            let factor = a[row][col] / a[col][col];
            for k in col..n {
                a[row][k] -= factor * a[col][k];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut s = vec![0.0; n];
    for row in (0..n).rev() {
        let rest: f64 = (row + 1..n).map(|k| a[row][k] * s[k]).sum();
        s[row] = (b[row] - rest) / a[row][row];
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ragged_or_empty_input_is_rejected() {
        assert!(LogisticRegression::fit(&[], &[], 0.0).is_none());
        assert!(
            LogisticRegression::fit(&[vec![1.0], vec![1.0, 2.0]], &[true, false], 0.0).is_none()
        );
    }

    /// Separable data without penalty has no finite optimum; with a penalty it does, and the
    /// weight points the right way.
    #[test]
    fn penalty_makes_separable_data_fittable() {
        let x: Vec<Vec<f64>> = (0..20).map(|i| vec![i as f64 - 10.0]).collect();
        let y: Vec<bool> = (0..20).map(|i| i >= 10).collect();
        let m = LogisticRegression::fit(&x, &y, 1.0).unwrap();
        assert!(m.weights[0] > 0.0);
        assert!(m.predict(&[5.0]) > 0.9 && m.predict(&[-5.0]) < 0.1);
    }
}
