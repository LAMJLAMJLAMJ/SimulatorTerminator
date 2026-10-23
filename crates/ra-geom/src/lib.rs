//! Mesh checks and repair, and later the BVH and ray-triangle tests.
//!
//! Owned by the model-import thread. Everything here runs in both builds, so the
//! browser and the Mac app report and repair the same model the same way.
//! [`prepare`] is what the engine runs on every imported scene.

pub mod leak;
pub mod repair;

use std::collections::HashMap;

use ra_core::{SceneArrays, TRI_FLAG_CONTACT};
use serde::{Deserialize, Serialize};

pub use leak::LeakReport;

/// What the mesh checks found and what the repair changed. Shown to the user after every import.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ImportReport {
    pub triangles: u32,
    pub vertices: u32,
    /// Edges used by exactly one triangle: gaps that leak ray energy.
    pub open_edges: u32,
    /// Edges used by three or more triangles.
    pub non_manifold_edges: u32,
    /// Edges whose two triangles run the same way, so one of them is flipped.
    pub inconsistent_edges: u32,
    /// Triangles with (near) zero area.
    pub degenerate_triangles: u32,
    /// Air enclosed by the mesh, in m³: minus its signed volume, so positive when normals face
    /// the air (into the room, out of objects in it). `None` when the mesh is not closed.
    pub enclosed_volume_m3: Option<f64>,
    pub surface_area_m2: f64,
    // Added in format 1.1: the repairs `prepare` made and the room checks.
    /// Total length of the open edges, m.
    #[serde(default)]
    pub open_edge_length_m: f64,
    /// Vertices merged into a neighbour closer than 1 mm.
    #[serde(default)]
    pub welded_vertices: u32,
    /// Zero-area triangles removed, most of them collapsed by welding.
    #[serde(default)]
    pub removed_degenerate: u32,
    /// Triangles removed because another one has the same three corners.
    #[serde(default)]
    pub removed_duplicates: u32,
    /// Triangles split where a neighbour's vertex lay on their edge (T-junctions).
    #[serde(default)]
    pub t_junctions_split: u32,
    /// Triangles turned over so each part is wound consistently and faces the air.
    #[serde(default)]
    pub flipped_triangles: u32,
    /// Pieces of surface connected through edges shared by exactly two triangles.
    #[serde(default)]
    pub parts: u32,
    #[serde(default)]
    pub closed_parts: u32,
    /// Parts under 0.01 m²: specks left over from modelling.
    #[serde(default)]
    pub tiny_parts: u32,
    /// Volume inside the room shell, the largest closed part, in m³. `None` when no closed
    /// part encloses at least 30 % of the bounding box.
    #[serde(default)]
    pub room_volume_m3: Option<f64>,
    /// Triangles flat against another surface (flagged, never hit by sound).
    #[serde(default)]
    pub contact_triangles: u32,
    #[serde(default)]
    pub contact_area_m2: f64,
    /// Area per group that sound can reach (contact faces left out), m², indexed like the groups.
    #[serde(default)]
    pub group_area_m2: Vec<f64>,
    #[serde(default)]
    pub leak: Option<LeakReport>,
}

/// Repairs an imported scene and checks it. Vertices are in metres.
pub fn prepare(scene: &SceneArrays, n_groups: usize) -> (SceneArrays, ImportReport) {
    let (arrays, r) = repair::repair(scene);
    let mut report = check_mesh(&arrays);
    report.welded_vertices = r.welded_vertices;
    report.removed_degenerate = r.removed_degenerate;
    report.removed_duplicates = r.removed_duplicates;
    report.t_junctions_split = r.t_junctions_split;
    report.flipped_triangles = r.flipped_triangles;
    report.parts = r.parts;
    report.closed_parts = r.closed_parts;
    report.tiny_parts = r.tiny_parts;
    report.room_volume_m3 = r.room_volume_m3;
    report.contact_triangles = r.contact_triangles;
    report.group_area_m2 = vec![0.0; n_groups];
    for (t, tri) in arrays.triangles.iter().enumerate() {
        let area = tri_area(tri.map(|i| arrays.vertices[i as usize].map(f64::from)));
        if arrays.tri_flags.as_ref().is_some_and(|f| f[t] & TRI_FLAG_CONTACT != 0) {
            report.contact_area_m2 += area;
        } else if let Some(g) = report.group_area_m2.get_mut(usize::from(arrays.tri_group[t])) {
            *g += area;
        }
    }
    report.leak = leak::leak_test(&arrays);
    (arrays, report)
}

/// Area below which a triangle counts as degenerate, in m².
const DEGENERATE_AREA_M2: f64 = 1e-8;

pub fn check_mesh(scene: &SceneArrays) -> ImportReport {
    let v = &scene.vertices;
    // edge (low, high) -> (count, sum of directions: +1 if low->high in the triangle)
    let mut edges: HashMap<(u32, u32), (u32, i32)> = HashMap::new();
    let mut degenerate = 0;
    let mut area = 0.0;
    let mut volume = 0.0;

    for t in &scene.triangles {
        let [a, b, c] = t.map(|i| v[i as usize].map(f64::from));
        let ab = sub(b, a);
        let ac = sub(c, a);
        let n = cross(ab, ac);
        let tri_area = 0.5 * dot(n, n).sqrt();
        if tri_area < DEGENERATE_AREA_M2 {
            degenerate += 1;
        }
        area += tri_area;
        volume += dot(a, cross(b, c)) / 6.0;

        for (p, q) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (key, dir) = if p < q { ((p, q), 1) } else { ((q, p), -1) };
            let e = edges.entry(key).or_insert((0, 0));
            e.0 += 1;
            e.1 += dir;
        }
    }

    let mut report = ImportReport {
        triangles: scene.triangles.len() as u32,
        vertices: v.len() as u32,
        degenerate_triangles: degenerate,
        surface_area_m2: area,
        ..Default::default()
    };
    for (&(p, q), &(count, dir_sum)) in &edges {
        match count {
            1 => {
                report.open_edges += 1;
                let d = sub(v[p as usize].map(f64::from), v[q as usize].map(f64::from));
                report.open_edge_length_m += dot(d, d).sqrt();
            }
            2 if dir_sum != 0 => report.inconsistent_edges += 1,
            2 => {}
            _ => report.non_manifold_edges += 1,
        }
    }
    if report.open_edges == 0 && report.non_manifold_edges == 0 && report.inconsistent_edges == 0 {
        report.enclosed_volume_m3 = Some(-volume);
    }
    report
}

type Vec3 = [f64; 3];

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn tri_area([a, b, c]: [Vec3; 3]) -> f64 {
    let n = cross(sub(b, a), sub(c, a));
    0.5 * dot(n, n).sqrt()
}

/// A closed room from the origin to `size`, normals facing in. Used by tests across the workspace.
pub fn shoebox(size: [f32; 3]) -> SceneArrays {
    let [x, y, z] = size;
    let vertices = vec![
        [0.0, 0.0, 0.0],
        [x, 0.0, 0.0],
        [x, y, 0.0],
        [0.0, y, 0.0],
        [0.0, 0.0, z],
        [x, 0.0, z],
        [x, y, z],
        [0.0, y, z],
    ];
    let triangles = vec![
        [0, 1, 2],
        [0, 2, 3], // floor
        [4, 6, 5],
        [4, 7, 6], // ceiling
        [0, 5, 1],
        [0, 4, 5], // y = 0
        [2, 7, 3],
        [2, 6, 7], // y = max
        [0, 7, 4],
        [0, 3, 7], // x = 0
        [1, 6, 2],
        [1, 5, 6], // x = max
    ];
    let n = triangles.len();
    SceneArrays { vertices, triangles, tri_group: vec![0; n], ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_box_has_volume() {
        let r = check_mesh(&shoebox([4.0, 5.0, 3.0]));
        assert_eq!((r.open_edges, r.non_manifold_edges, r.inconsistent_edges), (0, 0, 0));
        assert!((r.enclosed_volume_m3.unwrap() - 60.0).abs() < 1e-9);
        assert!((r.surface_area_m2 - 94.0).abs() < 1e-9);
    }

    #[test]
    fn missing_face_leaves_open_edges() {
        let mut s = shoebox([4.0, 5.0, 3.0]);
        s.triangles.truncate(10);
        s.tri_group.truncate(10);
        let r = check_mesh(&s);
        assert_eq!(r.open_edges, 4);
        assert_eq!(r.enclosed_volume_m3, None);
    }

    #[test]
    fn flipped_triangle_is_reported() {
        let mut s = shoebox([4.0, 5.0, 3.0]);
        s.triangles[0].swap(1, 2);
        let r = check_mesh(&s);
        assert_eq!(r.inconsistent_edges, 3);
        assert_eq!(r.enclosed_volume_m3, None);
    }
}
