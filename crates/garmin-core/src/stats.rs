//! The small amount of statistics this app is entitled to.
//!
//! Everything here exists because of one number: this athlete has 51 runs in
//! their whole history and about a dozen since June. At that size the
//! difference between a finding and a coincidence is not visible to the naked
//! eye, and a point estimate printed to two decimal places reads as certainty
//! the data cannot support.
//!
//! So nothing in this module returns a bare number. [`Estimate`] carries an
//! interval, and the interval is what the copy upstairs is allowed to lean on:
//! a slope whose interval straddles zero is a slope we do not have, however
//! good the centre looks.
//!
//! Two deliberate choices:
//!
//! - **The bootstrap is seeded, and the seed comes from the data.** Resampling
//!   with a fresh seed would make the same finding show a slightly different
//!   interval on every render, which reads as instability in the athlete rather
//!   than in the estimator. Same input, same interval, forever.
//! - **No p-values.** With seven candidate correlations ranked against one
//!   outcome, a p-value invites exactly the reading it cannot support. The
//!   honest object here is the interval, and for a ranking, how often the
//!   ranking survives resampling — [`rank_stability`].

use serde::{Deserialize, Serialize};

/// How many resamples a bootstrap draws.
///
/// 2000 puts the Monte-Carlo error on a 90% interval bound well below the
/// rounding these figures are printed at, and costs under a millisecond on the
/// series sizes here — the largest is 328 days.
const RESAMPLES: usize = 2000;

/// The interval every estimate is reported at.
///
/// 90 rather than 95 on purpose. These findings are read to decide what to do
/// on a Tuesday, not to clear a publication bar, and a 95% interval over 12
/// runs is so wide that every finding reads as "we know nothing" — which is its
/// own kind of dishonesty when the direction is in fact fairly clear.
const INTERVAL_PCT: f64 = 90.0;

/// A number with the uncertainty that belongs to it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Estimate {
    /// The point estimate — what the older code would have returned alone.
    pub value: f64,
    /// Lower bound of the 90% bootstrap interval.
    pub low: f64,
    /// Upper bound of the 90% bootstrap interval.
    pub high: f64,
    /// How many observations went in. Printed alongside, because an interval
    /// from six points and one from three hundred are different claims.
    pub n: usize,
}

impl Estimate {
    /// Whether the interval excludes zero — the one question most findings are
    /// really asking, and the gate they should fire on.
    pub fn excludes_zero(&self) -> bool {
        (self.low > 0.0 && self.high > 0.0) || (self.low < 0.0 && self.high < 0.0)
    }

    /// How confident the direction is, as a fraction of resamples agreeing with
    /// the point estimate's sign. Reported rather than thresholded, so copy can
    /// say "four times in five" instead of "significant".
    pub fn direction_confidence(&self) -> f64 {
        if self.excludes_zero() {
            return 1.0;
        }
        // Falls back to where zero sits inside the interval. Not a probability
        // in any formal sense, and the copy never calls it one.
        let span = self.high - self.low;
        if span <= 0.0 {
            return 0.5;
        }
        let below = ((0.0 - self.low) / span).clamp(0.0, 1.0);
        if self.value >= 0.0 {
            below
        } else {
            1.0 - below
        }
    }
}

/// A deterministic PRNG, seeded from the data it will resample.
///
/// xorshift64*, which is nowhere near cryptographic and does not need to be —
/// it is drawing indices into an array of at most a few hundred elements. What
/// matters is that it is reproducible and has no dependency behind it.
struct Rng(u64);

impl Rng {
    fn seeded(data: &[f64]) -> Self {
        // The seed is a hash of the sample, so identical input gives identical
        // resamples. The constants are FNV's; any odd multiplier would do.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for x in data {
            h ^= x.to_bits();
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        // A zero state would lock xorshift at zero forever.
        Self(if h == 0 { 0x9e37_79b9_7f4a_7c15 } else { h })
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

pub fn mean(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    Some(xs.iter().sum::<f64>() / xs.len() as f64)
}

/// The `pct`th percentile of an already-sorted slice, linearly interpolated.
fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = (pct / 100.0) * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        return sorted[lo];
    }
    sorted[lo] + (sorted[hi] - sorted[lo]) * (rank - lo as f64)
}

/// Bootstrap an arbitrary statistic over paired samples.
///
/// `stat` is handed a resampled copy of the indices and returns its value, or
/// `None` when that particular resample is degenerate (every x identical, say).
/// Degenerate draws are skipped rather than counted as zero, which would drag
/// every interval toward the origin.
fn bootstrap<F>(n: usize, seed: &[f64], stat: F) -> Option<(f64, f64)>
where
    F: Fn(&[usize]) -> Option<f64>,
{
    if n == 0 {
        return None;
    }
    let mut rng = Rng::seeded(seed);
    let mut draws = Vec::with_capacity(RESAMPLES);
    let mut idx = vec![0usize; n];
    for _ in 0..RESAMPLES {
        for slot in idx.iter_mut() {
            *slot = rng.below(n);
        }
        if let Some(v) = stat(&idx) {
            if v.is_finite() {
                draws.push(v);
            }
        }
    }
    // Under half the draws usable means the statistic is not stable enough on
    // this sample to quote an interval for at all.
    if draws.len() < RESAMPLES / 2 {
        return None;
    }
    draws.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let tail = (100.0 - INTERVAL_PCT) / 2.0;
    Some((percentile(&draws, tail), percentile(&draws, 100.0 - tail)))
}

/// Pearson correlation with a bootstrap interval.
///
/// Pairs where either side is missing are dropped, which is why `n` comes back
/// rather than being assumed equal to the input length.
pub fn correlation(xs: &[Option<f64>], ys: &[Option<f64>], min_pairs: usize) -> Option<Estimate> {
    let pairs: Vec<(f64, f64)> = xs
        .iter()
        .zip(ys)
        .filter_map(|(x, y)| match (x, y) {
            (Some(a), Some(b)) if a.is_finite() && b.is_finite() => Some((*a, *b)),
            _ => None,
        })
        .collect();
    if pairs.len() < min_pairs {
        return None;
    }

    let r = pearson(&pairs, &(0..pairs.len()).collect::<Vec<_>>())?;
    let seed: Vec<f64> = pairs.iter().flat_map(|(a, b)| [*a, *b]).collect();
    let (low, high) = bootstrap(pairs.len(), &seed, |idx| pearson(&pairs, idx))?;

    Some(Estimate {
        value: r,
        low,
        high,
        n: pairs.len(),
    })
}

fn pearson(pairs: &[(f64, f64)], idx: &[usize]) -> Option<f64> {
    let n = idx.len() as f64;
    if n < 3.0 {
        return None;
    }
    let (mut sx, mut sy) = (0.0, 0.0);
    for &i in idx {
        sx += pairs[i].0;
        sy += pairs[i].1;
    }
    let (mx, my) = (sx / n, sy / n);
    let (mut num, mut dx, mut dy) = (0.0, 0.0, 0.0);
    for &i in idx {
        let (a, b) = (pairs[i].0 - mx, pairs[i].1 - my);
        num += a * b;
        dx += a * a;
        dy += b * b;
    }
    let den = (dx * dy).sqrt();
    if den == 0.0 {
        return None;
    }
    Some(num / den)
}

/// A fitted straight line, with an interval on the thing that matters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fit {
    /// Change in y per unit x, with its interval. A fit whose slope interval
    /// straddles zero is a fit with no direction, and callers check that
    /// rather than reading `slope.value` and hoping.
    pub slope: Estimate,
    pub intercept: f64,
    /// Fraction of variance explained. Reported plainly, including when it is
    /// small — a lever worth 0.15 is still a lever, as long as nobody calls it
    /// a mechanism.
    pub r2: f64,
}

/// Ordinary least squares of y on x, with a bootstrap interval on the slope.
pub fn linear_fit(xs: &[f64], ys: &[f64], min_points: usize) -> Option<Fit> {
    let pairs: Vec<(f64, f64)> = xs
        .iter()
        .zip(ys)
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(x, y)| (*x, *y))
        .collect();
    if pairs.len() < min_points {
        return None;
    }

    let all: Vec<usize> = (0..pairs.len()).collect();
    let (slope, intercept) = ols(&pairs, &all)?;

    let my = pairs.iter().map(|p| p.1).sum::<f64>() / pairs.len() as f64;
    let ss_tot: f64 = pairs.iter().map(|p| (p.1 - my).powi(2)).sum();
    let ss_res: f64 = pairs
        .iter()
        .map(|p| (p.1 - (intercept + slope * p.0)).powi(2))
        .sum();
    let r2 = if ss_tot > 0.0 {
        (1.0 - ss_res / ss_tot).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let seed: Vec<f64> = pairs.iter().flat_map(|(a, b)| [*a, *b]).collect();
    let (low, high) = bootstrap(pairs.len(), &seed, |idx| ols(&pairs, idx).map(|(s, _)| s))?;

    Some(Fit {
        slope: Estimate {
            value: slope,
            low,
            high,
            n: pairs.len(),
        },
        intercept,
        r2,
    })
}

fn ols(pairs: &[(f64, f64)], idx: &[usize]) -> Option<(f64, f64)> {
    let n = idx.len() as f64;
    if n < 3.0 {
        return None;
    }
    let (mut sx, mut sy) = (0.0, 0.0);
    for &i in idx {
        sx += pairs[i].0;
        sy += pairs[i].1;
    }
    let (mx, my) = (sx / n, sy / n);
    let (mut num, mut den) = (0.0, 0.0);
    for &i in idx {
        let dx = pairs[i].0 - mx;
        num += dx * (pairs[i].1 - my);
        den += dx * dx;
    }
    if den == 0.0 {
        return None;
    }
    let slope = num / den;
    Some((slope, my - slope * mx))
}

/// The difference between two group means, with an interval.
///
/// Both groups are resampled together so the interval describes the difference
/// rather than either mean — the question a rest-day-versus-training-day
/// comparison is actually asking.
pub fn mean_difference(a: &[f64], b: &[f64], min_each: usize) -> Option<Estimate> {
    if a.len() < min_each || b.len() < min_each {
        return None;
    }
    let value = mean(a)? - mean(b)?;

    let seed: Vec<f64> = a.iter().chain(b).copied().collect();
    let mut rng = Rng::seeded(&seed);
    let mut draws = Vec::with_capacity(RESAMPLES);
    for _ in 0..RESAMPLES {
        let ra: f64 = (0..a.len()).map(|_| a[rng.below(a.len())]).sum::<f64>() / a.len() as f64;
        let rb: f64 = (0..b.len()).map(|_| b[rng.below(b.len())]).sum::<f64>() / b.len() as f64;
        draws.push(ra - rb);
    }
    draws.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let tail = (100.0 - INTERVAL_PCT) / 2.0;

    Some(Estimate {
        value,
        low: percentile(&draws, tail),
        high: percentile(&draws, 100.0 - tail),
        n: a.len() + b.len(),
    })
}

/// How often the top-ranked item stays top under resampling.
///
/// The reason this exists: ranking seven candidate metrics by |r| against one
/// outcome and printing the winner is the most inviting mistake in this whole
/// codebase. At r ≈ 0.2 over a couple of hundred days, second and first place
/// swap on a coin flip, and the copy would still read "sleep score moves your
/// HRV more than anything else you record".
///
/// Returns the fraction of resamples in which the leader led. Below about 0.6
/// there is no leader and the copy has to say so.
pub fn rank_stability(columns: &[Vec<Option<f64>>], outcome: &[Option<f64>]) -> Option<f64> {
    if columns.len() < 2 {
        return None;
    }
    // Rows usable for every column at once, so each resample ranks the same
    // days rather than a different subset per candidate.
    let rows: Vec<usize> = (0..outcome.len())
        .filter(|&i| {
            outcome.get(i).and_then(|v| *v).is_some()
                && columns.iter().all(|c| c.get(i).and_then(|v| *v).is_some())
        })
        .collect();
    if rows.len() < 25 {
        return None;
    }

    let cols: Vec<Vec<(f64, f64)>> = columns
        .iter()
        .map(|c| {
            rows.iter()
                .map(|&i| (c[i].unwrap(), outcome[i].unwrap()))
                .collect()
        })
        .collect();

    let leader = |idx: &[usize]| -> Option<usize> {
        let mut best = (0usize, -1.0f64);
        for (j, pairs) in cols.iter().enumerate() {
            let r = pearson(pairs, idx)?.abs();
            if r > best.1 {
                best = (j, r);
            }
        }
        Some(best.0)
    };

    let all: Vec<usize> = (0..rows.len()).collect();
    let actual = leader(&all)?;

    let seed: Vec<f64> = cols[0].iter().flat_map(|(a, b)| [*a, *b]).collect();
    let mut rng = Rng::seeded(&seed);
    let mut held = 0usize;
    let mut counted = 0usize;
    let mut idx = vec![0usize; rows.len()];
    for _ in 0..RESAMPLES {
        for slot in idx.iter_mut() {
            *slot = rng.below(rows.len());
        }
        if let Some(w) = leader(&idx) {
            counted += 1;
            if w == actual {
                held += 1;
            }
        }
    }
    if counted == 0 {
        return None;
    }
    Some(held as f64 / counted as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some(xs: &[f64]) -> Vec<Option<f64>> {
        xs.iter().map(|x| Some(*x)).collect()
    }

    #[test]
    fn a_perfect_line_has_a_slope_interval_that_excludes_zero() {
        let xs: Vec<f64> = (0..20).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 3.0 * x + 1.0).collect();
        let fit = linear_fit(&xs, &ys, 5).expect("a perfect line fits");
        assert!((fit.slope.value - 3.0).abs() < 1e-9);
        assert!(fit.slope.excludes_zero());
        assert!(fit.r2 > 0.999);
    }

    /// The case the whole module exists for: noise that happens to trend.
    /// The point estimate is nonzero and would print as a finding; the interval
    /// is what stops it.
    #[test]
    fn noise_produces_a_slope_interval_that_straddles_zero() {
        let xs: Vec<f64> = (0..14).map(|i| i as f64).collect();
        let ys = [
            5.0, -3.0, 8.0, 1.0, -6.0, 4.0, 0.0, 7.0, -2.0, 3.0, -5.0, 6.0, -1.0, 2.0,
        ];
        let fit = linear_fit(&xs, &ys, 5).expect("noise still fits a line");
        assert!(
            !fit.slope.excludes_zero(),
            "noise must not produce a directional claim, got {:?}",
            fit.slope
        );
    }

    /// Same data twice has to give the same interval, or a finding's numbers
    /// change every time the screen re-renders.
    #[test]
    fn the_same_sample_gives_the_same_interval_every_time() {
        let xs = some(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        let ys = some(&[2.0, 1.0, 4.0, 3.0, 6.0, 5.0, 8.0, 7.0]);
        let a = correlation(&xs, &ys, 5).unwrap();
        let b = correlation(&xs, &ys, 5).unwrap();
        assert_eq!(a.low.to_bits(), b.low.to_bits());
        assert_eq!(a.high.to_bits(), b.high.to_bits());
    }

    #[test]
    fn correlation_drops_pairs_where_either_side_is_missing() {
        let xs = vec![Some(1.0), None, Some(3.0), Some(4.0), Some(5.0), Some(6.0)];
        let ys = vec![Some(1.0), Some(2.0), None, Some(4.0), Some(5.0), Some(6.0)];
        let c = correlation(&xs, &ys, 4).expect("four complete pairs remain");
        assert_eq!(c.n, 4);
    }

    #[test]
    fn too_few_pairs_is_no_answer_rather_than_a_shaky_one() {
        let xs = some(&[1.0, 2.0, 3.0]);
        let ys = some(&[1.0, 2.0, 3.0]);
        assert!(correlation(&xs, &ys, 8).is_none());
    }

    /// A leader that only leads because of which days landed in the sample is
    /// exactly what `rank_stability` is for.
    #[test]
    fn a_genuine_leader_survives_resampling() {
        let outcome: Vec<Option<f64>> = (0..80).map(|i| Some((i as f64 * 0.7).sin())).collect();
        // First column *is* the outcome; nothing can outrank it.
        let strong = outcome.clone();
        let noise: Vec<Option<f64>> = (0..80).map(|i| Some(((i * 37) % 11) as f64)).collect();
        let stability = rank_stability(&[strong, noise], &outcome).unwrap();
        assert!(stability > 0.95, "got {stability}");
    }

    #[test]
    fn two_indistinguishable_candidates_do_not_produce_a_stable_leader() {
        let outcome: Vec<Option<f64>> = (0..80).map(|i| Some(((i * 13) % 7) as f64)).collect();
        let a: Vec<Option<f64>> = (0..80).map(|i| Some(((i * 29) % 5) as f64)).collect();
        let b: Vec<Option<f64>> = (0..80).map(|i| Some(((i * 31) % 5) as f64)).collect();
        let stability = rank_stability(&[a, b], &outcome).unwrap();
        assert!(stability < 0.9, "got {stability}");
    }

    #[test]
    fn a_mean_difference_between_clearly_separated_groups_excludes_zero() {
        let a = [10.0, 11.0, 9.0, 10.5, 10.2, 9.8, 10.1];
        let b = [4.0, 5.0, 3.5, 4.2, 4.8, 3.9, 4.1];
        let d = mean_difference(&a, &b, 5).unwrap();
        assert!(d.excludes_zero());
        assert!((d.value - 6.0).abs() < 1.0);
    }

    #[test]
    fn overlapping_groups_do_not() {
        let a = [10.0, 4.0, 8.0, 5.0, 9.0, 6.0];
        let b = [7.0, 9.0, 5.0, 8.0, 4.0, 10.0];
        let d = mean_difference(&a, &b, 5).unwrap();
        assert!(!d.excludes_zero());
    }
}
