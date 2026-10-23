//! The engine facade. Every request is a [`Command`], every answer an [`Event`],
//! and large arrays travel beside them as raw little-endian buffers. The
//! WebAssembly and Tauri wrappers only move these three things.
//!
//! `cargo test -p ra-api --features ts` regenerates the TypeScript protocol in
//! `apps/web/src/engine/protocol/`.

use std::collections::BTreeMap;

use ra_core::{BandSet, Material, SceneArrays, SceneMeta};
use ra_engine::{Estimate, PreparedScene, RunRequest, Solver};
use ra_geom::ImportReport;
use ra_project::{Project, Scene, SceneDoc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Capabilities,
    NewProject {
        bands: BandSet,
    },
    /// bufs[0] = the `.raproj` bytes.
    OpenProject,
    /// Replies with bufs[0] = the `.raproj` bytes.
    SaveProject,
    /// Replies with the project: manifest, scene and material library.
    GetProject,
    /// Replaces the project's acoustic material library. Layers assigned a material that is
    /// no longer in it become unassigned. Replies with the project.
    SetMaterials {
        materials: Vec<Material>,
    },
    /// Assigns acoustic materials by group key (the layer path); `null` unassigns. Groups not
    /// named keep their material. Replies with the project.
    AssignMaterials {
        assignments: BTreeMap<String, Option<String>>,
    },
    /// bufs = vertices (f32 × 3), triangles (u32 × 3), tri_group (u16), then optionally
    /// tri_flags (u8); an empty buffer skips it.
    /// Vertices are in source units and axes; the engine converts them to metres, Z up.
    /// A group with no acoustic material keeps the one its key had in the previous scene, so
    /// re-importing an edited model keeps the layer mapping.
    SetScene {
        meta: SceneMeta,
        #[serde(default = "one")]
        unit_scale_to_metres: f64,
        #[serde(default)]
        up_axis: UpAxis,
    },
    Estimate {
        run: RunRequest,
    },
    /// Runs every chunk in this instance. The browser's worker pool will call
    /// plan / run_chunk / merge separately once chunk results have a binary encoding.
    Run {
        run: RunRequest,
    },
}

/// Up axis of the source model. Rhino and SketchUp are Z up; glTF and OBJ are usually Y up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum UpAxis {
    #[default]
    Z,
    Y,
}

fn one() -> f64 {
    1.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Ok,
    Error { message: String },
    Capabilities(Capabilities),
    Project { manifest: ra_project::Manifest, scene: Option<Box<SceneDoc>>, materials: Vec<Material> },
    SceneReport { report: ImportReport },
    Estimate { estimate: Estimate },
    Progress { done: u32, total: u32 },
    RunDone { chunks: u32, receivers: u32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Capabilities {
    pub engine_version: String,
    /// "wasm32" or "native".
    pub target: String,
    pub solvers: Vec<String>,
    pub format: String,
}

/// An answer and the buffers that go with it.
#[derive(Debug, PartialEq)]
pub struct Reply {
    pub event: Event,
    pub bufs: Vec<Vec<u8>>,
}

impl From<Event> for Reply {
    fn from(event: Event) -> Self {
        Reply { event, bufs: Vec::new() }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Project(#[from] ra_project::ProjectError),
    #[error("{0}")]
    Invalid(String),
}

pub struct Engine {
    project: Project,
    solvers: Vec<Box<dyn Solver>>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Engine { project: Project::new(BandSet::Octave63To8k), solvers: ra_engine::solvers() }
    }

    pub fn capabilities(&self) -> Capabilities {
        Capabilities {
            engine_version: env!("CARGO_PKG_VERSION").into(),
            target: if cfg!(target_arch = "wasm32") { "wasm32" } else { "native" }.into(),
            solvers: self.solvers.iter().map(|s| s.id().to_string()).collect(),
            format: format!("{}.{}", ra_project::FORMAT_MAJOR, ra_project::FORMAT_MINOR),
        }
    }

    /// Handles one command. Errors come back as `Event::Error`, never as a panic.
    pub fn handle(&mut self, cmd: Command, bufs: Vec<Vec<u8>>, progress: &mut dyn FnMut(Event)) -> Reply {
        self.try_handle(cmd, bufs, progress)
            .unwrap_or_else(|e| Event::Error { message: e.to_string() }.into())
    }

    /// JSON in, JSON out: what the wrappers call.
    pub fn handle_json(
        &mut self,
        cmd: &str,
        bufs: Vec<Vec<u8>>,
        progress: &mut dyn FnMut(String),
    ) -> (String, Vec<Vec<u8>>) {
        let reply = match serde_json::from_str::<Command>(cmd) {
            Ok(cmd) => self.handle(cmd, bufs, &mut |e| progress(to_json(&e))),
            Err(e) => Event::Error { message: format!("bad command: {e}") }.into(),
        };
        (to_json(&reply.event), reply.bufs)
    }

    fn try_handle(
        &mut self,
        cmd: Command,
        mut bufs: Vec<Vec<u8>>,
        progress: &mut dyn FnMut(Event),
    ) -> Result<Reply, EngineError> {
        Ok(match cmd {
            Command::Capabilities => Event::Capabilities(self.capabilities()).into(),
            Command::NewProject { bands } => {
                self.project = Project::new(bands);
                self.project_event().into()
            }
            Command::OpenProject => {
                let bytes = bufs.first().ok_or_else(|| EngineError::Invalid("no project bytes".into()))?;
                self.project = ra_project::read(bytes)?;
                self.project_event().into()
            }
            Command::SaveProject => Reply { event: Event::Ok, bufs: vec![ra_project::write(&self.project)?] },
            Command::GetProject => self.project_event().into(),
            Command::SetMaterials { materials } => {
                self.set_materials(materials)?;
                self.project_event().into()
            }
            Command::AssignMaterials { assignments } => {
                self.assign_materials(assignments)?;
                self.project_event().into()
            }
            Command::SetScene { mut meta, unit_scale_to_metres: k, up_axis } => {
                bufs.resize(4, Vec::new());
                // right-handed Y up to right-handed Z up: (x, y, z) -> (x, -z, y)
                let to_z_up = |[x, y, z]: [f32; 3]| match up_axis {
                    UpAxis::Z => [x, y, z],
                    UpAxis::Y => [x, -z, y],
                };
                let [ox, oy, oz] = meta.origin;
                meta.origin = match up_axis {
                    UpAxis::Z => [ox, oy, oz],
                    UpAxis::Y => [ox, -oz, oy],
                };
                let arrays = SceneArrays {
                    vertices: le::<f32>(&bufs[0])?
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .map(|c| to_z_up(c.map(|x| (f64::from(x) * k) as f32)))
                        .collect(),
                    triangles: le::<u32>(&bufs[1])?.as_chunks::<3>().0.to_vec(),
                    tri_group: le::<u16>(&bufs[2])?,
                    tri_flags: (!bufs[3].is_empty()).then(|| bufs[3].clone()),
                };
                arrays.validate(meta.groups.len()).map_err(EngineError::Invalid)?;
                let (arrays, report) = ra_geom::prepare(&arrays, meta.groups.len());
                if let Some(old) = &self.project.scene {
                    let previous: BTreeMap<&str, &str> = old
                        .doc
                        .meta
                        .groups
                        .iter()
                        .filter_map(|g| Some((g.key.as_str(), g.acoustic_material.as_deref()?)))
                        .collect();
                    for g in meta.groups.iter_mut().filter(|g| g.acoustic_material.is_none()) {
                        g.acoustic_material = previous.get(g.key.as_str()).map(|m| m.to_string());
                    }
                }
                self.project.scene = Some(Scene { doc: SceneDoc { meta, report: report.clone() }, arrays });
                Event::SceneReport { report }.into()
            }
            Command::Estimate { run } => {
                let (solver, scene) = self.prepare(&run, false)?;
                Event::Estimate { estimate: solver.estimate(&scene, &run) }.into()
            }
            Command::Run { run } => {
                let (solver, scene) = self.prepare(&run, true)?;
                let plan = solver.plan(&scene, &run);
                let total = plan.len() as u32;
                let mut parts = Vec::with_capacity(plan.len());
                for chunk in &plan {
                    parts.push(solver.run_chunk(&scene, &run, chunk));
                    progress(Event::Progress { done: chunk.index + 1, total });
                }
                let out = solver.merge(&run, parts);
                Event::RunDone { chunks: total, receivers: out.receivers.len() as u32 }.into()
            }
        })
    }

    fn project_event(&self) -> Event {
        Event::Project {
            manifest: self.project.manifest.clone(),
            scene: self.project.scene.as_ref().map(|s| Box::new(s.doc.clone())),
            materials: self.project.materials.clone(),
        }
    }

    fn set_materials(&mut self, materials: Vec<Material>) -> Result<(), EngineError> {
        let bands = self.project.manifest.bands.len();
        let mut names = std::collections::BTreeSet::new();
        for m in &materials {
            if !names.insert(m.name.as_str()) {
                return Err(EngineError::Invalid(format!("two materials are called {:?}", m.name)));
            }
            for (what, values) in [("absorption", &m.absorption), ("scattering", &m.scattering)] {
                if values.len() != bands {
                    return Err(EngineError::Invalid(format!(
                        "{:?} has {} {what} values for {bands} bands",
                        m.name,
                        values.len()
                    )));
                }
                if values.iter().any(|v| !(0.0..=1.0).contains(v)) {
                    return Err(EngineError::Invalid(format!("{:?} has {what} outside 0 to 1", m.name)));
                }
            }
        }
        if let Some(scene) = &mut self.project.scene {
            for g in &mut scene.doc.meta.groups {
                if g.acoustic_material.as_deref().is_some_and(|m| !names.contains(m)) {
                    g.acoustic_material = None;
                }
            }
        }
        self.project.materials = materials;
        Ok(())
    }

    fn assign_materials(&mut self, assignments: BTreeMap<String, Option<String>>) -> Result<(), EngineError> {
        let library = &self.project.materials;
        let scene =
            self.project.scene.as_mut().ok_or_else(|| EngineError::Invalid("no scene loaded".into()))?;
        // Check everything first, so a bad entry changes nothing.
        for (key, material) in &assignments {
            if !scene.doc.meta.groups.iter().any(|g| &g.key == key) {
                return Err(EngineError::Invalid(format!("no layer {key:?} in the scene")));
            }
            if let Some(m) = material
                && !library.iter().any(|l| &l.name == m)
            {
                return Err(EngineError::Invalid(format!("no material {m:?} in the library")));
            }
        }
        for g in &mut scene.doc.meta.groups {
            if let Some(m) = assignments.get(&g.key) {
                g.acoustic_material = m.clone();
            }
        }
        Ok(())
    }

    /// `need_materials`: every layer must have an acoustic material, as a run needs them.
    fn prepare(
        &self,
        run: &RunRequest,
        need_materials: bool,
    ) -> Result<(&dyn Solver, PreparedScene), EngineError> {
        let solver = self
            .solvers
            .iter()
            .find(|s| s.id() == run.solver)
            .ok_or_else(|| EngineError::Invalid(format!("no solver {:?} in this build", run.solver)))?;
        let scene =
            self.project.scene.as_ref().ok_or_else(|| EngineError::Invalid("no scene loaded".into()))?;
        let mut materials = Vec::new();
        if need_materials {
            let mut missing = Vec::new();
            for g in &scene.doc.meta.groups {
                match g
                    .acoustic_material
                    .as_ref()
                    .and_then(|m| self.project.materials.iter().find(|l| &l.name == m))
                {
                    Some(m) => materials.push(m.clone()),
                    None => missing.push(g.layer_path.as_str()),
                }
            }
            if !missing.is_empty() {
                return Err(EngineError::Invalid(format!("no acoustic material for {}", missing.join(", "))));
            }
        }
        // Speakers are wired in with the speaker work.
        let prepared = PreparedScene {
            meta: scene.doc.meta.clone(),
            arrays: scene.arrays.clone(),
            materials,
            speakers: Vec::new(),
        };
        Ok((solver.as_ref(), prepared))
    }
}

fn to_json(e: &Event) -> String {
    serde_json::to_string(e).expect("events always serialise")
}

fn le<T: FromLe>(bytes: &[u8]) -> Result<Vec<T>, EngineError> {
    if !bytes.len().is_multiple_of(T::SIZE) {
        return Err(EngineError::Invalid(format!(
            "buffer of {} bytes is not a whole number of elements",
            bytes.len()
        )));
    }
    Ok(bytes.chunks_exact(T::SIZE).map(T::from_le).collect())
}

trait FromLe: Sized {
    const SIZE: usize;
    fn from_le(b: &[u8]) -> Self;
}

macro_rules! from_le {
    ($($t:ty),*) => {$(
        impl FromLe for $t {
            const SIZE: usize = std::mem::size_of::<$t>();
            fn from_le(b: &[u8]) -> Self { <$t>::from_le_bytes(b.try_into().unwrap()) }
        }
    )*};
}
from_le!(f32, u32, u16);

#[cfg(test)]
mod tests {
    use super::*;
    use ra_core::Group;

    fn bytes<T: Copy, const N: usize>(xs: &[[T; N]], f: impl Fn(T) -> Vec<u8>) -> Vec<u8> {
        xs.iter().flatten().flat_map(|&x| f(x)).collect()
    }

    fn set_scene(engine: &mut Engine) -> Reply {
        let s = ra_geom::shoebox([6.0, 8.0, 3.0]);
        let meta = SceneMeta {
            origin: [0.0; 3],
            groups: vec![Group {
                key: "Room".into(),
                layer_path: "Room".into(),
                material: Some("Default".into()),
                label: "Room".into(),
                acoustic_material: None,
            }],
            importer: "test".into(),
        };
        let bufs = vec![
            bytes(&s.vertices, |x: f32| x.to_le_bytes().to_vec()),
            bytes(&s.triangles, |x: u32| x.to_le_bytes().to_vec()),
            s.tri_group.iter().flat_map(|g| g.to_le_bytes()).collect(),
        ];
        engine.handle(
            Command::SetScene { meta, unit_scale_to_metres: 1.0, up_axis: UpAxis::Z },
            bufs,
            &mut |_| {},
        )
    }

    fn run() -> RunRequest {
        RunRequest {
            solver: "null".into(),
            bands: BandSet::Octave63To8k,
            max_order: 3,
            n_rays: 40_000,
            seed: 1,
            air_absorption_per_m: None,
            duration_s: 1.0,
            receivers: vec!["R1".into()],
            grids: vec![],
            listening: vec![],
        }
    }

    #[test]
    fn scene_in_report_out() {
        let mut e = Engine::new();
        match set_scene(&mut e).event {
            Event::SceneReport { report } => assert_eq!(report.enclosed_volume_m3, Some(144.0)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn millimetre_y_up_model_becomes_metres_z_up() {
        let mut e = Engine::new();
        let cmd = r#"{"type":"set_scene","meta":{"origin":[0,0,0],"groups":[{"key":"A","layer_path":"A","label":"A"}],"importer":"t"},"unit_scale_to_metres":0.001,"up_axis":"y"}"#;
        let v: Vec<u8> = [1000.0f32, 2000.0, 3000.0, 2000.0, 2000.0, 3000.0, 1000.0, 2000.0, 4000.0]
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect();
        let t: Vec<u8> = [0u32, 1, 2].iter().flat_map(|x| x.to_le_bytes()).collect();
        let (reply, _) = e.handle_json(cmd, vec![v, t, vec![0, 0]], &mut |_| {});
        assert!(reply.contains("scene_report"), "{reply}");
        let s = e.project.scene.as_ref().unwrap();
        assert_eq!(s.arrays.vertices, vec![[1.0, -3.0, 2.0], [2.0, -3.0, 2.0], [1.0, -4.0, 2.0]]);
    }

    #[test]
    fn save_and_open_round_trip() {
        let mut e = Engine::new();
        set_scene(&mut e);
        let saved = e.handle(Command::SaveProject, vec![], &mut |_| {});
        let mut other = Engine::new();
        match other.handle(Command::OpenProject, saved.bufs, &mut |_| {}).event {
            Event::Project { scene: Some(doc), .. } => assert_eq!(doc.meta.groups[0].key, "Room"),
            ev => panic!("{ev:?}"),
        }
    }

    fn material(name: &str) -> Material {
        Material {
            name: name.into(),
            absorption: vec![0.1; 8],
            scattering: vec![0.1; 8],
            source: "test".into(),
        }
    }

    fn groups(e: &Engine) -> Vec<(String, Option<String>)> {
        let scene = e.project.scene.as_ref().unwrap();
        scene.doc.meta.groups.iter().map(|g| (g.key.clone(), g.acoustic_material.clone())).collect()
    }

    fn assign(e: &mut Engine, key: &str, m: Option<&str>) -> Event {
        let assignments = BTreeMap::from([(key.to_string(), m.map(String::from))]);
        e.handle(Command::AssignMaterials { assignments }, vec![], &mut |_| {}).event
    }

    fn with_materials(e: &mut Engine) {
        set_scene(e);
        let materials = vec![material("Plaster"), material("Carpet")];
        let ev = e.handle(Command::SetMaterials { materials }, vec![], &mut |_| {}).event;
        assert!(matches!(ev, Event::Project { .. }), "{ev:?}");
        assert!(matches!(assign(e, "Room", Some("Plaster")), Event::Project { .. }));
    }

    #[test]
    fn mapping_is_saved_with_the_project() {
        let mut e = Engine::new();
        with_materials(&mut e);
        let saved = e.handle(Command::SaveProject, vec![], &mut |_| {});
        let mut other = Engine::new();
        match other.handle(Command::OpenProject, saved.bufs, &mut |_| {}).event {
            Event::Project { scene: Some(doc), materials, .. } => {
                assert_eq!(doc.meta.groups[0].acoustic_material.as_deref(), Some("Plaster"));
                assert_eq!(materials.len(), 2);
            }
            ev => panic!("{ev:?}"),
        }
    }

    #[test]
    fn reimporting_keeps_the_mapping() {
        let mut e = Engine::new();
        with_materials(&mut e);
        set_scene(&mut e);
        assert_eq!(groups(&e), vec![("Room".into(), Some("Plaster".into()))]);
    }

    #[test]
    fn bad_assignments_change_nothing() {
        let mut e = Engine::new();
        with_materials(&mut e);
        let assignments =
            BTreeMap::from([("Room".to_string(), None), ("Balcony".to_string(), Some("Carpet".into()))]);
        let ev = e.handle(Command::AssignMaterials { assignments }, vec![], &mut |_| {}).event;
        assert!(matches!(ev, Event::Error { ref message } if message.contains("Balcony")), "{ev:?}");
        assert!(matches!(assign(&mut e, "Room", Some("Velvet")), Event::Error { .. }));
        assert_eq!(groups(&e), vec![("Room".into(), Some("Plaster".into()))]);
    }

    #[test]
    fn library_is_checked_and_removed_materials_are_unassigned() {
        let mut e = Engine::new();
        with_materials(&mut e);
        let mut short = material("Glass");
        short.absorption.pop();
        let ev = e.handle(Command::SetMaterials { materials: vec![short] }, vec![], &mut |_| {}).event;
        assert!(
            matches!(ev, Event::Error { ref message } if message.contains("7 absorption values")),
            "{ev:?}"
        );
        let ev = e
            .handle(Command::SetMaterials { materials: vec![material("Carpet")] }, vec![], &mut |_| {})
            .event;
        assert!(matches!(ev, Event::Project { .. }));
        assert_eq!(groups(&e), vec![("Room".into(), None)]);
    }

    #[test]
    fn a_run_needs_every_layer_mapped() {
        let mut e = Engine::new();
        set_scene(&mut e);
        let ev = e.handle(Command::Run { run: run() }, vec![], &mut |_| {}).event;
        assert!(
            matches!(ev, Event::Error { ref message } if message == "no acoustic material for Room"),
            "{ev:?}"
        );
        let ev = e.handle(Command::Estimate { run: run() }, vec![], &mut |_| {}).event;
        assert!(matches!(ev, Event::Estimate { .. }), "{ev:?}");
    }

    #[test]
    fn run_reports_progress_per_chunk() {
        let mut e = Engine::new();
        with_materials(&mut e);
        let mut seen = Vec::new();
        let reply = e.handle(Command::Run { run: run() }, vec![], &mut |ev| seen.push(ev));
        assert_eq!(reply.event, Event::RunDone { chunks: 3, receivers: 1 });
        assert_eq!(seen.last(), Some(&Event::Progress { done: 3, total: 3 }));
    }

    #[test]
    fn json_errors_do_not_panic() {
        let mut e = Engine::new();
        let (reply, _) = e.handle_json("{\"type\":\"nope\"}", vec![], &mut |_| {});
        assert!(reply.contains("\"error\""));
        let (reply, _) = e.handle_json(
            &serde_json::to_string(&Command::Estimate { run: run() }).unwrap(),
            vec![],
            &mut |_| {},
        );
        assert!(reply.contains("no scene loaded"));
    }
}
