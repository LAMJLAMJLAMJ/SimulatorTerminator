//! Mechanical clean-up of an imported mesh. Nothing here changes what the model
//! shows: vertices closer than the weld tolerance become one, zero-area and
//! duplicate triangles go, edges that meet geometrically but not vertex to
//! vertex are split, and each connected part is wound consistently. Faces that
//! lie flat against another part are flagged, not removed.

use std::collections::{HashMap, HashSet};

use ra_core::{SceneArrays, TRI_FLAG_CONTACT};

use crate::{DEGENERATE_AREA_M2, Vec3, cross, dot, sub, tri_area};

/// Vertices closer than this, in metres, are merged.
pub const WELD_TOLERANCE_M: f64 = 0.001;
/// Two antiparallel faces closer than this, in metres, are in contact.
pub const CONTACT_DISTANCE_M: f64 = 0.002;
/// A closed part must enclose at least this share of the bounding box to count as the room.
const ROOM_SHARE_OF_BOUNDS: f64 = 0.3;
/// Connected parts smaller than this, in m², are reported as specks.
const TINY_PART_M2: f64 = 0.01;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Repairs {
    pub welded_vertices: u32,
    pub removed_degenerate: u32,
    pub removed_duplicates: u32,
    pub t_junctions_split: u32,
    pub flipped_triangles: u32,
    pub parts: u32,
    pub closed_parts: u32,
    pub tiny_parts: u32,
    pub room_volume_m3: Option<f64>,
    pub contact_triangles: u32,
}

struct Mesh {
    p: Vec<Vec3>,
    f: Vec<[u32; 3]>,
    group: Vec<u16>,
    flags: Vec<u8>,
}

impl Mesh {
    fn corners(&self, t: usize) -> [Vec3; 3] {
        self.f[t].map(|i| self.p[i as usize])
    }

    /// Adds a triangle cut from triangle `from`, unless it has no area. Returns whether it was added.
    fn push_if_area(&mut self, tri: [u32; 3], from: usize) -> bool {
        if tri_area(tri.map(|i| self.p[i as usize])) < DEGENERATE_AREA_M2 {
            return false;
        }
        self.f.push(tri);
        self.group.push(self.group[from]);
        self.flags.push(self.flags[from]);
        true
    }
}

/// Cleans `scene` and says what it changed. Vertices are in metres.
pub fn repair(scene: &SceneArrays) -> (SceneArrays, Repairs) {
    let mut r = Repairs::default();
    let (p, remap) = weld(&scene.vertices, WELD_TOLERANCE_M);
    let mut mesh = Mesh { p, f: Vec::new(), group: Vec::new(), flags: Vec::new() };
    let flags = scene.tri_flags.clone().unwrap_or_else(|| vec![0; scene.triangles.len()]);

    let mut seen = HashSet::new();
    for (t, tri) in scene.triangles.iter().enumerate() {
        let w = tri.map(|i| remap[i as usize]);
        if w[0] == w[1]
            || w[1] == w[2]
            || w[0] == w[2]
            || tri_area(w.map(|i| mesh.p[i as usize])) < DEGENERATE_AREA_M2
        {
            r.removed_degenerate += 1;
            continue;
        }
        let mut key = w;
        key.sort_unstable();
        if !seen.insert(key) {
            r.removed_duplicates += 1;
            continue;
        }
        mesh.f.push(w);
        mesh.group.push(scene.tri_group[t]);
        mesh.flags.push(flags[t] & !TRI_FLAG_CONTACT);
    }

    r.t_junctions_split = split_t_junctions(&mut mesh, &mut r.removed_degenerate);
    let shell = orient(&mut mesh, &mut r);
    r.contact_triangles = flag_contacts(&mut mesh, shell.as_deref());

    // Keep only the vertices the triangles use, in their original order.
    let mut used = vec![false; mesh.p.len()];
    mesh.f.iter().flatten().for_each(|&v| used[v as usize] = true);
    let mut new_index = vec![u32::MAX; mesh.p.len()];
    let mut vertices = Vec::new();
    for (i, p) in mesh.p.iter().enumerate().filter(|(i, _)| used[*i]) {
        new_index[i] = vertices.len() as u32;
        vertices.push(p.map(|x| x as f32));
    }
    mesh.f.iter_mut().flatten().for_each(|v| *v = new_index[*v as usize]);
    let used_before = {
        let mut used = vec![false; scene.vertices.len()];
        scene.triangles.iter().flatten().for_each(|&i| used[i as usize] = true);
        used.iter().filter(|&&u| u).count()
    };
    r.welded_vertices = used_before.saturating_sub(vertices.len()) as u32;

    let keep_flags = scene.tri_flags.is_some() || mesh.flags.iter().any(|&f| f != 0);
    let out = SceneArrays {
        vertices,
        triangles: mesh.f,
        tri_group: mesh.group,
        tri_flags: keep_flags.then_some(mesh.flags),
    };
    (out, r)
}

/// Merges vertices within `tol` of an earlier one (per axis) on a hash grid.
fn weld(vertices: &[[f32; 3]], tol: f64) -> (Vec<Vec3>, Vec<u32>) {
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    let mut out: Vec<Vec3> = Vec::new();
    let mut remap = Vec::with_capacity(vertices.len());
    for v in vertices {
        let p = v.map(f64::from);
        let c = p.map(|x| (x / tol).round() as i64);
        let mut found = None;
        'search: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(ids) = grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) else { continue };
                    for &i in ids {
                        let q = out[i as usize];
                        if (0..3).all(|k| (q[k] - p[k]).abs() <= tol) {
                            found = Some(i);
                            break 'search;
                        }
                    }
                }
            }
        }
        let id = found.unwrap_or_else(|| {
            let id = out.len() as u32;
            grid.entry(c).or_default().push(id);
            out.push(p);
            id
        });
        remap.push(id);
    }
    (out, remap)
}

fn edge_key(a: u32, b: u32) -> u64 {
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    (u64::from(lo) << 32) | u64::from(hi)
}

/// Faces on each undirected edge: the count and the first two.
fn edge_faces(f: &[[u32; 3]]) -> HashMap<u64, (u32, u32, u32)> {
    let mut edges: HashMap<u64, (u32, u32, u32)> = HashMap::with_capacity(f.len() * 3 / 2);
    for (t, tri) in f.iter().enumerate() {
        for k in 0..3 {
            let e = edges.entry(edge_key(tri[k], tri[(k + 1) % 3])).or_insert((0, u32::MAX, u32::MAX));
            match e.0 {
                0 => e.1 = t as u32,
                1 => e.2 = t as u32,
                _ => {}
            }
            e.0 += 1;
        }
    }
    edges
}

/// A long edge meeting a finely divided neighbour (a wall against a tiled ceiling) leaves
/// vertices lying on the edge but not part of it, so the edge looks open. Splits the
/// triangle at those vertices. Returns the number of triangles split; pieces with no area
/// are left out and counted in `slivers`.
fn split_t_junctions(mesh: &mut Mesh, slivers: &mut u32) -> u32 {
    const CELL: f64 = 0.5;
    let tol = WELD_TOLERANCE_M;
    let cell_of = |x: f64| (x / CELL).floor() as i64;
    let mut total = 0;
    for _pass in 0..4 {
        let edges = edge_faces(&mesh.f);
        let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
        let mut on_open = HashSet::new();
        for (&e, &(count, ..)) in &edges {
            if count == 1 {
                on_open.insert((e >> 32) as u32);
                on_open.insert(e as u32);
            }
        }
        if on_open.is_empty() {
            break;
        }
        for &v in &on_open {
            grid.entry(mesh.p[v as usize].map(cell_of)).or_default().push(v);
        }

        let mut changed = 0;
        let n = mesh.f.len();
        let mut keep = vec![true; n];
        for (t, kept) in keep.iter_mut().enumerate() {
            let tri = mesh.f[t];
            // Split points per edge k (tri[k] -> tri[k + 1]), ordered along the edge.
            let mut splits: [Vec<u32>; 3] = Default::default();
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                if edges[&edge_key(a, b)].0 != 1 {
                    continue;
                }
                let (pa, pb) = (mesh.p[a as usize], mesh.p[b as usize]);
                let d = sub(pb, pa);
                let len2 = dot(d, d);
                let lo = [0, 1, 2].map(|i| cell_of(pa[i].min(pb[i]) - tol));
                let hi = [0, 1, 2].map(|i| cell_of(pa[i].max(pb[i]) + tol));
                let mut hits = Vec::new();
                for x in lo[0]..=hi[0] {
                    for y in lo[1]..=hi[1] {
                        for z in lo[2]..=hi[2] {
                            for &v in grid.get(&[x, y, z]).map(Vec::as_slice).unwrap_or_default() {
                                if v == a || v == b {
                                    continue;
                                }
                                let q = sub(mesh.p[v as usize], pa);
                                let s = dot(q, d) / len2;
                                if s <= 1e-6 || s >= 1.0 - 1e-6 {
                                    continue;
                                }
                                let off = sub(q, d.map(|c| c * s));
                                if dot(off, off) <= tol * tol {
                                    hits.push((s, v));
                                }
                            }
                        }
                    }
                }
                hits.sort_by(|x, y| x.0.total_cmp(&y.0));
                hits.dedup_by_key(|h| h.1);
                splits[k] = hits.into_iter().map(|h| h.1).collect();
            }
            let n_split = splits.iter().filter(|s| !s.is_empty()).count();
            if n_split == 0 {
                continue;
            }
            let mut ring = Vec::new();
            for k in 0..3 {
                ring.push(tri[k]);
                ring.extend(&splits[k]);
            }
            *kept = false;
            changed += 1;
            if n_split == 1 {
                // Fan from the corner opposite the split edge.
                let k = splits.iter().position(|s| !s.is_empty()).unwrap();
                let apex = ring.iter().position(|&v| v == tri[(k + 2) % 3]).unwrap();
                ring.rotate_left(apex);
                for i in 1..ring.len() - 1 {
                    *slivers += u32::from(!mesh.push_if_area([ring[0], ring[i], ring[i + 1]], t));
                }
            } else {
                // Two or three split edges: no corner sees all the others, so fan from the centroid.
                let c = mesh.corners(t);
                let centre = [0, 1, 2].map(|i| (c[0][i] + c[1][i] + c[2][i]) / 3.0);
                let cv = mesh.p.len() as u32;
                mesh.p.push(centre);
                for i in 0..ring.len() {
                    *slivers += u32::from(!mesh.push_if_area([cv, ring[i], ring[(i + 1) % ring.len()]], t));
                }
            }
        }
        if changed == 0 {
            break;
        }
        keep.resize(mesh.f.len(), true);
        mesh.f = filter(&mesh.f, &keep);
        mesh.group = filter(&mesh.group, &keep);
        mesh.flags = filter(&mesh.flags, &keep);
        total += changed;
    }
    total
}

fn filter<T: Copy>(xs: &[T], keep: &[bool]) -> Vec<T> {
    xs.iter().zip(keep).filter(|(_, k)| **k).map(|(x, _)| *x).collect()
}

/// Winds every connected part consistently, then points normals at the air: into the room
/// shell, and out of closed objects standing in the room. This is how rooms are usually
/// modelled and the side sound arrives on. Open parts keep the winding of their first
/// triangle. Returns which triangles belong to the room shell.
fn orient(mesh: &mut Mesh, r: &mut Repairs) -> Option<Vec<bool>> {
    let edges = edge_faces(&mesh.f);
    let n = mesh.f.len();
    let mut part = vec![u32::MAX; n];
    let mut flipped = vec![false; n];
    struct Part {
        faces: Vec<u32>,
        area: f64,
        volume: f64,
        closed: bool,
    }
    let mut parts = Vec::new();
    for s in 0..n {
        if part[s] != u32::MAX {
            continue;
        }
        let id = parts.len() as u32;
        part[s] = id;
        let mut stack = vec![s as u32];
        let mut faces = Vec::new();
        while let Some(t) = stack.pop() {
            faces.push(t);
            let tri = mesh.f[t as usize];
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                let (count, f0, f1) = edges[&edge_key(a, b)];
                if count != 2 {
                    continue;
                }
                let g = if f0 == t { f1 } else { f0 } as usize;
                if part[g] != u32::MAX {
                    continue;
                }
                // A consistently wound neighbour runs the shared edge b -> a.
                let h = mesh.f[g];
                if (0..3).any(|m| h[m] == a && h[(m + 1) % 3] == b) {
                    mesh.f[g].swap(1, 2);
                    flipped[g] ^= true;
                }
                part[g] = id;
                stack.push(g as u32);
            }
        }
        let (mut area, mut volume, mut closed) = (0.0, 0.0, true);
        for &t in &faces {
            let [a, b, c] = mesh.corners(t as usize);
            area += tri_area([a, b, c]);
            volume += dot(a, cross(b, c)) / 6.0;
            let tri = mesh.f[t as usize];
            closed &= (0..3).all(|k| edges[&edge_key(tri[k], tri[(k + 1) % 3])].0 == 2);
        }
        parts.push(Part { faces, area, volume, closed });
    }

    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for tri in &mesh.f {
        for &v in tri {
            for k in 0..3 {
                lo[k] = lo[k].min(mesh.p[v as usize][k]);
                hi[k] = hi[k].max(mesh.p[v as usize][k]);
            }
        }
    }
    let bounds_volume = (0..3).map(|k| (hi[k] - lo[k]).max(0.0)).product::<f64>();
    let room = parts
        .iter()
        .enumerate()
        .filter(|(_, p)| p.closed)
        .max_by(|a, b| a.1.volume.abs().total_cmp(&b.1.volume.abs()))
        .filter(|(_, p)| p.volume.abs() > ROOM_SHARE_OF_BOUNDS * bounds_volume)
        .map(|(i, _)| i);

    for (i, p) in parts.iter_mut().enumerate() {
        // Signed volume is negative when normals point into the enclosed space.
        let want_positive = Some(i) != room;
        if p.closed && (p.volume > 0.0) != want_positive {
            for &t in &p.faces {
                mesh.f[t as usize].swap(1, 2);
                flipped[t as usize] ^= true;
            }
            p.volume = -p.volume;
        }
    }

    r.flipped_triangles = flipped.iter().filter(|&&f| f).count() as u32;
    r.parts = parts.len() as u32;
    r.closed_parts = parts.iter().filter(|p| p.closed).count() as u32;
    r.tiny_parts = parts.iter().filter(|p| p.area < TINY_PART_M2).count() as u32;
    r.room_volume_m3 = room.map(|i| -parts[i].volume);
    room.map(|i| part.iter().map(|&p| p == i as u32).collect())
}

/// Flags faces lying flat against an antiparallel face of another part (within
/// [`CONTACT_DISTANCE_M`]): no sound reaches them, so they must not add absorption.
/// The room shell itself is never flagged. Returns the number of flagged faces.
fn flag_contacts(mesh: &mut Mesh, shell: Option<&[bool]>) -> u32 {
    const CELL: f64 = 2.0;
    const PLANE_STEP: f64 = 0.05;
    struct Tri {
        c: [Vec3; 3],
        n: Vec3,
        mid: Vec3,
        lo: Vec3,
        hi: Vec3,
        part: u32,
    }
    // Contact is between parts, so label parts by connectivity (any shared edge).
    let part = connected_parts(&mesh.f);
    let tris: Vec<Tri> = (0..mesh.f.len())
        .map(|t| {
            let c = mesh.corners(t);
            let n = cross(sub(c[1], c[0]), sub(c[2], c[0]));
            let l = dot(n, n).sqrt().max(f64::MIN_POSITIVE);
            Tri {
                c,
                n: n.map(|x| x / l),
                mid: [0, 1, 2].map(|i| (c[0][i] + c[1][i] + c[2][i]) / 3.0),
                lo: [0, 1, 2].map(|i| c[0][i].min(c[1][i]).min(c[2][i])),
                hi: [0, 1, 2].map(|i| c[0][i].max(c[1][i]).max(c[2][i])),
                part: part[t],
            }
        })
        .collect();
    let axis = |n: Vec3| {
        let a = n.map(f64::abs);
        if a[0] > a[1] {
            if a[0] > a[2] { 0 } else { 2 }
        } else if a[1] > a[2] {
            1
        } else {
            2
        }
    };
    let plane = |ax: usize| [[1, 2], [2, 0], [0, 1]][ax];
    let mut buckets: HashMap<(usize, i64, i64, i64), Vec<u32>> = HashMap::new();
    for (t, tri) in tris.iter().enumerate() {
        let ax = axis(tri.n);
        let [u, v] = plane(ax);
        let off = (tri.mid[ax] / PLANE_STEP).round() as i64;
        for cu in (tri.lo[u] / CELL).floor() as i64..=(tri.hi[u] / CELL).floor() as i64 {
            for cv in (tri.lo[v] / CELL).floor() as i64..=(tri.hi[v] / CELL).floor() as i64 {
                buckets.entry((ax, off, cu, cv)).or_default().push(t as u32);
            }
        }
    }
    let on_tri = |p: Vec3, t: &Tri| {
        let tol = CONTACT_DISTANCE_M;
        if (0..3).any(|k| p[k] < t.lo[k] - tol || p[k] > t.hi[k] + tol) {
            return false;
        }
        if dot(t.n, sub(p, t.c[0])).abs() > tol {
            return false;
        }
        (0..3).all(|k| dot(cross(sub(t.c[(k + 1) % 3], t.c[k]), sub(p, t.c[k])), t.n) >= -1e-9)
    };
    let mut count = 0;
    for (t, tri) in tris.iter().enumerate() {
        if shell.is_some_and(|s| s[t]) {
            continue;
        }
        let ax = axis(tri.n);
        let [u, v] = plane(ax);
        let off = (tri.mid[ax] / PLANE_STEP).round() as i64;
        let (cu, cv) = ((tri.mid[u] / CELL).floor() as i64, (tri.mid[v] / CELL).floor() as i64);
        let touching = (-1..=1).any(|d| {
            buckets.get(&(ax, off + d, cu, cv)).is_some_and(|cand| {
                cand.iter().any(|&g| {
                    let o = &tris[g as usize];
                    g as usize != t && o.part != tri.part && dot(o.n, tri.n) < -0.999 && on_tri(tri.mid, o)
                })
            })
        });
        if touching {
            mesh.flags[t] |= TRI_FLAG_CONTACT;
            count += 1;
        }
    }
    count
}

/// Part label per triangle, joining triangles that share any edge.
fn connected_parts(f: &[[u32; 3]]) -> Vec<u32> {
    let mut parent: Vec<u32> = (0..f.len() as u32).collect();
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }
    let mut first: HashMap<u64, u32> = HashMap::with_capacity(f.len() * 3 / 2);
    for (t, tri) in f.iter().enumerate() {
        for k in 0..3 {
            let e = edge_key(tri[k], tri[(k + 1) % 3]);
            if let Some(&o) = first.get(&e) {
                let (a, b) = (find(&mut parent, t as u32), find(&mut parent, o));
                parent[a as usize] = b;
            } else {
                first.insert(e, t as u32);
            }
        }
    }
    (0..f.len() as u32).map(|t| find(&mut parent, t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{check_mesh, shoebox};

    /// Each face of the box with its own vertices, nudged by up to half a millimetre.
    fn loose_box() -> SceneArrays {
        let b = shoebox([4.0, 5.0, 3.0]);
        let mut s = SceneArrays::default();
        for (t, tri) in b.triangles.iter().enumerate() {
            let base = s.vertices.len() as u32;
            for (k, &i) in tri.iter().enumerate() {
                let j = (t * 3 + k) as f32;
                s.vertices.push(b.vertices[i as usize].map(|x| x + 0.0004 * (j * 0.37).sin()));
            }
            s.triangles.push([base, base + 1, base + 2]);
            s.tri_group.push(0);
        }
        s
    }

    fn add_box(s: &mut SceneArrays, lo: [f32; 3], size: [f32; 3], group: u16) {
        let b = shoebox(size);
        let base = s.vertices.len() as u32;
        s.vertices.extend(b.vertices.iter().map(|v| [0, 1, 2].map(|k| v[k] + lo[k])));
        s.triangles.extend(b.triangles.iter().map(|t| t.map(|i| i + base)));
        s.tri_group.extend(std::iter::repeat_n(group, b.triangles.len()));
    }

    #[test]
    fn a_clean_room_is_left_alone() {
        let b = shoebox([4.0, 5.0, 3.0]);
        let (out, r) = repair(&b);
        assert_eq!(out, b);
        assert_eq!(r.room_volume_m3, Some(60.0));
        assert_eq!((r.parts, r.closed_parts, r.flipped_triangles, r.contact_triangles), (1, 1, 0, 0));
    }

    #[test]
    fn loose_faces_are_welded_shut() {
        let (out, r) = repair(&loose_box());
        assert_eq!(out.vertices.len(), 8);
        assert_eq!(r.welded_vertices, 36 - 8);
        let c = check_mesh(&out);
        assert_eq!(c.open_edges, 0);
        assert!((c.enclosed_volume_m3.unwrap() - 60.0).abs() < 0.05);
    }

    #[test]
    fn duplicates_and_slivers_go() {
        let mut s = shoebox([4.0, 5.0, 3.0]);
        s.triangles.push([0, 2, 1]); // the floor's first triangle again, other winding
        s.vertices.push([2.0, 0.0, 0.0]); // on the floor edge 0-1
        s.triangles.push([0, 8, 1]); // zero area
        s.tri_group.extend([0, 0]);
        let (out, r) = repair(&s);
        assert_eq!((r.removed_duplicates, r.removed_degenerate), (1, 1));
        assert_eq!(out.triangles.len(), 12);
        assert_eq!(check_mesh(&out).open_edges, 0);
    }

    #[test]
    fn t_junctions_are_split() {
        // The x = max wall gets a vertex in the middle of its bottom edge; the floor does not.
        let mut s = shoebox([4.0, 5.0, 3.0]);
        s.vertices.push([4.0, 2.5, 0.0]);
        s.triangles[10] = [1, 6, 8];
        s.triangles.push([8, 6, 2]);
        s.tri_group.push(0);
        assert_eq!(check_mesh(&s).open_edges, 3);
        let (out, r) = repair(&s);
        assert_eq!(r.t_junctions_split, 1);
        let c = check_mesh(&out);
        assert_eq!((c.open_edges, c.inconsistent_edges), (0, 0));
        assert!((c.enclosed_volume_m3.unwrap() - 60.0).abs() < 1e-9);
    }

    #[test]
    fn normals_end_up_facing_the_air() {
        // A room with its normals facing out, except one triangle.
        let mut s = shoebox([4.0, 5.0, 3.0]);
        s.triangles.iter_mut().skip(1).for_each(|t| t.swap(1, 2));
        let (out, r) = repair(&s);
        assert_eq!(r.flipped_triangles, 11);
        assert_eq!(r.room_volume_m3, Some(60.0));
        assert_eq!(check_mesh(&out).enclosed_volume_m3, Some(60.0));
    }

    #[test]
    fn a_block_on_the_floor_has_its_base_flagged() {
        let mut s = shoebox([4.0, 5.0, 3.0]);
        add_box(&mut s, [1.0, 1.0, 0.0], [1.0, 1.0, 0.5], 1);
        let (out, r) = repair(&s);
        assert_eq!(r.room_volume_m3, Some(60.0));
        assert_eq!(r.contact_triangles, 2);
        let flags = out.tri_flags.unwrap();
        let flagged: Vec<usize> = (0..flags.len()).filter(|&t| flags[t] & TRI_FLAG_CONTACT != 0).collect();
        assert!(flagged.iter().all(|&t| out.tri_group[t] == 1), "only the block's base: {flagged:?}");
        // The block faces the air too, so the closed mesh holds the air volume.
        assert!(
            (check_mesh(&SceneArrays { tri_flags: None, ..out }).enclosed_volume_m3.unwrap() - 59.5).abs()
                < 1e-9
        );
    }
}
