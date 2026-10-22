//! Energy decay curve and the ISO 3382-1 parameters of one band, in f64.

use crate::Options;

/// ISO 3382-1 parameters of one band. Decay times are `None` when the
/// energy decay curve does not reach the bottom of the evaluation range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub edt: Option<f64>,
    pub t20: Option<f64>,
    pub t30: Option<f64>,
    pub c50: f64,
    pub c80: f64,
    pub d50: f64,
    pub ts: f64,
}

/// Parameters from a pressure impulse response (squared internally).
pub fn band_from_ir(ir: &[f64], fs: f64, opts: &Options) -> Band {
    let energy: Vec<f64> = ir.iter().map(|p| p * p).collect();
    band_from_energy(&energy, fs, opts)
}

/// Parameters from a squared-pressure sequence or an energy histogram with bin width 1/fs.
pub fn band_from_energy(energy: &[f64], fs: f64, opts: &Options) -> Band {
    assert!(!energy.is_empty(), "empty energy sequence");
    let start = if opts.skip_onset { 0 } else { onset_index(energy, -20.0) };
    let e = &energy[start..];
    let edc = schroeder(e);
    let total = edc[0];
    let edc_db: Vec<f64> = edc.iter().map(|v| 10.0 * (v / total).max(1e-300).log10()).collect();

    let n50 = ((0.05 * fs).round() as usize).min(e.len());
    let n80 = ((0.08 * fs).round() as usize).min(e.len());
    let early50 = total - edc.get(n50).copied().unwrap_or(0.0);
    let early80 = total - edc.get(n80).copied().unwrap_or(0.0);
    let ts = e.iter().enumerate().map(|(i, v)| i as f64 * v).sum::<f64>() / (fs * total);

    // ISO 3382-1 5.3.3: the floor must sit at least 10 dB under the bottom of
    // the evaluation range (35 dB for T20, 45 dB for T30). Here the floor is
    // the end of the response, which also catches truncated simulations.
    let range = dynamic_range_db(e, fs);
    let needs = |lower_db: f64| range >= -lower_db + 10.0;
    Band {
        edt: regression_t(&edc_db, fs, 0.0, -10.0).filter(|_| needs(-10.0)),
        t20: regression_t(&edc_db, fs, -5.0, -25.0).filter(|_| needs(-25.0)),
        t30: regression_t(&edc_db, fs, -5.0, -35.0).filter(|_| needs(-35.0)),
        c50: 10.0 * (early50 / (total - early50)).log10(),
        c80: 10.0 * (early80 / (total - early80)).log10(),
        d50: early50 / total,
        ts,
    }
}

/// Schroeder backward integral, not normalised: edc[i] = sum of e[i..].
pub fn schroeder(energy: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; energy.len()];
    let mut acc = 0.0;
    for (o, e) in out.iter_mut().zip(energy).rev() {
        acc += e;
        *o = acc;
    }
    out
}

/// Level of the first 5 ms against the last 5 % of the response (at least 5 ms), in dB.
pub fn dynamic_range_db(energy: &[f64], fs: f64) -> f64 {
    let w = ((0.005 * fs) as usize).max(1);
    let tail = (energy.len() / 20).max(w).min(energy.len());
    let head = w.min(energy.len());
    let mean = |s: &[f64]| s.iter().sum::<f64>() / s.len() as f64;
    let floor = mean(&energy[energy.len() - tail..]);
    if floor <= 0.0 {
        return f64::INFINITY;
    }
    10.0 * (mean(&energy[..head]) / floor).log10()
}

fn onset_index(energy: &[f64], threshold_db: f64) -> usize {
    let max = energy.iter().cloned().fold(0.0, f64::max);
    let limit = max * 10f64.powf(threshold_db / 10.0);
    energy.iter().position(|&v| v >= limit).unwrap_or(0)
}

/// Least-squares line through the EDC between `upper_db` and `lower_db`,
/// both points included, extrapolated to 60 dB.
fn regression_t(edc_db: &[f64], fs: f64, upper_db: f64, lower_db: f64) -> Option<f64> {
    let i0 = edc_db.iter().position(|&v| v <= upper_db)?;
    let i1 = edc_db.iter().position(|&v| v <= lower_db)?;
    if i1 <= i0 + 1 {
        return None;
    }
    let n = (i1 - i0 + 1) as f64;
    let (mut st, mut sy, mut stt, mut sty) = (0.0, 0.0, 0.0, 0.0);
    for (k, y) in edc_db[i0..=i1].iter().enumerate() {
        let t = (i0 + k) as f64 / fs;
        st += t;
        sy += y;
        stt += t * t;
        sty += t * y;
    }
    let slope = (n * sty - st * sy) / (n * stt - st * st);
    (slope < 0.0).then(|| -60.0 / slope)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 48_000.0;
    const D60: f64 = 13.815_510_557_964_274; // 6 ln 10

    fn exp_decay(t60: f64, secs: f64) -> Vec<f64> {
        (0..(secs * FS) as usize).map(|i| (-D60 / t60 * i as f64 / FS).exp()).collect()
    }

    #[test]
    fn ideal_exponential_matches_closed_forms() {
        for &t60 in &[0.4, 1.0, 2.5] {
            let p = band_from_energy(&exp_decay(t60, 2.5 * t60), FS, &Options::default());
            let d = D60 / t60;
            for t in [p.edt, p.t20, p.t30] {
                assert!((t.unwrap() / t60 - 1.0).abs() < 1e-3, "{t:?} vs {t60}");
            }
            let c80 = 10.0 * ((d * 0.08).exp() - 1.0).log10();
            let c50 = 10.0 * ((d * 0.05).exp() - 1.0).log10();
            assert!((p.c80 - c80).abs() < 0.02, "C80 {} vs {}", p.c80, c80);
            assert!((p.c50 - c50).abs() < 0.02);
            assert!((p.d50 - (1.0 - (-d * 0.05).exp())).abs() < 1e-3);
            assert!((p.ts - 1.0 / d).abs() < 5e-4);
        }
    }

    #[test]
    fn onset_skips_leading_silence() {
        let mut e = vec![0.0; 480];
        e.extend(exp_decay(1.0, 2.0));
        let a = band_from_energy(&e, FS, &Options::default());
        let b = band_from_energy(&e[480..], FS, &Options { skip_onset: true });
        assert_eq!(a, b);
    }

    #[test]
    fn short_response_gives_no_t30() {
        let p = band_from_energy(&exp_decay(1.0, 0.4), FS, &Options::default());
        assert!(p.t30.is_none());
        assert!(p.edt.is_some());
    }

    #[test]
    fn from_ir_squares() {
        let ir: Vec<f64> = exp_decay(1.0, 2.0).iter().map(|e| -e.sqrt()).collect();
        let a = band_from_ir(&ir, FS, &Options::default());
        let b = band_from_energy(&exp_decay(1.0, 2.0), FS, &Options::default());
        assert_eq!(a, b);
    }
}
