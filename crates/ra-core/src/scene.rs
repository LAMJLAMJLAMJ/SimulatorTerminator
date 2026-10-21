use serde::{Deserialize, Serialize};

/// Bit 0 of `tri_flags`: the triangle came from a curved or smoothed source face.
pub const TRI_FLAG_CURVED: u8 = 1;

/// One geometry group: the triangles sharing a source layer and resolved material.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Group {
    /// Stable across re-imports: layer or tag path plus resolved material, e.g. "Walls|Plaster".
    pub key: String,
    /// Rhino layer path or SketchUp tag path.
    pub layer_path: String,
    /// Resolved source material: object over layer in Rhino, face paint in SketchUp.
    pub material: String,
    pub label: String,
    /// Acoustic material assigned in the app, by name in the project's library.
    #[serde(default)]
    pub acoustic_material: Option<String>,
}

/// The JSON half of a scene. The arrays travel separately as raw buffers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SceneMeta {
    /// World position of the vertex origin, in metres. Vertices are stored relative to it,
    /// so float32 keeps millimetre precision on georeferenced models.
    pub origin: [f64; 3],
    pub groups: Vec<Group>,
    /// The importer that produced the scene, e.g. "rhino3dm" or "openskp".
    #[serde(default)]
    pub importer: String,
}

/// The binary half of a scene, already in metres, Z up, relative to `SceneMeta::origin`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneArrays {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    pub tri_group: Vec<u16>,
    /// SketchUp only: the group of the paint on each triangle's back face.
    pub tri_group_back: Option<Vec<u16>>,
    /// Per-triangle flags, see [`TRI_FLAG_CURVED`].
    pub tri_flags: Option<Vec<u8>>,
}

impl SceneArrays {
    /// Checks that the arrays agree with each other and with the group list.
    pub fn validate(&self, n_groups: usize) -> Result<(), String> {
        let n_tri = self.triangles.len();
        if self.tri_group.len() != n_tri {
            return Err(format!("tri_group has {} entries for {n_tri} triangles", self.tri_group.len()));
        }
        if let Some(back) = &self.tri_group_back
            && back.len() != n_tri
        {
            return Err(format!("tri_group_back has {} entries for {n_tri} triangles", back.len()));
        }
        if let Some(flags) = &self.tri_flags
            && flags.len() != n_tri
        {
            return Err(format!("tri_flags has {} entries for {n_tri} triangles", flags.len()));
        }
        let n_vert = self.vertices.len() as u32;
        if let Some(t) = self.triangles.iter().find(|t| t.iter().any(|&i| i >= n_vert)) {
            return Err(format!("triangle {t:?} refers past {n_vert} vertices"));
        }
        let groups = self.tri_group.iter().chain(self.tri_group_back.iter().flatten());
        if let Some(g) = groups.copied().find(|&g| usize::from(g) >= n_groups) {
            return Err(format!("group index {g} is past {n_groups} groups"));
        }
        Ok(())
    }
}
