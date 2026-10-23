//! Stochastic ray tracer: energy histograms at receiver spheres.
//!
//! Each source sends `n_rays` rays, each carrying 1/n_rays of its energy in every
//! band. At a surface a ray loses its absorption and leaves in the mirror direction or,
//! with the surface's scattering coefficient as probability, in a cosine-weighted
//! random one (both branches reweighted per band, so one ray serves all bands).
//! Every ray segment that crosses a receiver sphere adds its energy times the chord
//! length over the sphere's volume, which gives 1/(4πd²) for the direct sound on
//! average; the direct sound itself is added exactly, once, with a visibility test.
//! Rays are seeded by source and ray index, so the result does not depend on how
//! the run is split into chunks.

use crate::bvh::{Vec3, cross, dot, sub};
use crate::{
    ChunkResult, ChunkSpec, DEFAULT_CHUNK_RAYS, EnergyResponse, Estimate, PreparedScene, RunRequest, Solver,
    SolverOutput, chunk_seed, plan_chunks,
};

pub struct RayTracer;

/// Histogram bin width, s.
pub const BIN_S: f64 = 0.001;
/// A ray stops when every band has fallen 90 dB below its start.
const ENERGY_FLOOR: f64 = 1e-9;

impl Solver for RayTracer {
    fn id(&self) -> &'static str {
        "rt"
    }

    fn estimate(&self, scene: &PreparedScene, run: &RunRequest) -> Estimate {
        let chunks = self.plan(scene, run).len() as u32;
        let n_bins = (f64::from(run.duration_s) / BIN_S).ceil() as u64;
        let result_bytes = run.receivers.len() as u64 * run.bands.len() as u64 * n_bins * 4;
        // About c·T / (mean free path) reflections per ray, a few microseconds each.
        let (lo, hi) = extent(scene);
        let d = sub(hi, lo);
        let mfp = 4.0 * d[0] * d[1] * d[2] / (2.0 * (d[0] * d[1] + d[1] * d[2] + d[2] * d[0])).max(1e-9);
        let reflections = run.air.speed_of_sound() * f64::from(run.duration_s) / mfp.max(0.1);
        let rays = run.n_rays as f64 * scene.sources.len() as f64;
        Estimate {
            run_time_s: (rays * reflections * 2e-6) as f32,
            peak_memory_bytes: result_bytes + scene.arrays.triangles.len() as u64 * 250,
            result_bytes,
            chunks,
            prefer_native: None,
        }
    }

    fn run_chunk(&self, scene: &PreparedScene, run: &RunRequest, chunk: &ChunkSpec) -> ChunkResult {
        let bands = run.bands.len();
        let n_bins = (f64::from(run.duration_s) / BIN_S).ceil() as usize;
        let c = run.air.speed_of_sound();
        let air: Vec<f64> = match &run.air_absorption_per_m {
            Some(m) => m.iter().map(|&x| f64::from(x)).collect(),
            None => run
                .bands
                .centres_hz()
                .iter()
                .map(|&f| run.air.energy_attenuation_per_m(f64::from(f)))
                .collect(),
        };
        let receivers: Vec<Vec3> = run
            .receivers
            .iter()
            .filter_map(|id| scene.receivers.iter().find(|r| &r.id == id).map(|r| r.position))
            .collect();
        let r = run.receiver_radius_m;
        let sphere = 4.0 / 3.0 * std::f64::consts::PI * r * r * r;
        let max_path = c * f64::from(run.duration_s);
        let mut hist = vec![vec![0.0f64; bands * n_bins]; receivers.len()];
        let deposit = |hist: &mut Vec<Vec<f64>>, k: usize, dist: f64, energy: &[f64]| {
            let bin = (dist / c / BIN_S) as usize;
            if bin < n_bins {
                for b in 0..bands {
                    hist[k][b * n_bins + bin] += energy[b] * libm::exp(-air[b] * dist);
                }
            }
        };

        let (mut escaped, mut segments, mut length) = (0u64, 0u64, 0.0f64);
        let start = 1.0 / run.n_rays as f64;
        let mut energy = vec![0.0; bands];
        for (si, source) in scene.sources.iter().enumerate() {
            let source_seed = chunk_seed(run.seed, si as u64 ^ 0x5EED_0000_0000);
            for ray in chunk.first_ray..chunk.first_ray + chunk.n_rays {
                let mut rng = Rng::new(chunk_seed(source_seed, ray));
                energy.fill(start);
                let mut pos = source.position;
                let mut dir = rng.sphere();
                let mut path = 0.0;
                let mut order = 0u32;
                loop {
                    let hit = scene.bvh.intersect(pos, dir, max_path - path);
                    let seg = hit.map_or(max_path - path, |h| h.t);
                    // Order 0 is the direct sound, added exactly below.
                    if order > 0 {
                        for (k, &rec) in receivers.iter().enumerate() {
                            let w = sub(rec, pos);
                            let s = dot(w, dir);
                            let d2 = dot(w, w) - s * s;
                            if d2 >= r * r {
                                continue;
                            }
                            let half = (r * r - d2).sqrt();
                            let (s0, s1) = ((s - half).max(0.0), (s + half).min(seg));
                            if s1 > s0 {
                                let weight = (s1 - s0) / sphere;
                                let e: Vec<f64> = energy.iter().map(|e| e * weight).collect();
                                deposit(&mut hist, k, path + 0.5 * (s0 + s1), &e);
                            }
                        }
                    }
                    let Some(hit) = hit else {
                        if path + seg < max_path - 1e-9 {
                            escaped += 1;
                        }
                        break;
                    };
                    segments += 1;
                    length += hit.t;
                    path += hit.t;
                    pos = [0, 1, 2].map(|k| pos[k] + dir[k] * hit.t);

                    let tri = hit.triangle as usize;
                    let Some(material) = scene.materials.get(usize::from(scene.arrays.tri_group[tri])) else {
                        break;
                    };
                    let [a, b, cc] =
                        scene.arrays.triangles[tri].map(|i| scene.arrays.vertices[i as usize].map(f64::from));
                    let n = normalise(cross(sub(b, a), sub(cc, a)));
                    // The side the ray came from.
                    let facing = if dot(dir, n) > 0.0 { n.map(|x| -x) } else { n };
                    let p: f64 =
                        material.scattering.iter().map(|&s| f64::from(s)).sum::<f64>() / bands as f64;
                    let diffuse = p > 0.0 && (p >= 1.0 || rng.uniform() < p);
                    for (bi, e) in energy.iter_mut().enumerate() {
                        let s = f64::from(material.scattering[bi]);
                        let share = if diffuse { s / p } else { (1.0 - s) / (1.0 - p) };
                        *e *= (1.0 - f64::from(material.absorption[bi])) * share;
                    }
                    dir = if diffuse {
                        rng.lambert(facing)
                    } else {
                        let k = 2.0 * dot(dir, n);
                        normalise([0, 1, 2].map(|i| dir[i] - k * n[i]))
                    };
                    order += 1;
                    if energy.iter().all(|&e| e < ENERGY_FLOOR * start) {
                        break;
                    }
                }
            }
        }

        // The direct sound, once per run.
        if chunk.index == 0 {
            for source in &scene.sources {
                for (k, &rec) in receivers.iter().enumerate() {
                    let w = sub(rec, source.position);
                    let d = dot(w, w).sqrt();
                    if d == 0.0 || scene.bvh.occluded(source.position, w.map(|x| x / d), d) {
                        continue;
                    }
                    let e = vec![1.0 / (4.0 * std::f64::consts::PI * d.max(r) * d.max(r)); bands];
                    deposit(&mut hist, k, d, &e);
                }
            }
        }

        let rays = (chunk.n_rays * scene.sources.len() as u64).max(1);
        SolverOutput {
            receivers: run
                .receivers
                .iter()
                .filter(|id| scene.receivers.iter().any(|r| &r.id == *id))
                .zip(hist)
                .map(|(id, h)| EnergyResponse {
                    receiver: id.clone(),
                    bin_s: BIN_S as f32,
                    n_bins,
                    data: h.into_iter().map(|x| x as f32).collect(),
                })
                .collect(),
            mean_free_path_m: if segments > 0 { (length / segments as f64) as f32 } else { 0.0 },
            leak_fraction: (escaped as f64 / rays as f64) as f32,
            ..Default::default()
        }
    }

    fn merge(&self, run: &RunRequest, parts: Vec<ChunkResult>) -> SolverOutput {
        // Chunk sizes, to weight the per-chunk means.
        let weights: Vec<f64> =
            plan_chunks(run.n_rays, DEFAULT_CHUNK_RAYS, run.seed).iter().map(|c| c.n_rays as f64).collect();
        let total: f64 = weights.iter().take(parts.len()).sum::<f64>().max(1.0);
        let (mut mfp, mut leak) = (0.0, 0.0);
        let mut parts = parts.into_iter().enumerate();
        let Some((_, mut out)) = parts.next() else {
            return SolverOutput::default();
        };
        mfp += f64::from(out.mean_free_path_m) * weights.first().copied().unwrap_or(0.0);
        leak += f64::from(out.leak_fraction) * weights.first().copied().unwrap_or(0.0);
        for (i, part) in parts {
            let w = weights.get(i).copied().unwrap_or(0.0);
            mfp += f64::from(part.mean_free_path_m) * w;
            leak += f64::from(part.leak_fraction) * w;
            for (acc, add) in out.receivers.iter_mut().zip(part.receivers) {
                acc.data.iter_mut().zip(add.data).for_each(|(a, b)| *a += b);
            }
        }
        out.mean_free_path_m = (mfp / total) as f32;
        out.leak_fraction = (leak / total) as f32;
        out
    }
}

fn extent(scene: &PreparedScene) -> (Vec3, Vec3) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for v in &scene.arrays.vertices {
        for k in 0..3 {
            lo[k] = lo[k].min(f64::from(v[k]));
            hi[k] = hi[k].max(f64::from(v[k]));
        }
    }
    (lo, hi)
}

fn normalise(v: Vec3) -> Vec3 {
    let l = dot(v, v).sqrt();
    v.map(|x| x / l)
}

/// xoshiro256**, seeded through SplitMix64: the same numbers on every platform.
struct Rng([u64; 4]);

impl Rng {
    fn new(seed: u64) -> Self {
        let mut z = seed;
        Rng([0; 4].map(|_| {
            z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            x ^ (x >> 31)
        }))
    }

    fn next(&mut self) -> u64 {
        let s = &mut self.0;
        let out = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        out
    }

    /// Uniform in [0, 1).
    fn uniform(&mut self) -> f64 {
        (self.next() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform direction.
    fn sphere(&mut self) -> Vec3 {
        let z = 2.0 * self.uniform() - 1.0;
        let phi = 2.0 * std::f64::consts::PI * self.uniform();
        let r = (1.0 - z * z).max(0.0).sqrt();
        [r * libm::cos(phi), r * libm::sin(phi), z]
    }

    /// Cosine-weighted direction around the unit normal `n` (Lambert).
    fn lambert(&mut self, n: Vec3) -> Vec3 {
        let u = self.uniform();
        let phi = 2.0 * std::f64::consts::PI * self.uniform();
        let (sin_t, cos_t) = (u.sqrt(), (1.0 - u).sqrt());
        let helper = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let t = normalise(cross(n, helper));
        let b = cross(n, t);
        let (x, y) = (sin_t * libm::cos(phi), sin_t * libm::sin(phi));
        normalise([0, 1, 2].map(|k| x * t[k] + y * b[k] + cos_t * n[k]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Point;
    use ra_core::{BandSet, Group, Material, SceneMeta};

    fn shoebox_scene(alpha: f32, scattering: f32) -> PreparedScene {
        let arrays = ra_geom::shoebox([10.0, 8.0, 4.0]);
        let meta = SceneMeta {
            origin: [0.0; 3],
            groups: vec![Group {
                key: "Room".into(),
                layer_path: "Room".into(),
                material: None,
                label: "Room".into(),
                acoustic_material: Some("M".into()),
            }],
            importer: "test".into(),
        };
        let m = Material {
            name: "M".into(),
            absorption: vec![alpha; 8],
            scattering: vec![scattering; 8],
            source: String::new(),
        };
        PreparedScene::new(
            meta,
            arrays,
            vec![m],
            vec![Point { id: "S".into(), position: [2.5, 3.0, 1.5], from_model: false }],
            vec![Point { id: "R".into(), position: [7.0, 5.0, 1.2], from_model: false }],
        )
    }

    fn request(n_rays: u64) -> RunRequest {
        RunRequest {
            solver: "rt".into(),
            bands: BandSet::Octave63To8k,
            max_order: 0,
            n_rays,
            seed: 7,
            air_absorption_per_m: Some(vec![0.0; 8]),
            air: Default::default(),
            receiver_radius_m: 0.5,
            duration_s: 1.5,
            receivers: vec!["R".into()],
            grids: vec![],
            listening: vec![],
        }
    }

    fn run(scene: &PreparedScene, run: &RunRequest) -> SolverOutput {
        let rt = RayTracer;
        let parts = rt.plan(scene, run).iter().map(|c| rt.run_chunk(scene, run, c)).collect();
        rt.merge(run, parts)
    }

    #[test]
    fn energy_conservation_without_absorption() {
        // With nothing absorbed, the late energy density settles at E / V: the
        // histogram per bin is E · c · bin / V for E = 1.
        let scene = shoebox_scene(0.0, 0.5);
        let mut req = request(4000);
        req.duration_s = 0.3;
        let out = run(&scene, &req);
        let h = &out.receivers[0].data[4 * 300..5 * 300];
        let late: f64 = h[100..300].iter().map(|&x| f64::from(x)).sum::<f64>() / 200.0;
        let expect = 343.2 * BIN_S / 320.0;
        assert!((late / expect - 1.0).abs() < 0.1, "{late} vs {expect}");
        assert_eq!(out.leak_fraction, 0.0);
        // 4V/S = 4 · 320 / 304.
        assert!(
            (f64::from(out.mean_free_path_m) / (1280.0 / 304.0) - 1.0).abs() < 0.05,
            "{}",
            out.mean_free_path_m
        );
    }

    #[test]
    fn direct_sound_is_exact() {
        let scene = shoebox_scene(1.0, 0.0);
        let out = run(&scene, &request(10));
        let d = (4.5f64 * 4.5 + 2.0 * 2.0 + 0.3 * 0.3).sqrt();
        let bin = (d / 343.2 / BIN_S) as usize;
        let n = out.receivers[0].n_bins;
        let got = f64::from(out.receivers[0].data[4 * n + bin]);
        assert!((got * 4.0 * std::f64::consts::PI * d * d - 1.0).abs() < 1e-5, "{got}");
    }

    #[test]
    fn chunking_does_not_change_the_result() {
        let scene = shoebox_scene(0.2, 0.3);
        let req = request(3 * DEFAULT_CHUNK_RAYS / 2 + 17);
        let rt = RayTracer;
        let whole =
            rt.run_chunk(&scene, &req, &ChunkSpec { index: 0, first_ray: 0, n_rays: req.n_rays, seed: 0 });
        let split = run(&scene, &req);
        for (a, b) in whole.receivers[0].data.iter().zip(&split.receivers[0].data) {
            assert!((a - b).abs() <= 1e-6 * a.abs().max(1e-12), "{a} {b}");
        }
    }
}
