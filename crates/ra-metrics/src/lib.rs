//! Room-acoustic metrics. Owned by the validation thread.
//!
//! The same functions compute metrics from ray-tracer histograms and from
//! measured or synthesised impulse responses.
//!
//! - ISO 3382-1:2009: EDT, T20, T30, C50, C80, D50, Ts
//! - Sabine and Eyring reverberation time from volume and absorption
//! - IEC 60268-16:2020 indirect method: MTF and STI (male weights, no
//!   level-dependent masking or ambient noise yet)
//!
//! Inputs are already split into bands. Octave filtering of a broadband IR
//! is not here yet; the engine writes per-band energy directly.

pub mod decay;
pub mod statistical;
pub mod sti;

pub use decay::{Band, band_from_energy, band_from_ir, dynamic_range_db, schroeder};
pub use sti::{BANDS_HZ, MOD_FREQS_HZ, mtf_band, sti_from_mtf};

use ra_engine::EnergyResponse;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Options {
    /// Evaluate from sample 0 instead of from the direct sound (first sample
    /// within 20 dB of the maximum, ISO 3382-1 A.3.4).
    pub skip_onset: bool,
}

/// ISO 3382-1 quantities, one value per band. `None` where a value can't be evaluated.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Iso3382 {
    pub edt_s: Vec<Option<f32>>,
    pub t20_s: Vec<Option<f32>>,
    pub t30_s: Vec<Option<f32>>,
    pub c50_db: Vec<Option<f32>>,
    pub c80_db: Vec<Option<f32>>,
    pub d50: Vec<Option<f32>>,
    pub ts_s: Vec<Option<f32>>,
    /// Needs a calibrated free-field reference; not computed yet.
    pub g_db: Vec<Option<f32>>,
}

impl Iso3382 {
    fn push(&mut self, b: Option<Band>) {
        let f = |v: f64| v.is_finite().then_some(v as f32);
        self.edt_s.push(b.and_then(|b| b.edt).map(|v| v as f32));
        self.t20_s.push(b.and_then(|b| b.t20).map(|v| v as f32));
        self.t30_s.push(b.and_then(|b| b.t30).map(|v| v as f32));
        self.c50_db.push(b.and_then(|b| f(b.c50)));
        self.c80_db.push(b.and_then(|b| f(b.c80)));
        self.d50.push(b.and_then(|b| f(b.d50)));
        self.ts_s.push(b.and_then(|b| f(b.ts)));
        self.g_db.push(None);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sti {
    pub sti: f32,
    /// Modulation transfer function, `[7 octaves][14 modulation frequencies]`.
    pub mtf: Vec<[f32; 14]>,
}

#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
    #[error("invalid input: {0}")]
    Invalid(String),
}

/// Parameters per band from an energy histogram. The bin width comes from
/// `energy.bin_s`; `fs` must agree with it (it is the histogram's rate, 1 / bin_s).
pub fn from_energy(energy: &EnergyResponse, fs: f32, opts: &Options) -> Result<Iso3382, MetricsError> {
    let n = energy.n_bins;
    if n == 0 || !energy.data.len().is_multiple_of(n) {
        return Err(MetricsError::Invalid(format!(
            "{} values is not a whole number of {n}-bin bands",
            energy.data.len()
        )));
    }
    let rate = 1.0 / energy.bin_s as f64;
    if (fs as f64 * energy.bin_s as f64 - 1.0).abs() > 1e-3 {
        return Err(MetricsError::Invalid(format!("fs {fs} Hz does not match bin width {} s", energy.bin_s)));
    }
    let mut out = Iso3382::default();
    for band in energy.data.chunks(n) {
        let e: Vec<f64> = band.iter().map(|&v| v as f64).collect();
        let ok = e.iter().all(|v| v.is_finite() && *v >= 0.0) && e.iter().any(|&v| v > 0.0);
        out.push(ok.then(|| band_from_energy(&e, rate, opts)));
    }
    Ok(out)
}

/// Parameters of one band-limited (or broadband) pressure impulse response.
pub fn from_ir(ir: &[f32], fs: f32, opts: &Options) -> Result<Iso3382, MetricsError> {
    if ir.is_empty() || ir.iter().all(|&v| v == 0.0) || fs.is_nan() || fs <= 0.0 {
        return Err(MetricsError::Invalid("empty or silent impulse response".into()));
    }
    let x: Vec<f64> = ir.iter().map(|&v| v as f64).collect();
    let mut out = Iso3382::default();
    out.push(Some(band_from_ir(&x, fs as f64, opts)));
    Ok(out)
}

/// IEC 60268-16 indirect method from octave-band impulse responses, 125 Hz to 8 kHz.
pub fn sti_indirect(octave_irs: [&[f64]; 7], fs: f64) -> Result<Sti, MetricsError> {
    let mut mtf = [[0.0; 14]; 7];
    for (row, ir) in mtf.iter_mut().zip(octave_irs) {
        if ir.iter().all(|&v| v == 0.0) {
            return Err(MetricsError::Invalid("silent octave band".into()));
        }
        let e: Vec<f64> = ir.iter().map(|p| p * p).collect();
        *row = mtf_band(&e, fs);
    }
    Ok(Sti { sti: sti_from_mtf(&mtf) as f32, mtf: mtf.iter().map(|r| r.map(|v| v as f32)).collect() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const D60: f64 = 13.815_510_557_964_274;

    #[test]
    fn histogram_bands_are_evaluated_separately() {
        let bin_s = 0.001f32;
        let n = 3000;
        let band = |t60: f64| (0..n).map(move |i| (-D60 / t60 * i as f64 * bin_s as f64).exp() as f32);
        let mut data: Vec<f32> = band(2.0).collect();
        data.extend(band(1.0));
        data.extend(std::iter::repeat_n(0.0, n));
        let e = EnergyResponse { receiver: "R1".into(), bin_s, n_bins: n, data };
        let m = from_energy(&e, 1000.0, &Options::default()).unwrap();
        assert!((m.t20_s[0].unwrap() - 2.0).abs() < 0.01);
        assert!((m.t20_s[1].unwrap() - 1.0).abs() < 0.01);
        assert_eq!(m.t20_s[2], None, "silent band");
        assert!(from_energy(&e, 48_000.0, &Options::default()).is_err(), "fs must match bin width");
    }

    #[test]
    fn sti_from_pressure_irs() {
        let fs = 8000.0;
        let ir: Vec<f64> = (0..48_000).map(|i| (-D60 / 2.0 * i as f64 / fs).exp().sqrt()).collect();
        let s = sti_indirect([&ir[..]; 7], fs).unwrap();
        let row =
            MOD_FREQS_HZ.map(|f| 1.0 / (1.0 + (2.0 * std::f64::consts::PI * f * 2.0 / D60).powi(2)).sqrt());
        assert!((s.sti as f64 - sti_from_mtf(&[row; 7])).abs() < 0.003);
        assert_eq!(s.mtf.len(), 7);
    }
}
