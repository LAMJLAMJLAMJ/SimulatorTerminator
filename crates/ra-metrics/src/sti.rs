//! IEC 60268-16:2020 indirect method: MTF from the squared impulse response, then STI.

pub const BANDS_HZ: [f64; 7] = [125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];
pub const MOD_FREQS_HZ: [f64; 14] =
    [0.63, 0.8, 1.0, 1.25, 1.6, 2.0, 2.5, 3.15, 4.0, 5.0, 6.3, 8.0, 10.0, 12.5];
/// Male speech band weights (alpha) and redundancy factors (beta), revision 5.
const ALPHA: [f64; 7] = [0.085, 0.127, 0.230, 0.233, 0.309, 0.224, 0.173];
const BETA: [f64; 6] = [0.085, 0.078, 0.065, 0.011, 0.047, 0.095];

/// Schroeder's indirect MTF of one band: |sum e(t) exp(-j 2 pi F t)| / sum e(t).
pub fn mtf_band(energy: &[f64], fs: f64) -> [f64; 14] {
    let total: f64 = energy.iter().sum();
    let mut out = [0.0; 14];
    for (m, f) in out.iter_mut().zip(MOD_FREQS_HZ) {
        let w = 2.0 * std::f64::consts::PI * f / fs;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, e) in energy.iter().enumerate() {
            let (s, c) = (w * i as f64).sin_cos();
            re += e * c;
            im -= e * s;
        }
        *m = re.hypot(im) / total;
    }
    out
}

/// STI from a 7 x 14 MTF, without level-dependent masking.
pub fn sti_from_mtf(mtf: &[[f64; 14]; 7]) -> f64 {
    let mut mti = [0.0; 7];
    for (k, row) in mtf.iter().enumerate() {
        let ti: f64 = row
            .iter()
            .map(|&m| {
                let m = m.clamp(1e-12, 1.0 - 1e-12);
                let snr = (10.0 * (m / (1.0 - m)).log10()).clamp(-15.0, 15.0);
                (snr + 15.0) / 30.0
            })
            .sum();
        mti[k] = ti / 14.0;
    }
    let mut sti: f64 = ALPHA.iter().zip(&mti).map(|(a, m)| a * m).sum();
    for k in 0..6 {
        sti -= BETA[k] * (mti[k] * mti[k + 1]).sqrt();
    }
    sti.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 8_000.0;
    const D60: f64 = 13.815_510_557_964_274;

    fn closed_form(t60: f64) -> f64 {
        let row =
            MOD_FREQS_HZ.map(|f| 1.0 / (1.0 + (2.0 * std::f64::consts::PI * f * t60 / D60).powi(2)).sqrt());
        sti_from_mtf(&[row; 7])
    }

    #[test]
    fn exponential_decay_matches_schroeder_mtf() {
        for &t60 in &[0.5f64, 1.0, 2.0] {
            let n = (FS * (3.0 * t60).max(1.6)) as usize;
            let e: Vec<f64> = (0..n).map(|i| (-D60 / t60 * i as f64 / FS).exp()).collect();
            let got = sti_from_mtf(&[mtf_band(&e, FS); 7]);
            assert!((got - closed_form(t60)).abs() < 0.003, "{got} vs {}", closed_form(t60));
        }
    }

    #[test]
    fn perfect_channel_is_one() {
        let mut e = vec![0.0; 16_000];
        e[0] = 1.0;
        assert!((sti_from_mtf(&[mtf_band(&e, FS); 7]) - 1.0).abs() < 1e-9);
    }
}
