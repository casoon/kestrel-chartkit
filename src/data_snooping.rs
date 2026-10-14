//! Tests against data snooping: is the best of many rules better than a benchmark, given that it
//! was picked as the best?
//!
//! Input: `K` aligned series of **excess performance** `d_k[t]` (rule `k` minus benchmark in
//! period `t`, positive = better), all of length `T`. Both tests resample the periods jointly with
//! the **stationary bootstrap** (Politis–Romano): a resample starts at a uniform index; each further
//! index is a new uniform start with probability `1 / mean_block`, otherwise the next index (wrapping
//! around). The random numbers come from SplitMix64 seeded with `seed`
//! (`u = (x >> 11) · 2⁻⁵³`, index `⌊u · T⌋`), so the result is reproducible.
//!
//! With `d̄_k` the mean of `d_k` and `d̄*_{k,b}` its mean in resample `b` (`B` resamples):
//!
//! | test | statistic | resampled statistic | p-value |
//! |---|---|---|---|
//! | White's Reality Check | `V = max_k √T d̄_k` | `V*_b = max_k √T (d̄*_{k,b} − d̄_k)` | `#{V*_b ≥ V} / B` |
//! | Hansen's SPA (consistent) | `T = max(0, max_k √T d̄_k / ω_k)` | `T*_b = max(0, max_k √T (d̄*_{k,b} − μ_k) / ω_k)` | `#{T*_b ≥ T} / B` |
//!
//! `ω_k² = (1/B) Σ_b (√T (d̄*_{k,b} − d̄_k))²` (bootstrap variance), `μ_k = d̄_k` if
//! `√T d̄_k / ω_k ≥ −√(2 ln ln T)` else `0` (Hansen's `g_c`; the threshold is 0 for `T < 3`). Series
//! with `ω_k = 0` (constant) are left out of the SPA maximum. The SPA is less conservative than the
//! Reality Check when clearly bad rules are among the candidates: they no longer inflate the null.
//!
//! `≥` matters at the boundary: when every rule is worse than the benchmark, `T = 0`, and every
//! resample reaches it — `p = 1`, as it must. Under the null the tests hold their size for
//! weakly dependent excess series; with clear autocorrelation choose `mean_block` at least as long
//! as it lasts, or the bootstrap variance is too small and both tests reject too often.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/data_snooping.py`.

/// Settings of [`data_snooping`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnoopingConfig {
    /// Mean block length of the stationary bootstrap (periods), at least 1.
    pub mean_block: f64,
    pub resamples: usize,
    pub seed: u64,
}

impl Default for SnoopingConfig {
    fn default() -> Self {
        Self {
            mean_block: 10.0,
            resamples: 2000,
            seed: 0x005E_ED0F_5AA0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DataSnooping {
    /// Index of the rule with the highest mean excess.
    pub best: usize,
    pub best_mean: f64,
    pub p_reality_check: f64,
    pub p_spa: f64,
}

/// SplitMix64.
struct SplitMix(u64);

impl SplitMix {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    fn index(&mut self, n: usize) -> usize {
        ((self.uniform() * n as f64) as usize).min(n - 1)
    }
}

/// Reality Check and SPA for the excess series `excess` (see the module docs). `None` without a
/// series, with series of different lengths, fewer than two periods, `mean_block < 1` or no
/// resamples.
// `b` indexes the b-th resample in every series of `boot` at once.
#[allow(clippy::needless_range_loop)]
pub fn data_snooping(excess: &[Vec<f64>], cfg: SnoopingConfig) -> Option<DataSnooping> {
    let k = excess.len();
    let t = excess.first()?.len();
    if k == 0
        || t < 2
        || excess.iter().any(|d| d.len() != t)
        || cfg.mean_block < 1.0
        || cfg.resamples == 0
    {
        return None;
    }
    let tf = t as f64;
    let wurzel_t = tf.sqrt();
    let mittel = |d: &[f64]| d.iter().sum::<f64>() / tf;
    let dbar: Vec<f64> = excess.iter().map(|d| mittel(d)).collect();
    let mut best = 0;
    for (i, m) in dbar.iter().enumerate() {
        if *m > dbar[best] {
            best = i;
        }
    }

    // Bootstrap means of every series.
    let mut rng = SplitMix(cfg.seed);
    let p_neu = 1.0 / cfg.mean_block;
    let mut boot: Vec<Vec<f64>> = vec![Vec::with_capacity(cfg.resamples); k];
    let mut idx = vec![0usize; t];
    for _ in 0..cfg.resamples {
        idx[0] = rng.index(t);
        for s in 1..t {
            idx[s] = if rng.uniform() < p_neu {
                rng.index(t)
            } else {
                (idx[s - 1] + 1) % t
            };
        }
        for (j, d) in excess.iter().enumerate() {
            let summe: f64 = idx.iter().map(|&i| d[i]).sum();
            boot[j].push(summe / tf);
        }
    }

    // Reality Check.
    let v = dbar
        .iter()
        .map(|m| wurzel_t * m)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut ueber_rc = 0usize;
    for b in 0..cfg.resamples {
        let vb = (0..k)
            .map(|j| wurzel_t * (boot[j][b] - dbar[j]))
            .fold(f64::NEG_INFINITY, f64::max);
        if vb >= v {
            ueber_rc += 1;
        }
    }

    // SPA (consistent).
    let omega: Vec<f64> = (0..k)
        .map(|j| {
            let q: f64 = boot[j]
                .iter()
                .map(|x| {
                    let z = wurzel_t * (x - dbar[j]);
                    z * z
                })
                .sum();
            (q / cfg.resamples as f64).sqrt()
        })
        .collect();
    let schwelle = if t >= 3 {
        (2.0 * tf.ln().ln()).sqrt()
    } else {
        0.0
    };
    let aktiv: Vec<usize> = (0..k).filter(|&j| omega[j] > 0.0).collect();
    let t_spa = aktiv
        .iter()
        .map(|&j| wurzel_t * dbar[j] / omega[j])
        .fold(0.0, f64::max);
    let mu: Vec<f64> = (0..k)
        .map(|j| {
            if omega[j] > 0.0 && wurzel_t * dbar[j] / omega[j] >= -schwelle {
                dbar[j]
            } else {
                0.0
            }
        })
        .collect();
    let mut ueber_spa = 0usize;
    for b in 0..cfg.resamples {
        let tb = aktiv
            .iter()
            .map(|&j| wurzel_t * (boot[j][b] - mu[j]) / omega[j])
            .fold(0.0, f64::max);
        if tb >= t_spa {
            ueber_spa += 1;
        }
    }
    let b = cfg.resamples as f64;
    Some(DataSnooping {
        best,
        best_mean: dbar[best],
        p_reality_check: ueber_rc as f64 / b,
        p_spa: ueber_spa as f64 / b,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clear_winner_survives_and_noise_does_not() {
        // Rule 0 beats the benchmark by 1 in every period (plus wiggle); rules 1–9 are noise.
        let t = 200;
        let mut rng = SplitMix(7);
        let mut series = vec![(0..t)
            .map(|_| 1.0 + rng.uniform() - 0.5)
            .collect::<Vec<_>>()];
        for _ in 1..10 {
            series.push((0..t).map(|_| rng.uniform() - 0.5).collect());
        }
        let r = data_snooping(&series, SnoopingConfig::default()).unwrap();
        assert_eq!(r.best, 0);
        assert!(r.p_spa < 0.01 && r.p_reality_check < 0.01, "{r:?}");
        // Every rule clearly worse than the benchmark: no evidence at all.
        let schlecht: Vec<Vec<f64>> = series[1..]
            .iter()
            .map(|d| d.iter().map(|x| x - 1.0).collect())
            .collect();
        let r = data_snooping(&schlecht, SnoopingConfig::default()).unwrap();
        assert_eq!((r.p_spa, r.p_reality_check), (1.0, 1.0), "{r:?}");
    }

    #[test]
    fn invalid_input_gives_none() {
        assert!(data_snooping(&[], SnoopingConfig::default()).is_none());
        assert!(data_snooping(&[vec![1.0]], SnoopingConfig::default()).is_none());
        assert!(data_snooping(&[vec![1.0, 2.0], vec![1.0]], SnoopingConfig::default()).is_none());
    }
}
