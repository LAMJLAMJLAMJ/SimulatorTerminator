//! The engine facade. Every request is a [`Command`], every answer an [`Event`],
//! and large arrays travel beside them as raw little-endian buffers. The
//! WebAssembly and Tauri wrappers only move these three things.
//!
//! `cargo test -p ra-api --features ts` regenerates the TypeScript protocol in
//! `apps/web/src/engine/protocol/`.

use std::collections::BTreeMap;

use ra_core::{AirConditions, BandSet, Material, Point, SceneArrays, SceneMeta, Setup, TRI_FLAG_CONTACT};
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
        /// Source and receiver points found in the model. They replace the model points of the
        /// previous scene; points added in the app are kept.
        #[serde(default)]
        points: Vec<ModelPoint>,
    },
    /// Replaces the sources and receivers. Replies with the project.
    SetSetup {
        setup: Setup,
    },
    /// Sabine and Eyring reverberation times of the loaded scene from its volume, the area
    /// of each layer that faces the room and the layers' materials.
    Reverberation {
        #[serde(default)]
        air: AirConditions,
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

/// A point object read from the model, in source units and axes, relative to the scene origin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ModelPoint {
    pub id: String,
    pub role: PointRole,
    pub position: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum PointRole {
    Source,
    Receiver,
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
    Error {
        message: String,
    },
    Capabilities(Capabilities),
    Project {
        manifest: ra_project::Manifest,
        scene: Option<Box<SceneDoc>>,
        materials: Vec<Material>,
        setup: Setup,
    },
    SceneReport {
        report: ImportReport,
    },
    Reverberation {
        report: ReverberationReport,
    },
    Estimate {
        estimate: Estimate,
    },
    Progress {
        done: u32,
        total: u32,
    },
    RunDone {
        chunks: u32,
        results: RunResults,
    },
}

/// ISO 3382-1 parameters per receiver, from the run's energy histograms.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RunResults {
    pub bands_hz: Vec<f32>,
    /// The receiver radius used, m.
    pub receiver_radius_m: f64,
    /// Mean distance between reflections, m.
    pub mean_free_path_m: f32,
    /// Share of rays that left the model through a gap.
    pub leak_fraction: f32,
    pub receivers: Vec<ReceiverResult>,
}

/// One value per band; `null` where the decay is too short or noisy to evaluate.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ReceiverResult {
    pub id: String,
    pub edt_s: Vec<Option<f32>>,
    pub t20_s: Vec<Option<f32>>,
    pub t30_s: Vec<Option<f32>>,
    pub c50_db: Vec<Option<f32>>,
    pub c80_db: Vec<Option<f32>>,
    pub d50: Vec<Option<f32>>,
    pub ts_s: Vec<Option<f32>>,
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

/// Diffuse-field reverberation estimate, one value per band of the project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ReverberationReport {
    pub bands_hz: Vec<f32>,
    pub air: AirConditions,
    pub volume_m3: f64,
    /// How the volume was found, for the report.
    pub volume_from: String,
    /// Surface area facing the room, m²; a panel with air on both sides counts twice.
    pub area_m2: f64,
    pub sabine_s: Vec<f64>,
    pub eyring_s: Vec<f64>,
    /// Absorption area of the surfaces, m² (sum of area × absorption).
    pub absorption_area_m2: Vec<f64>,
    /// Absorption of the air, 4mV, m².
    pub air_absorption_m2: Vec<f64>,
    /// Per layer, in scene order.
    pub layers: Vec<LayerAbsorption>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LayerAbsorption {
    pub key: String,
    pub material: Option<String>,
    pub area_m2: f64,
    pub absorption_area_m2: Vec<f64>,
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
            Command::SetSetup { setup } => {
                for (what, points) in [("source", &setup.sources), ("receiver", &setup.receivers)] {
                    let mut ids = std::collections::BTreeSet::new();
                    for p in points {
                        if p.id.trim().is_empty() || !ids.insert(p.id.as_str()) {
                            return Err(EngineError::Invalid(format!(
                                "{what} names must be unique and not empty: {:?}",
                                p.id
                            )));
                        }
                        if !p.position.iter().all(|c| c.is_finite()) {
                            return Err(EngineError::Invalid(format!(
                                "{what} {:?} has no valid position",
                                p.id
                            )));
                        }
                    }
                }
                self.project.setup = setup;
                self.project_event().into()
            }
            Command::AssignMaterials { assignments } => {
                self.assign_materials(assignments)?;
                self.project_event().into()
            }
            Command::SetScene { mut meta, unit_scale_to_metres: k, up_axis, points } => {
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
                let setup = &mut self.project.setup;
                setup.sources.retain(|p| !p.from_model);
                setup.receivers.retain(|p| !p.from_model);
                for p in points {
                    let [x, y, z] = p.position.map(|c| c * k);
                    let position = match up_axis {
                        UpAxis::Z => [x, y, z],
                        UpAxis::Y => [x, -z, y],
                    };
                    let point = Point { id: p.id, position, from_model: true };
                    match p.role {
                        PointRole::Source => setup.sources.push(point),
                        PointRole::Receiver => setup.receivers.push(point),
                    }
                }
                Event::SceneReport { report }.into()
            }
            Command::Reverberation { air } => {
                Event::Reverberation { report: self.reverberation(air)? }.into()
            }
            Command::Estimate { run } => {
                let (solver, scene) = self.prepare(&run, false)?;
                Event::Estimate { estimate: solver.estimate(&scene, &run) }.into()
            }
            Command::Run { mut run } => {
                if run.receivers.is_empty() {
                    run.receivers = self.project.setup.receivers.iter().map(|r| r.id.clone()).collect();
                }
                if run.receiver_radius_m <= 0.0 {
                    let volume = self.project.scene.as_ref().and_then(|s| {
                        let r = &s.doc.report;
                        r.enclosed_volume_m3
                            .or(r.leak.as_ref().and_then(|l| l.air_volume_m3))
                            .or(r.room_volume_m3)
                    });
                    run.receiver_radius_m = volume.map_or(0.5, |v| {
                        ra_engine::auto_receiver_radius(v, run.n_rays, run.air.speed_of_sound())
                    });
                }
                let (solver, scene) = self.prepare(&run, true)?;
                if scene.sources.is_empty() || run.receivers.is_empty() {
                    return Err(EngineError::Invalid(
                        "a run needs at least one source and one receiver".into(),
                    ));
                }
                if let Some(id) =
                    run.receivers.iter().find(|id| !scene.receivers.iter().any(|r| &r.id == *id))
                {
                    return Err(EngineError::Invalid(format!("no receiver {id:?}")));
                }
                let plan = solver.plan(&scene, &run);
                let total = plan.len() as u32;
                let mut parts = Vec::with_capacity(plan.len());
                for chunk in &plan {
                    parts.push(solver.run_chunk(&scene, &run, chunk));
                    progress(Event::Progress { done: chunk.index + 1, total });
                }
                let out = solver.merge(&run, parts);
                let receivers = out
                    .receivers
                    .iter()
                    .map(|e| {
                        let m = ra_metrics::from_energy(e, 1.0 / e.bin_s, &ra_metrics::Options::default())
                            .unwrap_or_default();
                        ReceiverResult {
                            id: e.receiver.clone(),
                            edt_s: m.edt_s,
                            t20_s: m.t20_s,
                            t30_s: m.t30_s,
                            c50_db: m.c50_db,
                            c80_db: m.c80_db,
                            d50: m.d50,
                            ts_s: m.ts_s,
                        }
                    })
                    .collect();
                let results = RunResults {
                    bands_hz: run.bands.centres_hz().to_vec(),
                    receiver_radius_m: run.receiver_radius_m,
                    mean_free_path_m: out.mean_free_path_m,
                    leak_fraction: out.leak_fraction,
                    receivers,
                };
                Event::RunDone { chunks: total, results }.into()
            }
        })
    }

    fn project_event(&self) -> Event {
        Event::Project {
            manifest: self.project.manifest.clone(),
            scene: self.project.scene.as_ref().map(|s| Box::new(s.doc.clone())),
            materials: self.project.materials.clone(),
            setup: self.project.setup.clone(),
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

    fn reverberation(&self, air: AirConditions) -> Result<ReverberationReport, EngineError> {
        let scene =
            self.project.scene.as_ref().ok_or_else(|| EngineError::Invalid("no scene loaded".into()))?;
        let (meta, report, arrays) = (&scene.doc.meta, &scene.doc.report, &scene.arrays);
        let mut warnings = Vec::new();

        // Only the area that faces the room absorbs: not the outside of the shell, faces inside
        // walls or flat against another surface. A hung panel absorbs on both sides.
        let sealed = report.leak.as_ref().and_then(|l| l.sealed_at_voxel_m);
        let contact = sealed.and_then(|h| ra_geom::leak::room_contact(arrays, h));
        let mut area = vec![0.0; meta.groups.len()];
        match &contact {
            Some(c) => {
                for (t, a) in c.facing_area_m2.iter().enumerate() {
                    let flags = arrays.tri_flags.as_ref().map_or(0, |f| f[t]);
                    if flags & TRI_FLAG_CONTACT == 0
                        && let Some(g) = area.get_mut(usize::from(arrays.tri_group[t]))
                    {
                        *g += a;
                    }
                }
            }
            None => {
                area.clone_from(&report.group_area_m2);
                area.resize(meta.groups.len(), 0.0);
                warnings.push(
                    "The room leaks, so every surface counts once, including any outside the room.".into(),
                );
            }
        }

        let (volume_m3, volume_from) = if let Some(v) = report.enclosed_volume_m3.filter(|v| *v > 0.0) {
            (v, "closed mesh".to_string())
        } else if let (Some(c), Some(h)) = (&contact, sealed) {
            (c.volume_m3, format!("air filled in at {h} m voxels"))
        } else if let Some(v) = report.room_volume_m3 {
            warnings.push("Volume is the room shell's, objects inside it included.".into());
            (v, "room shell".to_string())
        } else {
            return Err(EngineError::Invalid(
                "the room has no volume: it is not closed and leaks at every voxel size".into(),
            ));
        };

        let bands = self.project.manifest.bands;
        let bands_hz = bands.centres_hz().to_vec();
        let n = bands_hz.len();
        let mut unassigned = Vec::new();
        let layers: Vec<LayerAbsorption> = meta
            .groups
            .iter()
            .zip(&area)
            .map(|(g, &a)| {
                let material = g
                    .acoustic_material
                    .as_ref()
                    .and_then(|m| self.project.materials.iter().find(|l| &l.name == m));
                if material.is_none() && a > 0.0 {
                    unassigned.push((g.layer_path.as_str(), a));
                }
                LayerAbsorption {
                    key: g.key.clone(),
                    material: material.map(|m| m.name.clone()),
                    area_m2: a,
                    absorption_area_m2: (0..n)
                        .map(|b| material.map_or(0.0, |m| a * f64::from(m.absorption[b])))
                        .collect(),
                }
            })
            .collect();
        if !unassigned.is_empty() {
            let total: f64 = unassigned.iter().map(|(_, a)| a).sum();
            let names: Vec<&str> = unassigned.iter().map(|(n, _)| *n).collect();
            warnings.push(format!(
                "{total:.0} m² on {} without a material counts as not absorbing: {}",
                if names.len() == 1 { "1 layer".to_string() } else { format!("{} layers", names.len()) },
                names.join(", ")
            ));
        }

        let area_m2: f64 = area.iter().sum();
        let absorption_area_m2: Vec<f64> =
            (0..n).map(|b| layers.iter().map(|l| l.absorption_area_m2[b]).sum()).collect();
        let c = air.speed_of_sound();
        let air_per_m: Vec<f64> =
            bands_hz.iter().map(|&f| air.energy_attenuation_per_m(f64::from(f))).collect();
        let rt: Vec<_> = (0..n)
            .map(|b| {
                ra_metrics::statistical::reverberation(
                    volume_m3,
                    area_m2,
                    absorption_area_m2[b],
                    air_per_m[b],
                    c,
                )
            })
            .collect();
        Ok(ReverberationReport {
            bands_hz,
            air,
            volume_m3,
            volume_from,
            area_m2,
            sabine_s: rt.iter().map(|r| r.sabine_s).collect(),
            eyring_s: rt.iter().map(|r| r.eyring_s).collect(),
            absorption_area_m2,
            air_absorption_m2: air_per_m.iter().map(|m| 4.0 * m * volume_m3).collect(),
            layers,
            warnings,
        })
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
        let prepared = PreparedScene::new(
            scene.doc.meta.clone(),
            scene.arrays.clone(),
            materials,
            self.project.setup.sources.clone(),
            self.project.setup.receivers.clone(),
        );
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
            Command::SetScene {
                meta,
                unit_scale_to_metres: 1.0,
                up_axis: UpAxis::Z,
                points: vec![
                    ModelPoint { id: "S1".into(), role: PointRole::Source, position: [1.5, 2.0, 1.5] },
                    ModelPoint { id: "R1".into(), role: PointRole::Receiver, position: [4.5, 6.0, 1.2] },
                ],
            },
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
            air: AirConditions::default(),
            receiver_radius_m: 0.5,
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
        let Event::RunDone { chunks, results } = reply.event else { panic!("{:?}", reply.event) };
        assert_eq!(chunks, 3);
        assert_eq!(results.receivers.len(), 1);
        assert_eq!(seen.last(), Some(&Event::Progress { done: 3, total: 3 }));
    }

    fn setup(e: &Engine) -> Setup {
        let Event::Project { setup, .. } = e.project_event() else { unreachable!() };
        setup
    }

    #[test]
    fn model_points_are_replaced_on_reimport_and_app_points_kept() {
        let mut e = Engine::new();
        set_scene(&mut e);
        let mut s = setup(&e);
        assert_eq!(s.sources[0], Point { id: "S1".into(), position: [1.5, 2.0, 1.5], from_model: true });
        s.receivers.push(Point { id: "Seat 12".into(), position: [3.0, 4.0, 1.2], from_model: false });
        s.receivers[0].position[2] = 1.1;
        assert!(matches!(
            e.handle(Command::SetSetup { setup: s }, vec![], &mut |_| {}).event,
            Event::Project { .. }
        ));
        set_scene(&mut e);
        let s = setup(&e);
        let ids: Vec<&str> = s.receivers.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["Seat 12", "R1"]);
        // The model's point wins over the edit, as the model was re-imported.
        assert_eq!(s.receivers[1].position[2], 1.2);
    }

    #[test]
    fn setup_names_must_be_unique() {
        let mut e = Engine::new();
        let p = Point { id: "A".into(), position: [0.0; 3], from_model: false };
        let bad = Setup { sources: vec![p.clone(), p], receivers: vec![] };
        let ev = e.handle(Command::SetSetup { setup: bad }, vec![], &mut |_| {}).event;
        assert!(matches!(ev, Event::Error { .. }), "{ev:?}");
    }

    #[test]
    fn ray_traced_box_has_plausible_parameters() {
        let mut e = Engine::new();
        set_scene(&mut e);
        let m = Material {
            name: "Ten".into(),
            absorption: vec![0.1; 8],
            scattering: vec![0.3; 8],
            source: String::new(),
        };
        e.handle(Command::SetMaterials { materials: vec![m] }, vec![], &mut |_| {});
        assign(&mut e, "Room", Some("Ten"));
        let mut r = run();
        r.solver = "rt".into();
        r.n_rays = 4000;
        r.duration_s = 2.5;
        r.receivers.clear();
        let ev = e.handle(Command::Run { run: r }, vec![], &mut |_| {}).event;
        let Event::RunDone { results, .. } = ev else { panic!("{ev:?}") };
        assert_eq!(results.leak_fraction, 0.0);
        let rec = &results.receivers[0];
        assert_eq!(rec.id, "R1");
        // Eyring for 144 m³, 180 m², α = 0.1 is 0.161 · 144 / 19.0 = 1.22 s, less a little for air.
        let t30 = rec.t30_s[4].unwrap();
        assert!((t30 - 1.2).abs() < 0.15, "{t30}");
        assert!(rec.c80_db[4].is_some());
    }

    #[test]
    fn reverberation_of_the_test_box() {
        let mut e = Engine::new();
        set_scene(&mut e);
        // Every band at 0.1 on the 6 x 8 x 3 m box: S = 180 m², V = 144 m³.
        let m = Material {
            name: "Ten".into(),
            absorption: vec![0.1; 8],
            scattering: vec![0.1; 8],
            source: String::new(),
        };
        e.handle(Command::SetMaterials { materials: vec![m] }, vec![], &mut |_| {});
        let ev =
            e.handle(Command::Reverberation { air: AirConditions::default() }, vec![], &mut |_| {}).event;
        let Event::Reverberation { report: r } = ev else { panic!("{ev:?}") };
        assert!(r.warnings.iter().any(|w| w.contains("without a material")), "{:?}", r.warnings);
        // Only the air absorbs.
        assert!(r.absorption_area_m2.iter().all(|a| *a == 0.0));
        assert!(r.sabine_s[4] > 30.0, "{:?}", r.sabine_s);

        assign(&mut e, "Room", Some("Ten"));
        let ev =
            e.handle(Command::Reverberation { air: AirConditions::default() }, vec![], &mut |_| {}).event;
        let Event::Reverberation { report: r } = ev else { panic!("{ev:?}") };
        assert_eq!(r.volume_from, "closed mesh");
        assert!((r.volume_m3 - 144.0).abs() < 1e-6);
        assert!((r.area_m2 / 180.0 - 1.0).abs() < 0.02, "{}", r.area_m2);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        // 1 kHz: 0.161 · 144 / (18 + 4mV) with 4mV = 0.62 m² = 1.25 s; air takes more at 8 kHz.
        assert!((r.sabine_s[4] - 1.25).abs() < 0.03, "{:?}", r.sabine_s);
        assert!(r.eyring_s[4] < r.sabine_s[4]);
        assert!(r.sabine_s[7] < r.sabine_s[4]);
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
