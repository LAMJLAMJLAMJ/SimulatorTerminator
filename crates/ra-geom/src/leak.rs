//! Leak test for models that are not one closed mesh, which is most real
//! acoustic models: separate surfaces, hung panels, small cracks between
//! render meshes. The surfaces are voxelised and air is flooded in from outside
//! the bounding box. Gaps narrower than a voxel are sealed by construction, so
//! trying several voxel sizes gives the width of the widest leak, and the
//! sealed air gives the room volume.

use ra_core::SceneArrays;
use serde::{Deserialize, Serialize};

use crate::{Vec3, sub};

/// Voxel sizes tried, smallest first, in metres.
pub const VOXEL_SIZES_M: [f64; 5] = [0.1, 0.2, 0.5, 1.0, 2.0];
/// Sizes that need more voxels than this are skipped (about 5 bytes each).
const MAX_VOXELS: usize = 30_000_000;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LeakReport {
    /// The point taken to be in the room's air, in metres relative to the scene origin:
    /// the middle of the plan, 2 m above the lowest point (or a quarter of the height).
    pub seed: [f64; 3],
    /// The voxel sizes tried, smallest first. Sizes too large for memory are left out.
    pub voxel_sizes_m: Vec<f64>,
    /// The smallest voxel size at which no air escapes. Gaps narrower than this are
    /// ignored by the test. `None` when the room leaks at every size tried.
    pub sealed_at_voxel_m: Option<f64>,
    /// Air volume at that voxel size, m³.
    pub air_volume_m3: Option<f64>,
    /// A point next to where the air escapes at the largest size that still leaked, in
    /// metres relative to the scene origin.
    pub leak_at: Option<[f64; 3]>,
}

const EMPTY: u8 = 0;
const SOLID: u8 = 1;
/// Outside air reached by a step in direction `REACHED + d`, d in 0..6.
const REACHED: u8 = 2;
const START: u8 = 8;
const ROOM: u8 = 9;

pub fn leak_test(scene: &SceneArrays) -> Option<LeakReport> {
    let v = &scene.vertices;
    if scene.triangles.is_empty() {
        return None;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &i in scene.triangles.iter().flatten() {
        for k in 0..3 {
            lo[k] = lo[k].min(f64::from(v[i as usize][k]));
            hi[k] = hi[k].max(f64::from(v[i as usize][k]));
        }
    }
    let seed = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, lo[2] + 2f64.min((hi[2] - lo[2]) / 4.0)];
    let mut report = LeakReport { seed, ..Default::default() };

    for h in VOXEL_SIZES_M {
        let n = [0, 1, 2].map(|k| ((hi[k] - lo[k]) / h).ceil() as usize + 3);
        let total = n[0] * n[1] * n[2];
        if total > MAX_VOXELS {
            continue;
        }
        report.voxel_sizes_m.push(h);
        let grid = Grid { lo, h, n };
        let mut state = vec![EMPTY; total];
        for t in &scene.triangles {
            let c = t.map(|i| v[i as usize].map(f64::from));
            grid.rasterise(c, &mut state);
        }

        let mut queue = Vec::new();
        state[0] = START;
        flood(&grid, &mut state, &mut queue, 0, |d| REACHED + d as u8);

        // Step up out of the surface if the seed lands in a solid voxel, but not out
        // through the top of the model: then the voxels are too coarse for this room.
        let mut s = grid.index(seed);
        while state[s] == SOLID && grid.centre(s + n[0] * n[1])[2] < hi[2] {
            s += n[0] * n[1];
        }
        if state[s] == SOLID {
            continue;
        }
        if state[s] == EMPTY {
            state[s] = ROOM;
            let air = 1 + flood(&grid, &mut state, &mut queue, s, |_| ROOM);
            report.sealed_at_voxel_m = Some(h);
            report.air_volume_m3 = Some(air as f64 * h * h * h);
            break;
        }
        report.leak_at = grid.leak_point(&state, s);
    }
    Some(report)
}

struct Grid {
    lo: Vec3,
    h: f64,
    n: [usize; 3],
}

impl Grid {
    /// Voxel holding `p`; the grid has one empty layer of padding all round.
    fn index(&self, p: Vec3) -> usize {
        let c = [0, 1, 2].map(|k| {
            (((p[k] - self.lo[k]) / self.h).floor() as i64 + 1).clamp(0, self.n[k] as i64 - 1) as usize
        });
        c[0] + self.n[0] * (c[1] + self.n[1] * c[2])
    }

    fn centre(&self, i: usize) -> Vec3 {
        let c = [i % self.n[0], (i / self.n[0]) % self.n[1], i / (self.n[0] * self.n[1])];
        [0, 1, 2].map(|k| self.lo[k] + (c[k] as f64 - 0.5) * self.h)
    }

    /// The six neighbours of `i` that exist, with their direction index.
    fn neighbours(&self, i: usize) -> impl Iterator<Item = (usize, usize)> {
        let [nx, ny, nz] = self.n;
        let (x, y, z) = (i % nx, (i / nx) % ny, i / (nx * ny));
        let steps = [
            (x + 1 < nx, i + 1),
            (x > 0, i.wrapping_sub(1)),
            (y + 1 < ny, i + nx),
            (y > 0, i.wrapping_sub(nx)),
            (z + 1 < nz, i + nx * ny),
            (z > 0, i.wrapping_sub(nx * ny)),
        ];
        steps.into_iter().enumerate().filter(|(_, (ok, _))| *ok).map(|(d, (_, j))| (j, d))
    }

    /// Marks every voxel the triangle passes through, sampling it at under half a voxel.
    fn rasterise(&self, [a, b, c]: [Vec3; 3], state: &mut [u8]) {
        let len = |p: Vec3, q: Vec3| {
            let d = sub(p, q);
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        };
        let longest = len(a, b).max(len(b, c)).max(len(c, a));
        let s = ((longest / (self.h * 0.45)).ceil() as usize).max(1);
        for i in 0..=s {
            for j in 0..=s - i {
                let (u, w) = (i as f64 / s as f64, j as f64 / s as f64);
                let p = [0, 1, 2].map(|k| a[k] * (1.0 - u - w) + b[k] * u + c[k] * w);
                state[self.index(p)] = SOLID;
            }
        }
    }

    fn is_padding(&self, i: usize) -> bool {
        let c = [i % self.n[0], (i / self.n[0]) % self.n[1], i / (self.n[0] * self.n[1])];
        (0..3).any(|k| c[k] == 0 || c[k] == self.n[k] - 1)
    }

    /// Follows the flood's path from `from` back towards the outside and returns where it
    /// leaves the model: the last voxel touching the model before the path reaches the
    /// padding round the bounding box, or the voxel just inside the padding.
    fn leak_point(&self, state: &[u8], from: usize) -> Option<Vec3> {
        let back = [1usize, 0, 3, 2, 5, 4];
        let mut i = from;
        let mut last_touch = None;
        let mut prev = None;
        while !self.is_padding(i) && state[i] != START {
            if self.neighbours(i).any(|(j, _)| state[j] == SOLID) {
                last_touch = Some(i);
            }
            prev = Some(i);
            // The step that reached i went in direction d; undo it.
            let d = usize::from(state[i].checked_sub(REACHED)?);
            i = self.neighbours(i).find(|&(_, e)| e == back[d])?.0;
        }
        last_touch.or(prev).map(|i| self.centre(i))
    }
}

/// Breadth-first flood through empty voxels from `start`, marking each with `mark(direction)`.
/// Returns the number of voxels reached, not counting `start`.
fn flood(
    grid: &Grid,
    state: &mut [u8],
    queue: &mut Vec<u32>,
    start: usize,
    mark: impl Fn(usize) -> u8,
) -> usize {
    queue.clear();
    queue.push(start as u32);
    let mut head = 0;
    while head < queue.len() {
        let i = queue[head] as usize;
        head += 1;
        for (j, d) in grid.neighbours(i) {
            if state[j] == EMPTY {
                state[j] = mark(d);
                queue.push(j as u32);
            }
        }
    }
    queue.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shoebox;

    #[test]
    fn closed_box_is_sealed_at_the_finest_size() {
        let r = leak_test(&shoebox([6.0, 8.0, 3.0])).unwrap();
        assert_eq!(r.sealed_at_voxel_m, Some(0.1));
        // The voxelised air is a little smaller than the box: the walls take up a voxel.
        let air = r.air_volume_m3.unwrap();
        assert!(air > 0.8 * 144.0 && air < 144.0, "{air}");
        assert_eq!(r.leak_at, None);
    }

    #[test]
    fn a_missing_wall_leaks_at_every_size() {
        let mut s = shoebox([6.0, 8.0, 3.0]);
        s.triangles.drain(10..12); // x = max
        s.tri_group.drain(10..12);
        let r = leak_test(&s).unwrap();
        assert_eq!(r.sealed_at_voxel_m, None);
        let at = r.leak_at.unwrap();
        assert!((at[0] - 6.0).abs() < 2.5, "{at:?}");
    }

    #[test]
    fn a_small_gap_is_sealed_by_larger_voxels() {
        // Lower the top of the x = max wall to 2.7 m: a 0.3 m slit under the ceiling.
        let mut s = shoebox([6.0, 8.0, 3.0]);
        s.vertices.extend([[6.0, 8.0, 2.7], [6.0, 0.0, 2.7]]);
        for t in &mut s.triangles[10..12] {
            for v in t.iter_mut() {
                *v = match *v {
                    6 => 8,
                    5 => 9,
                    x => x,
                };
            }
        }
        let r = leak_test(&s).unwrap();
        let sealed = r.sealed_at_voxel_m.unwrap();
        assert!(sealed > 0.1 && sealed <= 0.5, "{r:?}");
        let at = r.leak_at.unwrap();
        assert!((at[0] - 6.0).abs() < 0.5 && at[2] > 2.0, "{at:?}");
    }
}
