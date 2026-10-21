//! A solver that returns empty histograms. It exercises planning, chunking,
//! progress and merging end to end until the real ray tracer lands.

use crate::{
    ChunkResult, ChunkSpec, EnergyResponse, Estimate, PreparedScene, RunRequest, Solver, SolverOutput,
};

pub struct NullSolver;

const BIN_S: f32 = 0.001;

impl Solver for NullSolver {
    fn id(&self) -> &'static str {
        "null"
    }

    fn estimate(&self, scene: &PreparedScene, run: &RunRequest) -> Estimate {
        let chunks = self.plan(scene, run).len() as u32;
        let bins = (run.duration_s / BIN_S).ceil() as u64;
        let result_bytes = run.receivers.len() as u64 * run.bands.len() as u64 * bins * 4;
        Estimate {
            run_time_s: 0.0,
            peak_memory_bytes: result_bytes,
            result_bytes,
            chunks,
            prefer_native: None,
        }
    }

    fn run_chunk(&self, _scene: &PreparedScene, run: &RunRequest, _chunk: &ChunkSpec) -> ChunkResult {
        let n_bins = (run.duration_s / BIN_S).ceil() as usize;
        SolverOutput {
            receivers: run
                .receivers
                .iter()
                .map(|r| EnergyResponse {
                    receiver: r.clone(),
                    bin_s: BIN_S,
                    n_bins,
                    data: vec![0.0; run.bands.len() * n_bins],
                })
                .collect(),
            ..Default::default()
        }
    }

    fn merge(&self, _run: &RunRequest, parts: Vec<ChunkResult>) -> SolverOutput {
        let mut parts = parts.into_iter();
        let mut out = parts.next().unwrap_or_default();
        for part in parts {
            for (acc, add) in out.receivers.iter_mut().zip(part.receivers) {
                acc.data.iter_mut().zip(add.data).for_each(|(a, b)| *a += b);
            }
        }
        out
    }
}
