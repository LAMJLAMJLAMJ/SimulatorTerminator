//! Mesh checks and repair, and later the BVH and ray-triangle tests.
//!
//! Owned by the model-import thread. `check_mesh` runs in both builds, so the
//! browser and the Mac app report the same problems for the same model.

use std::collections::HashMap;

use ra_core::SceneArrays;
use serde::{Deserialize, Serialize};

/// What the mesh checks found. Shown to the user after every import.
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
    /// Signed volume in m³. Positive when normals point out of the solid, which for a room
    /// model means they point away from the listener; `None` when the mesh is not closed.
    pub enclosed_volume_m3: Option<f64>,
    pub surface_area_m2: f64,
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
    for &(count, dir_sum) in edges.values() {
        match count {
            1 => report.open_edges += 1,
            2 if dir_sum != 0 => report.inconsistent_edges += 1,
            2 => {}
            _ => report.non_manifold_edges += 1,
        }
    }
    if report.open_edges == 0 && report.non_manifold_edges == 0 && report.inconsistent_edges == 0 {
        report.enclosed_volume_m3 = Some(volume);
    }
    report
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// A closed box from the origin to `size`, with outward normals. Used by tests across the workspace.
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
        [0, 2, 1],
        [0, 3, 2], // floor
        [4, 5, 6],
        [4, 6, 7], // ceiling
        [0, 1, 5],
        [0, 5, 4], // y = 0
        [2, 3, 7],
        [2, 7, 6], // y = max
        [0, 4, 7],
        [0, 7, 3], // x = 0
        [1, 2, 6],
        [1, 6, 5], // x = max
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
