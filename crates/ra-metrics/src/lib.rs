//! Room-acoustic metrics. Owned by the validation thread.
//!
//! The same functions compute metrics from ray-tracer histograms and from
//! measured or synthesised impulse responses.

use ra_engine::EnergyResponse;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Options {
    /// Noise floor handling, evaluation ranges and the like are added here.
    pub _reserved: (),
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
    pub g_db: Vec<Option<f32>>,
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

pub fn from_energy(energy: &EnergyResponse, fs: f32, opts: &Options) -> Result<Iso3382, MetricsError> {
    let _ = (energy, fs, opts);
    Err(MetricsError::NotImplemented("from_energy"))
}

pub fn from_ir(ir: &[f32], fs: f32, opts: &Options) -> Result<Iso3382, MetricsError> {
    let _ = (ir, fs, opts);
    Err(MetricsError::NotImplemented("from_ir"))
}

/// IEC 60268-16 indirect method from octave-band impulse responses, 125 Hz to 8 kHz.
pub fn sti_indirect(octave_irs: [&[f64]; 7], fs: f64) -> Result<Sti, MetricsError> {
    let _ = (octave_irs, fs);
    Err(MetricsError::NotImplemented("sti_indirect"))
}
