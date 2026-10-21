use serde::{Deserialize, Serialize};

/// An acoustic material: per-band absorption and scattering coefficients.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Material {
    pub name: String,
    /// One value per band of the project's band set, 0..=1.
    pub absorption: Vec<f32>,
    /// One value per band of the project's band set, 0..=1.
    pub scattering: Vec<f32>,
    /// Where the values come from, e.g. "PTB database #1234" or "fitted to measurement".
    #[serde(default)]
    pub source: String,
}
