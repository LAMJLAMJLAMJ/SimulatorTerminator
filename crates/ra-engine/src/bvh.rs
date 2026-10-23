//! Bounding volume hierarchy over the scene's triangles, for ray casting.
//!
//! Built once per scene with a binned surface-area heuristic. Nodes and triangles
//! are flat arrays, so the coordinator can later build it once and send it to the
//! browser workers.

use ra_core::{SceneArrays, TRI_FLAG_CONTACT};

pub type Vec3 = [f64; 3];

/// Triangles per leaf at most.
const LEAF_SIZE: usize = 4;
const BINS: usize = 12;

#[derive(Clone, Copy, Debug)]
struct Node {
    lo: Vec3,
    hi: Vec3,
    /// Leaf: first triangle in `order`. Inner: index of the left child; the right one follows it.
    first: u32,
    /// Triangles in a leaf, 0 for an inner node.
    count: u32,
}

#[derive(Clone, Debug)]
pub struct Bvh {
    nodes: Vec<Node>,
    /// Scene triangle index per BVH slot.
    order: Vec<u32>,
    /// Corners per BVH slot, in metres.
    tris: Vec<[Vec3; 3]>,
}

/// The nearest surface along a ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Distance along the (unit) direction, m.
    pub t: f64,
    /// Scene triangle index.
    pub triangle: u32,
}

impl Bvh {
    /// Every triangle except contact faces, which sound never reaches.
    pub fn new(scene: &SceneArrays) -> Self {
        let v = &scene.vertices;
        let mut order: Vec<u32> = (0..scene.triangles.len() as u32)
            .filter(|&t| scene.tri_flags.as_ref().is_none_or(|f| f[t as usize] & TRI_FLAG_CONTACT == 0))
            .collect();
        let corners = |t: u32| scene.triangles[t as usize].map(|i| v[i as usize].map(f64::from));
        let centroid: Vec<Vec3> = (0..scene.triangles.len() as u32)
            .map(|t| {
                let [a, b, c] = corners(t);
                [0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0)
            })
            .collect();
        let mut nodes = Vec::with_capacity(2 * order.len().div_ceil(LEAF_SIZE).max(1));
        nodes.push(Node { lo: [0.0; 3], hi: [0.0; 3], first: 0, count: 0 });
        let n = order.len();
        build(&mut nodes, 0, &mut order, 0, n, &corners, &centroid);
        let tris = order.iter().map(|&t| corners(t)).collect();
        Bvh { nodes, order, tris }
    }

    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    /// The nearest hit with `0 < t < t_max`. `dir` must be a unit vector.
    pub fn intersect(&self, origin: Vec3, dir: Vec3, t_max: f64) -> Option<Hit> {
        let inv = dir.map(|d| 1.0 / d);
        let mut best: Option<Hit> = None;
        let mut limit = t_max;
        let mut stack = [0u32; 64];
        let mut top = 0;
        if self.tris.is_empty() || slab(&self.nodes[0], origin, inv, limit).is_none() {
            return None;
        }
        stack[0] = 0;
        top += 1;
        while top > 0 {
            top -= 1;
            let node = &self.nodes[stack[top] as usize];
            if slab(node, origin, inv, limit).is_none() {
                continue;
            }
            if node.count > 0 {
                for slot in node.first..node.first + node.count {
                    if let Some(t) = triangle(self.tris[slot as usize], origin, dir)
                        && t < limit
                    {
                        limit = t;
                        best = Some(Hit { t, triangle: self.order[slot as usize] });
                    }
                }
                continue;
            }
            // Visit the nearer child first.
            let (l, r) = (node.first, node.first + 1);
            let tl = slab(&self.nodes[l as usize], origin, inv, limit);
            let tr = slab(&self.nodes[r as usize], origin, inv, limit);
            match (tl, tr) {
                (Some(a), Some(b)) => {
                    let (near, far) = if a <= b { (l, r) } else { (r, l) };
                    stack[top] = far;
                    stack[top + 1] = near;
                    top += 2;
                }
                (Some(_), None) => {
                    stack[top] = l;
                    top += 1;
                }
                (None, Some(_)) => {
                    stack[top] = r;
                    top += 1;
                }
                (None, None) => {}
            }
        }
        best
    }

    /// Whether anything lies between `origin` and `origin + dir · t_max`.
    pub fn occluded(&self, origin: Vec3, dir: Vec3, t_max: f64) -> bool {
        self.intersect(origin, dir, t_max).is_some()
    }
}

#[allow(clippy::too_many_arguments)]
fn build(
    nodes: &mut Vec<Node>,
    at: usize,
    order: &mut [u32],
    start: usize,
    end: usize,
    corners: &impl Fn(u32) -> [Vec3; 3],
    centroid: &[Vec3],
) {
    let (lo, hi) = bounds(order[start..end].iter().flat_map(|&t| corners(t)));
    nodes[at].lo = lo;
    nodes[at].hi = hi;
    let n = end - start;
    let leaf = |nodes: &mut Vec<Node>| {
        nodes[at].first = start as u32;
        nodes[at].count = n as u32;
    };
    if n <= LEAF_SIZE {
        return leaf(nodes);
    }
    let (clo, chi) = bounds(order[start..end].iter().map(|&t| centroid[t as usize]));
    // Binned SAH: the split plane with the least area-weighted triangle count.
    let mut best: Option<(f64, usize, f64)> = None;
    for axis in 0..3 {
        let extent = chi[axis] - clo[axis];
        if extent <= 0.0 {
            continue;
        }
        let bin_of = |t: u32| {
            (((centroid[t as usize][axis] - clo[axis]) / extent * BINS as f64) as usize).min(BINS - 1)
        };
        let mut count = [0usize; BINS];
        let mut boxes = [(([f64::INFINITY; 3]), [f64::NEG_INFINITY; 3]); BINS];
        for &t in &order[start..end] {
            let b = bin_of(t);
            count[b] += 1;
            for p in corners(t) {
                for k in 0..3 {
                    boxes[b].0[k] = boxes[b].0[k].min(p[k]);
                    boxes[b].1[k] = boxes[b].1[k].max(p[k]);
                }
            }
        }
        let mut right_cost = [0.0; BINS];
        let (mut acc, mut n_acc) = (([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]), 0);
        for b in (1..BINS).rev() {
            acc = merge(acc, boxes[b]);
            n_acc += count[b];
            right_cost[b] = area(acc) * n_acc as f64;
        }
        let (mut acc, mut n_acc) = (([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]), 0);
        for b in 0..BINS - 1 {
            acc = merge(acc, boxes[b]);
            n_acc += count[b];
            if n_acc == 0 || n_acc == n {
                continue;
            }
            let cost = area(acc) * n_acc as f64 + right_cost[b + 1];
            if best.is_none_or(|(c, _, _)| cost < c) {
                best = Some((cost, axis, clo[axis] + extent * (b + 1) as f64 / BINS as f64));
            }
        }
    }
    let Some((cost, axis, plane)) = best else {
        return leaf(nodes);
    };
    if cost >= area((lo, hi)) * n as f64 && n <= 4 * LEAF_SIZE {
        return leaf(nodes);
    }
    let mut mid = start;
    for i in start..end {
        if centroid[order[i] as usize][axis] < plane {
            order.swap(i, mid);
            mid += 1;
        }
    }
    if mid == start || mid == end {
        mid = start + n / 2;
    }
    let left = nodes.len();
    nodes[at].first = left as u32;
    nodes[at].count = 0;
    nodes.push(Node { lo: [0.0; 3], hi: [0.0; 3], first: 0, count: 0 });
    nodes.push(Node { lo: [0.0; 3], hi: [0.0; 3], first: 0, count: 0 });
    build(nodes, left, order, start, mid, corners, centroid);
    build(nodes, left + 1, order, mid, end, corners, centroid);
}

fn bounds(points: impl Iterator<Item = Vec3>) -> (Vec3, Vec3) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in points {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

fn merge(a: (Vec3, Vec3), b: (Vec3, Vec3)) -> (Vec3, Vec3) {
    ([0, 1, 2].map(|k| a.0[k].min(b.0[k])), [0, 1, 2].map(|k| a.1[k].max(b.1[k])))
}

fn area((lo, hi): (Vec3, Vec3)) -> f64 {
    if lo[0] > hi[0] {
        return 0.0;
    }
    let d = [0, 1, 2].map(|k| hi[k] - lo[k]);
    d[0] * d[1] + d[1] * d[2] + d[2] * d[0]
}

/// Entry distance into the node's box if the ray meets it before `limit`.
fn slab(node: &Node, origin: Vec3, inv: Vec3, limit: f64) -> Option<f64> {
    let (mut t0, mut t1) = (0.0f64, limit);
    for k in 0..3 {
        let a = (node.lo[k] - origin[k]) * inv[k];
        let b = (node.hi[k] - origin[k]) * inv[k];
        let (near, far) = if a <= b { (a, b) } else { (b, a) };
        // NaN from 0 · inf (a ray in the box's plane) keeps the bounds as they are.
        if near > t0 {
            t0 = near;
        }
        if far < t1 {
            t1 = far;
        }
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

/// Möller–Trumbore, both sides. Hits closer than a micrometre are ignored, so a ray
/// leaving a surface does not hit it again.
fn triangle([a, b, c]: [Vec3; 3], origin: Vec3, dir: Vec3) -> Option<f64> {
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let p = cross(dir, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-14 {
        return None;
    }
    let inv = 1.0 / det;
    let s = sub(origin, a);
    let u = dot(s, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let v = dot(dir, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot(e2, q) * inv;
    (t > 1e-6).then_some(t)
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra_geom::shoebox;

    #[test]
    fn rays_from_inside_a_box_hit_its_walls() {
        let bvh = Bvh::new(&shoebox([6.0, 8.0, 3.0]));
        let o = [1.0, 2.0, 1.0];
        let h = bvh.intersect(o, [1.0, 0.0, 0.0], f64::INFINITY).unwrap();
        assert!((h.t - 5.0).abs() < 1e-9);
        let h = bvh.intersect(o, [0.0, 0.0, -1.0], f64::INFINITY).unwrap();
        assert!((h.t - 1.0).abs() < 1e-9);
        let d = [1.0f64, 1.0, 1.0].map(|x| x / 3f64.sqrt());
        let h = bvh.intersect(o, d, f64::INFINITY).unwrap();
        // Leaves through the ceiling at z = 3 after 2 m in z.
        assert!((h.t - 2.0 * 3f64.sqrt()).abs() < 1e-9, "{h:?}");
        assert!(bvh.intersect(o, [1.0, 0.0, 0.0], 4.0).is_none());
    }

    #[test]
    fn agrees_with_brute_force_on_many_triangles() {
        // A box subdivided into many slivers, and rays in fixed directions.
        let mut s = shoebox([6.0, 8.0, 3.0]);
        let mut rng = 1u64;
        let mut next = || {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (rng >> 11) as f64 / (1u64 << 53) as f64
        };
        for _ in 0..500 {
            let base = s.vertices.len() as u32;
            let p = [next() * 6.0, next() * 8.0, next() * 3.0];
            s.vertices.extend(
                [p, [p[0] + 0.3, p[1], p[2]], [p[0], p[1] + 0.2, p[2] + 0.1]].map(|q| q.map(|x| x as f32)),
            );
            s.triangles.push([base, base + 1, base + 2]);
            s.tri_group.push(0);
        }
        let bvh = Bvh::new(&s);
        for _ in 0..2000 {
            let o = [next() * 6.0, next() * 8.0, next() * 3.0];
            let z = 2.0 * next() - 1.0;
            let phi = 2.0 * std::f64::consts::PI * next();
            let r = (1.0 - z * z).sqrt();
            let d = [r * phi.cos(), r * phi.sin(), z];
            let brute = s
                .triangles
                .iter()
                .enumerate()
                .filter_map(|(i, t)| {
                    triangle(t.map(|i| s.vertices[i as usize].map(f64::from)), o, d).map(|t| (t, i))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            let hit = bvh.intersect(o, d, f64::INFINITY);
            assert_eq!(hit.map(|h| h.t), brute.map(|b| b.0));
        }
    }
}
