//! Leak test for models that are not one closed mesh, which is most real
//! acoustic models: separate surfaces, hung panels, small cracks between
//! render meshes. The surfaces are voxelised and air is flooded in from outside
//! the bounding box. Gaps narrower than a voxel are sealed by construction, so
//! trying several voxel sizes gives the width of the widest leak, and the
//! sealed air gives the room volume.

use ra_core::SceneArrays;
use serde::{Deserialize, Serialize};

use crate::{Vec3, cross, sub};

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
    let (lo, hi) = bounds(scene)?;
    let seed = seed(lo, hi);
    let mut report = LeakReport { seed, ..Default::default() };
    for h in VOXEL_SIZES_M {
        match fill(scene, lo, hi, h) {
            Fill::TooLarge => continue,
            Fill::SeedInSurface => report.voxel_sizes_m.push(h),
            Fill::Sealed { air_voxels, .. } => {
                report.voxel_sizes_m.push(h);
                report.sealed_at_voxel_m = Some(h);
                report.air_volume_m3 = Some(air_voxels as f64 * h * h * h);
                break;
            }
            Fill::Leaked { grid, state, from } => {
                report.voxel_sizes_m.push(h);
                report.leak_at = grid.leak_point(&state, from);
            }
        }
    }
    Some(report)
}

/// Where the room's air touches the surfaces, from the flood at `voxel_m`, the size the
/// leak test found sealed. `None` when the room leaks at that size.
pub struct RoomContact {
    /// Per triangle, m²: the area for a surface with room air on one side, twice that for a
    /// panel with air on both, 0 for faces outside the room or inside a wall.
    pub facing_area_m2: Vec<f64>,
    /// The flooded air plus half a voxel over the room's surfaces, m³: the voxels on a surface
    /// are half in the room on average.
    pub volume_m3: f64,
}

pub fn room_contact(scene: &SceneArrays, voxel_m: f64) -> Option<RoomContact> {
    let (lo, hi) = bounds(scene)?;
    let Fill::Sealed { grid, state, air_voxels } = fill(scene, lo, hi, voxel_m) else {
        return None;
    };
    let h = voxel_m;
    let v = &scene.vertices;
    let facing_area_m2: Vec<f64> = scene
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| v[i as usize].map(f64::from));
            let n = cross(sub(b, a), sub(c, a));
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len == 0.0 {
                return 0.0;
            }
            let n = n.map(|x| x / len);
            // Sample away from the edges: next to a corner of the room the step along the
            // normal runs inside the other wall. Shrink the triangle about its incentre so
            // every sample is 1.5 voxels from an edge, or use the incentre alone.
            let [la, lb, lc] = [len3(sub(b, c)), len3(sub(c, a)), len3(sub(a, b))];
            let perimeter = la + lb + lc;
            let centre = [0, 1, 2].map(|i| (la * a[i] + lb * b[i] + lc * c[i]) / perimeter);
            let shrink = (1.0 - 1.5 * h / (len / perimeter)).max(0.0);
            let inset = [a, b, c].map(|p| [0, 1, 2].map(|i| centre[i] + shrink * (p[i] - centre[i])));
            // Per sample, each side is room air, other air, or unknown when every step
            // stays inside surfaces; unknown samples are left out.
            let (mut sides, mut known) = (0usize, 0usize);
            grid.each_sample(inset, 1.0, |p| {
                let mut decided = false;
                for side in [1.0, -1.0] {
                    let off = [0.6, 1.2, 1.8]
                        .iter()
                        .map(|k| state[grid.index([0, 1, 2].map(|i| p[i] + side * k * h * n[i]))])
                        .find(|&s| s != SOLID);
                    decided |= off.is_some();
                    sides += usize::from(off == Some(ROOM));
                }
                known += usize::from(decided);
            });
            // A face buried in other surfaces at this voxel size counts once.
            if known == 0 {
                return 0.5 * len;
            }
            0.5 * len * sides as f64 / known as f64
        })
        .collect();
    let area: f64 = facing_area_m2.iter().sum();
    Some(RoomContact { facing_area_m2, volume_m3: air_voxels as f64 * h * h * h + 0.5 * h * area })
}

fn len3(d: Vec3) -> f64 {
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

fn bounds(scene: &SceneArrays) -> Option<(Vec3, Vec3)> {
    if scene.triangles.is_empty() {
        return None;
    }
    let v = &scene.vertices;
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &i in scene.triangles.iter().flatten() {
        for k in 0..3 {
            lo[k] = lo[k].min(f64::from(v[i as usize][k]));
            hi[k] = hi[k].max(f64::from(v[i as usize][k]));
        }
    }
    Some((lo, hi))
}

/// The middle of the plan, 2 m above the lowest point (or a quarter of the height).
fn seed(lo: Vec3, hi: Vec3) -> Vec3 {
    [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, lo[2] + 2f64.min((hi[2] - lo[2]) / 4.0)]
}

enum Fill {
    TooLarge,
    /// The seed is inside a surface all the way up: the voxels are too coarse for the room.
    SeedInSurface,
    Sealed {
        grid: Grid,
        state: Vec<u8>,
        air_voxels: usize,
    },
    Leaked {
        grid: Grid,
        state: Vec<u8>,
        from: usize,
    },
}

/// Voxelises the surfaces at size `h`, floods the outside, then the room from the seed.
fn fill(scene: &SceneArrays, lo: Vec3, hi: Vec3, h: f64) -> Fill {
    let v = &scene.vertices;
    let n = [0, 1, 2].map(|k| ((hi[k] - lo[k]) / h).ceil() as usize + 3);
    let total = n[0] * n[1] * n[2];
    if total > MAX_VOXELS {
        return Fill::TooLarge;
    }
    let grid = Grid { lo, h, n };
    let mut state = vec![EMPTY; total];
    for t in &scene.triangles {
        let c = t.map(|i| v[i as usize].map(f64::from));
        grid.each_sample(c, 0.45, |p| state[grid.index(p)] = SOLID);
    }

    let mut queue = Vec::new();
    state[0] = START;
    flood(&grid, &mut state, &mut queue, 0, |d| REACHED + d as u8);

    // Step up out of the surface if the seed lands in a solid voxel, but not out
    // through the top of the model: then the voxels are too coarse for this room.
    let mut s = grid.index(seed(lo, hi));
    while state[s] == SOLID && grid.centre(s + n[0] * n[1])[2] < hi[2] {
        s += n[0] * n[1];
    }
    if state[s] == SOLID {
        return Fill::SeedInSurface;
    }
    if state[s] == EMPTY {
        state[s] = ROOM;
        let air_voxels = 1 + flood(&grid, &mut state, &mut queue, s, |_| ROOM);
        return Fill::Sealed { grid, state, air_voxels };
    }
    Fill::Leaked { grid, state, from: s }
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

    /// Points spread over the triangle, under `spacing` voxels apart.
    fn each_sample(&self, [a, b, c]: [Vec3; 3], spacing: f64, mut f: impl FnMut(Vec3)) {
        let len = |p: Vec3, q: Vec3| {
            let d = sub(p, q);
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        };
        let longest = len(a, b).max(len(b, c)).max(len(c, a));
        let s = ((longest / (self.h * spacing)).ceil() as usize).max(1);
        for i in 0..=s {
            for j in 0..=s - i {
                let (u, w) = (i as f64 / s as f64, j as f64 / s as f64);
                f([0, 1, 2].map(|k| a[k] * (1.0 - u - w) + b[k] * u + c[k] * w));
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
    use crate::{shoebox, tri_area};

    fn facing(s: &SceneArrays, h: f64) -> (f64, f64) {
        let c = room_contact(s, h).unwrap();
        (c.facing_area_m2.iter().sum(), c.volume_m3)
    }

    #[test]
    fn room_contact_of_a_box() {
        // Sizes off the voxel grid, as real rooms are.
        let [x, y, z] = [6.03, 8.07, 3.01];
        let (s, v) = (2.0 * (x * y + y * z + z * x), x * y * z);
        for h in [0.1, 0.2, 0.5] {
            let (area, volume) = facing(&shoebox([x as f32, y as f32, z as f32]), h);
            assert!((area / s - 1.0).abs() < 0.01, "{h}: {area} for {s}");
            assert!((volume / v - 1.0).abs() < 0.04, "{h}: {volume} for {v}");
        }
    }

    #[test]
    fn hung_panel_counts_twice_and_outside_faces_not_at_all() {
        let mut s = shoebox([6.0, 8.0, 3.0]);
        let base = s.vertices.len() as u32;
        // A 2 x 2 m panel hung at 2 m, and a 4 x 4 m slab 1 m outside the room.
        s.vertices.extend([[2.0, 3.0, 2.0], [4.0, 3.0, 2.0], [4.0, 5.0, 2.0], [2.0, 5.0, 2.0]]);
        s.vertices.extend([[-1.0, 0.0, 0.0], [-1.0, 4.0, 0.0], [-1.0, 4.0, 3.0], [-1.0, 0.0, 3.0]]);
        for q in [base, base + 4] {
            s.triangles.extend([[q, q + 1, q + 2], [q, q + 2, q + 3]]);
            s.tri_group.extend([0, 0]);
        }
        let c = room_contact(&s, 0.1).unwrap();
        let n = s.triangles.len();
        let panel: f64 = c.facing_area_m2[n - 4..n - 2].iter().sum();
        let outside: f64 = c.facing_area_m2[n - 2..].iter().sum();
        assert!((panel - 8.0).abs() < 0.4, "{panel}");
        assert_eq!(outside, 0.0);
        let room: f64 = c.facing_area_m2[..n - 4].iter().sum();
        let shell: f64 = s.triangles[..n - 4]
            .iter()
            .map(|t| tri_area(t.map(|i| s.vertices[i as usize].map(f64::from))))
            .sum();
        assert!((room / shell - 1.0).abs() < 0.02, "{room} {shell}");
    }

    #[test]
    fn no_room_contact_when_leaking() {
        let mut s = shoebox([6.0, 8.0, 3.0]);
        s.triangles.drain(10..12);
        s.tri_group.drain(10..12);
        assert!(room_contact(&s, 0.1).is_none());
    }

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
