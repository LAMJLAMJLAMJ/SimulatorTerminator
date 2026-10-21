//! The internal loudspeaker model. Every format (GLL, CLF, SOFA) is converted
//! into a [`SpeakerModel`], and the engine reads loudspeakers only through it.
//!
//! Owned by the loudspeaker-data thread.
//!
//! Speaker frame: +X on-axis, +Y left, +Z up (as SOFA). Balloon grid: theta is
//! the angle from +X (0..=180), phi the roll around +X (0 = up, 90 = right),
//! as GLL and CLF store it.

use num_complex::Complex32;
use ra_core::{Band, BandSet};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerFormat {
    Gll,
    Clf,
    Sofa,
    /// Built in: an ideal omnidirectional source, used by validation cases.
    Omni,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeakerOrigin {
    pub format: SpeakerFormat,
    pub file_name: String,
    pub format_version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseKind {
    Complex,
    MagnitudeOnly,
}

/// Which part of the sphere a balloon stores. The rest follows by mirroring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Symmetry {
    None,
    /// Mirror about the horizontal plane (phi 0..=180 stored).
    Half,
    /// Mirror about both planes (phi 0..=90 stored).
    Quarter,
    /// Rotationally symmetric about +X (one phi stored).
    Axial,
}

/// Directivity relative to on-axis, complex64 `[F, theta, phi]`, delay folded into phase.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Balloon {
    pub theta_step_deg: f32,
    pub phi_step_deg: f32,
    pub n_theta: usize,
    pub n_phi: usize,
    pub symmetry: Symmetry,
    /// Row-major `[freq][theta][phi]`. Stored as `.npy` in the project file.
    #[serde(skip)]
    pub data: Vec<Complex32>,
}

/// Absolute on-axis response at a reference distance and input.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OnAxis {
    #[serde(skip)]
    pub spectrum: Vec<Complex32>,
    pub ref_distance_m: f32,
    /// Reference input, in volts or watts as `ref_input_unit` says.
    pub ref_input: f32,
    pub ref_input_unit: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FilterChain {
    /// Complex gain per native frequency point; empty means flat.
    #[serde(skip)]
    pub response: Vec<Complex32>,
}

/// Energy-averaged power per band and direction, precomputed for the ray tracer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BandTable {
    pub bands: BandSet,
    /// Row-major `[band][theta][phi]` on the balloon's grid.
    #[serde(skip)]
    pub power: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transducer {
    /// Position in the cabinet, metres, speaker frame.
    pub position: [f32; 3],
    /// Orientation in the cabinet as a unit quaternion `[w, x, y, z]`.
    pub orientation: [f32; 4],
    pub directivity: Balloon,
    pub on_axis: OnAxis,
    pub filter: FilterChain,
    pub band_power: BandTable,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArrayGeometry {
    /// Splay angle between consecutive elements, degrees.
    pub splay_deg: Vec<f32>,
    pub element_gain_db: Vec<f32>,
    pub element_delay_s: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeakerModel {
    /// SHA-256 of the converted model, hex.
    pub id: String,
    pub name: String,
    pub origin: SpeakerOrigin,
    /// Native frequency points, e.g. 241 at 1/24 octave for GLL.
    pub freqs_hz: Vec<f32>,
    pub transducers: Vec<Transducer>,
    pub array: Option<ArrayGeometry>,
    pub phase: PhaseKind,
}

#[derive(Debug, thiserror::Error)]
pub enum SpeakerError {
    #[error("unsupported file: {0}")]
    Unsupported(String),
    #[error("malformed file: {0}")]
    Malformed(String),
}

/// Implemented by each format reader (CLF and SOFA here, GLL in `ra-gll` or in TypeScript).
pub trait SpeakerReader {
    fn read(bytes: &[u8]) -> Result<SpeakerModel, SpeakerError>;
}

/// Balloon grid indices for a direction in the speaker frame.
pub fn grid_angles_deg(dir: [f32; 3]) -> (f32, f32) {
    let [x, y, z] = dir;
    let len = (x * x + y * y + z * z).sqrt();
    let theta = (x / len).clamp(-1.0, 1.0).acos().to_degrees();
    // phi: 0 = up (+Z), 90 = right (-Y)
    let phi = (-y).atan2(z).to_degrees().rem_euclid(360.0);
    (theta, phi)
}

impl Balloon {
    /// A balloon of ones: no directivity.
    pub fn omni(n_freqs: usize) -> Self {
        Balloon {
            theta_step_deg: 180.0,
            phi_step_deg: 360.0,
            n_theta: 2,
            n_phi: 1,
            symmetry: Symmetry::Axial,
            data: vec![Complex32::new(1.0, 0.0); n_freqs * 2],
        }
    }

    fn index(&self, theta_deg: f32, phi_deg: f32) -> (usize, usize) {
        let phi = match self.symmetry {
            Symmetry::None => phi_deg,
            Symmetry::Half => {
                if phi_deg > 180.0 {
                    360.0 - phi_deg
                } else {
                    phi_deg
                }
            }
            Symmetry::Quarter => {
                let p = if phi_deg > 180.0 { 360.0 - phi_deg } else { phi_deg };
                if p > 90.0 { 180.0 - p } else { p }
            }
            Symmetry::Axial => 0.0,
        };
        let t = ((theta_deg / self.theta_step_deg).round() as usize).min(self.n_theta - 1);
        let p = ((phi / self.phi_step_deg).round() as usize) % self.n_phi.max(1);
        (t, p)
    }

    /// Nearest grid value. Interpolation comes with the real readers.
    pub fn at(&self, f: usize, dir: [f32; 3]) -> Complex32 {
        let (theta, phi) = grid_angles_deg(dir);
        let (t, p) = self.index(theta, phi);
        self.data[(f * self.n_theta + t) * self.n_phi + p]
    }
}

impl SpeakerModel {
    /// An ideal omnidirectional source with 0 dB on-axis at 1 m.
    pub fn omni(bands: BandSet) -> Self {
        let freqs_hz = bands.centres_hz().to_vec();
        let n = freqs_hz.len();
        let directivity = Balloon::omni(n);
        let band_power = BandTable { bands, power: vec![1.0; n * 2] };
        SpeakerModel {
            id: String::new(),
            name: "Omni".into(),
            origin: SpeakerOrigin {
                format: SpeakerFormat::Omni,
                file_name: String::new(),
                format_version: String::new(),
            },
            freqs_hz,
            transducers: vec![Transducer {
                position: [0.0; 3],
                orientation: [1.0, 0.0, 0.0, 0.0],
                directivity,
                on_axis: OnAxis {
                    spectrum: vec![Complex32::new(1.0, 0.0); n],
                    ref_distance_m: 1.0,
                    ref_input: 1.0,
                    ref_input_unit: "V".into(),
                },
                filter: FilterChain::default(),
                band_power,
            }],
            array: None,
            phase: PhaseKind::Complex,
        }
    }

    /// Complex response of the first transducer at native frequency point `f`, direction in
    /// the speaker frame. Coherent summation over transducers and array elements comes next.
    pub fn response(&self, f: usize, dir_local: [f32; 3]) -> Complex32 {
        let t = &self.transducers[0];
        t.on_axis.spectrum[f] * t.directivity.at(f, dir_local)
    }

    /// Band power relative to on-axis, from the precomputed table, for the ray tracer.
    pub fn band_power(&self, band: Band, dir_local: [f32; 3]) -> f32 {
        let t = &self.transducers[0];
        let b = &t.directivity;
        let (theta, phi) = grid_angles_deg(dir_local);
        let (ti, pi) = b.index(theta, phi);
        t.band_power.power[(band.index * b.n_theta + ti) * b.n_phi + pi]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_angles_follow_gll_convention() {
        let close = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4;
        assert!(close(grid_angles_deg([1.0, 0.0, 0.0]), (0.0, 0.0)));
        assert!(close(grid_angles_deg([0.0, 0.0, 1.0]), (90.0, 0.0))); // up
        assert!(close(grid_angles_deg([0.0, -1.0, 0.0]), (90.0, 90.0))); // right
        assert!(close(grid_angles_deg([0.0, 1.0, 0.0]), (90.0, 270.0))); // left
    }

    #[test]
    fn omni_is_flat() {
        let m = SpeakerModel::omni(BandSet::Octave63To8k);
        for dir in [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.3, -0.7]] {
            assert_eq!(m.response(3, dir), Complex32::new(1.0, 0.0));
            assert_eq!(m.band_power(Band { index: 2, centre_hz: 250.0 }, dir), 1.0);
        }
    }
}
