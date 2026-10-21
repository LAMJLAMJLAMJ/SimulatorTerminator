//! Solvers. Owned by the ray-engine thread.
//!
//! A run is planned as a list of chunks, each a fixed block of rays with its own
//! seed. Native builds run chunks on a thread pool, the browser on a pool of
//! workers; results are merged in chunk order, so the numbers never depend on
//! how many cores ran them.

pub mod null;

use ra_core::{BandSet, Material, SceneArrays, SceneMeta};
use ra_speaker::SpeakerModel;
use serde::{Deserialize, Serialize};

/// Rays per chunk unless a solver chooses otherwise.
pub const DEFAULT_CHUNK_RAYS: u64 = 16_384;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RunRequest {
    pub solver: String,
    pub bands: BandSet,
    pub max_order: u32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub n_rays: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub seed: u64,
    /// Per band, 1/m. `None` = ISO 9613-1 from temperature and humidity.
    #[serde(default)]
    pub air_absorption_per_m: Option<Vec<f32>>,
    /// Length of the histograms, seconds.
    pub duration_s: f32,
    pub receivers: Vec<String>,
    #[serde(default)]
    pub grids: Vec<String>,
    /// Receivers that also get ambisonic responses for listening.
    #[serde(default)]
    pub listening: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Estimate {
    pub run_time_s: f32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub peak_memory_bytes: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub result_bytes: u64,
    pub chunks: u32,
    /// Set when the run is better done in the Mac app, with the reason.
    pub prefer_native: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkSpec {
    pub index: u32,
    pub first_ray: u64,
    pub n_rays: u64,
    pub seed: u64,
}

/// Everything a solver needs, built once per scene hash.
#[derive(Clone, Debug)]
pub struct PreparedScene {
    pub meta: SceneMeta,
    pub arrays: SceneArrays,
    /// Indexed by group, from the group's assigned acoustic material.
    pub materials: Vec<Material>,
    pub speakers: Vec<SpeakerModel>,
}

/// Energy histogram at one receiver, `[bands][bins]` row-major.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnergyResponse {
    pub receiver: String,
    pub bin_s: f32,
    pub n_bins: usize,
    #[serde(skip)]
    pub data: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IsmPath {
    pub source: String,
    pub receiver: String,
    pub order: u32,
    pub delay_s: f64,
    /// Pressure amplitude per band, relative to 1 m on-axis.
    pub amplitude: Vec<f32>,
    /// Triangle group of each reflection, in order.
    pub walls: Vec<u16>,
}

/// Per grid point and band: direct, 0-50, 50-80, 80+ ms and total, as 64-bit fixed point,
/// so partial sums add exactly in any order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GridAccumulator {
    pub grid: String,
    pub n_points: usize,
    #[serde(skip)]
    pub windows: Vec<u64>,
}

pub const GRID_WINDOWS: usize = 5;

/// Ambisonic response, AmbiX (ACN, SN3D), `[channels][samples]` row-major.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AmbiResponse {
    pub receiver: String,
    pub order: u32,
    pub sample_rate: u32,
    #[serde(skip)]
    pub data: Vec<f32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SolverOutput {
    pub receivers: Vec<EnergyResponse>,
    pub image_sources: Vec<IsmPath>,
    /// `[bands][bins]`, energy left in the ray population.
    #[serde(skip)]
    pub room_decay: Vec<f32>,
    pub mean_free_path_m: f32,
    pub leak_fraction: f32,
    pub grids: Vec<GridAccumulator>,
    pub directional: Vec<AmbiResponse>,
}

/// One chunk's share of the output. Merged in chunk order.
pub type ChunkResult = SolverOutput;

pub trait Solver: Send + Sync {
    /// "ism-rt", later "fdtd-metal".
    fn id(&self) -> &'static str;
    fn estimate(&self, scene: &PreparedScene, run: &RunRequest) -> Estimate;
    fn plan(&self, scene: &PreparedScene, run: &RunRequest) -> Vec<ChunkSpec> {
        let _ = scene;
        plan_chunks(run.n_rays, DEFAULT_CHUNK_RAYS, run.seed)
    }
    fn run_chunk(&self, scene: &PreparedScene, run: &RunRequest, chunk: &ChunkSpec) -> ChunkResult;
    /// `parts` are in chunk order.
    fn merge(&self, run: &RunRequest, parts: Vec<ChunkResult>) -> SolverOutput;
}

/// Splits `n_rays` into chunks of `chunk_rays`, each seeded from the run seed and its index.
pub fn plan_chunks(n_rays: u64, chunk_rays: u64, run_seed: u64) -> Vec<ChunkSpec> {
    let chunk_rays = chunk_rays.max(1);
    (0..n_rays.div_ceil(chunk_rays))
        .map(|i| {
            let first_ray = i * chunk_rays;
            ChunkSpec {
                index: i as u32,
                first_ray,
                n_rays: chunk_rays.min(n_rays - first_ray),
                seed: chunk_seed(run_seed, i),
            }
        })
        .collect()
}

/// SplitMix64 of the run seed and chunk index: independent streams, same on every platform.
pub fn chunk_seed(run_seed: u64, index: u64) -> u64 {
    let mut z = run_seed ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// All solvers this build carries.
pub fn solvers() -> Vec<Box<dyn Solver>> {
    vec![Box::new(null::NullSolver)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_cover_every_ray_once() {
        let plan = plan_chunks(100_000, DEFAULT_CHUNK_RAYS, 7);
        assert_eq!(plan.len(), 7);
        assert_eq!(plan.iter().map(|c| c.n_rays).sum::<u64>(), 100_000);
        assert!(plan.windows(2).all(|w| w[0].first_ray + w[0].n_rays == w[1].first_ray));
    }

    #[test]
    fn seeds_differ_and_repeat() {
        let a = plan_chunks(50_000, 1000, 42);
        let b = plan_chunks(50_000, 1000, 42);
        assert_eq!(a, b);
        let mut seeds: Vec<u64> = a.iter().map(|c| c.seed).collect();
        seeds.sort_unstable();
        seeds.dedup();
        assert_eq!(seeds.len(), a.len());
    }
}
