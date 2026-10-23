//! `.raproj` project files: a ZIP of JSON documents and `.npy` arrays.
//!
//! This crate is the only reader and writer of the format, and the same
//! compiled code runs in the browser and on the Mac, so a project opens
//! identically in both. See `docs/format.md` for the layout.

pub mod npy;

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use ra_core::{BandSet, Material, SceneArrays, SceneMeta};
use ra_geom::ImportReport;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

/// The format version this build writes.
pub const FORMAT_MAJOR: u32 = 1;
pub const FORMAT_MINOR: u32 = 0;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("not a project file: {0}")]
    Malformed(String),
    #[error("project format {found} is newer than this app understands (up to {FORMAT_MAJOR}.x)")]
    TooNew { found: String },
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("json in {entry}: {source}")]
    Json { entry: String, source: serde_json::Error },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Manifest {
    /// "major.minor". Minor bumps only add optional fields.
    pub format: String,
    pub app_version: String,
    pub engine_version: String,
    pub units: String,
    pub up_axis: String,
    pub bands: BandSet,
    /// SHA-256 (hex) of each input: "scene", "materials", "speaker/<id>".
    pub hashes: BTreeMap<String, String>,
    /// Fields written by newer minor versions, kept on save.
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Manifest {
    pub fn new(bands: BandSet) -> Self {
        Manifest {
            format: format!("{FORMAT_MAJOR}.{FORMAT_MINOR}"),
            app_version: env!("CARGO_PKG_VERSION").into(),
            engine_version: env!("CARGO_PKG_VERSION").into(),
            units: "m".into(),
            up_axis: "z".into(),
            bands,
            hashes: BTreeMap::new(),
            extra: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SceneDoc {
    #[serde(flatten)]
    pub meta: SceneMeta,
    pub report: ImportReport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub doc: SceneDoc,
    pub arrays: SceneArrays,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub manifest: Manifest,
    pub scene: Option<Scene>,
    pub materials: Vec<Material>,
    /// Entries this build doesn't interpret (speakers, setup, runs, originals, or files from a
    /// newer minor version). Written back unchanged.
    pub other: BTreeMap<String, Vec<u8>>,
}

impl Project {
    pub fn new(bands: BandSet) -> Self {
        Project { manifest: Manifest::new(bands), scene: None, materials: Vec::new(), other: BTreeMap::new() }
    }
}

const MANIFEST: &str = "manifest.json";
const SCENE: &str = "scene/scene.json";
const VERTICES: &str = "scene/vertices.npy";
const TRIANGLES: &str = "scene/triangles.npy";
const TRI_GROUP: &str = "scene/tri_group.npy";
const TRI_FLAGS: &str = "scene/tri_flags.npy";
const MATERIALS: &str = "materials/library.json";

/// Writes a project. The output is byte-for-byte reproducible: entries are sorted and
/// timestamps fixed, so the browser and native builds write identical files.
pub fn write(project: &Project) -> Result<Vec<u8>, ProjectError> {
    let mut entries: BTreeMap<String, Vec<u8>> = project.other.clone();
    let mut manifest = project.manifest.clone();
    manifest.format = format!("{FORMAT_MAJOR}.{FORMAT_MINOR}");

    if let Some(scene) = &project.scene {
        let a = &scene.arrays;
        let arrays = [
            (VERTICES, npy::write(&[a.vertices.len(), 3], a.vertices.as_flattened())),
            (TRIANGLES, npy::write(&[a.triangles.len(), 3], a.triangles.as_flattened())),
            (TRI_GROUP, npy::write(&[a.tri_group.len()], &a.tri_group)),
        ];
        let mut hasher = Sha256::new();
        for (name, bytes) in arrays {
            hasher.update(&bytes);
            entries.insert(name.into(), bytes);
        }
        if let Some(flags) = &a.tri_flags {
            let bytes = npy::write(&[flags.len()], flags);
            hasher.update(&bytes);
            entries.insert(TRI_FLAGS.into(), bytes);
        }
        let doc = json(&scene.doc)?;
        hasher.update(&doc);
        entries.insert(SCENE.into(), doc);
        manifest.hashes.insert("scene".into(), hex(&hasher.finalize()));
    }

    let materials = json(&project.materials)?;
    manifest.hashes.insert("materials".into(), hex(&Sha256::digest(&materials)));
    entries.insert(MATERIALS.into(), materials);
    entries.insert(MANIFEST.into(), json(&manifest)?);

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let base = SimpleFileOptions::default().last_modified_time(DateTime::default());
    // manifest first, so a reader can check the version from the first bytes
    let order =
        std::iter::once(MANIFEST).chain(entries.keys().map(String::as_str).filter(|k| *k != MANIFEST));
    for name in order {
        // arrays are stored so they can be sliced in place; JSON is deflated
        let method =
            if name.ends_with(".npy") { CompressionMethod::Stored } else { CompressionMethod::Deflated };
        zip.start_file(name, base.compression_method(method))?;
        zip.write_all(&entries[name])?;
    }
    Ok(zip.finish()?.into_inner())
}

pub fn read(bytes: &[u8]) -> Result<Project, ProjectError> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))?;
    let mut entries = BTreeMap::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf)?;
        let name = f.name()?.to_string();
        entries.insert(name, buf);
    }

    let manifest: Manifest =
        parse(&mut entries, MANIFEST)?.ok_or_else(|| ProjectError::Malformed("no manifest.json".into()))?;
    let major: u32 = manifest
        .format
        .split('.')
        .next()
        .and_then(|m| m.parse().ok())
        .ok_or_else(|| ProjectError::Malformed(format!("bad format version {:?}", manifest.format)))?;
    if major > FORMAT_MAJOR {
        return Err(ProjectError::TooNew { found: manifest.format });
    }
    // Migrations from older majors go here once there is a 2.0.

    let materials = parse(&mut entries, MATERIALS)?.unwrap_or_default();
    let scene = match parse::<SceneDoc>(&mut entries, SCENE)? {
        None => None,
        Some(doc) => {
            let (_, v) = npy::read::<f32>(&take(&mut entries, VERTICES)?)?;
            let (_, t) = npy::read::<u32>(&take(&mut entries, TRIANGLES)?)?;
            let (_, tri_group) = npy::read::<u16>(&take(&mut entries, TRI_GROUP)?)?;
            let tri_flags = optional::<u8>(&mut entries, TRI_FLAGS)?;
            let arrays = SceneArrays {
                vertices: v.as_chunks::<3>().0.to_vec(),
                triangles: t.as_chunks::<3>().0.to_vec(),
                tri_group,
                tri_flags,
            };
            arrays.validate(doc.meta.groups.len()).map_err(ProjectError::Malformed)?;
            Some(Scene { doc, arrays })
        }
    };

    Ok(Project { manifest, scene, materials, other: entries })
}

fn json<T: Serialize>(value: &T) -> Result<Vec<u8>, ProjectError> {
    serde_json::to_vec_pretty(value).map_err(|source| ProjectError::Json { entry: "write".into(), source })
}

fn parse<T: for<'de> Deserialize<'de>>(
    entries: &mut BTreeMap<String, Vec<u8>>,
    name: &str,
) -> Result<Option<T>, ProjectError> {
    entries
        .remove(name)
        .map(|b| {
            serde_json::from_slice(&b).map_err(|source| ProjectError::Json { entry: name.into(), source })
        })
        .transpose()
}

fn take(entries: &mut BTreeMap<String, Vec<u8>>, name: &str) -> Result<Vec<u8>, ProjectError> {
    entries.remove(name).ok_or_else(|| ProjectError::Malformed(format!("missing {name}")))
}

fn optional<T: npy::Element>(
    entries: &mut BTreeMap<String, Vec<u8>>,
    name: &str,
) -> Result<Option<Vec<T>>, ProjectError> {
    entries.remove(name).map(|b| npy::read::<T>(&b).map(|(_, data)| data)).transpose()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra_core::Group;

    fn sample() -> Project {
        let mut arrays = ra_geom::shoebox([20.0, 30.0, 12.0]);
        arrays.tri_flags = Some(vec![0; arrays.triangles.len()]);
        let report = ra_geom::check_mesh(&arrays);
        let meta = SceneMeta {
            origin: [512_000.0, 6_150_000.0, 12.5],
            groups: vec![Group {
                key: "Walls".into(),
                layer_path: "Walls".into(),
                material: Some("Plaster".into()),
                label: "Walls".into(),
                acoustic_material: Some("Plaster on brick".into()),
            }],
            importer: "test".into(),
        };
        let mut p = Project::new(BandSet::Octave63To8k);
        p.scene = Some(Scene { doc: SceneDoc { meta, report }, arrays });
        p.materials.push(Material {
            name: "Plaster on brick".into(),
            absorption: vec![0.01, 0.02, 0.02, 0.03, 0.04, 0.05, 0.05, 0.05],
            scattering: vec![0.05; 8],
            source: "test".into(),
        });
        p.other.insert("setup/sources.json".into(), b"[]".to_vec());
        p
    }

    #[test]
    fn round_trip_keeps_everything() {
        let p = sample();
        let bytes = write(&p).unwrap();
        let back = read(&bytes).unwrap();
        assert_eq!(back.scene, p.scene);
        assert_eq!(back.materials, p.materials);
        assert_eq!(back.other, p.other);
        assert!(back.manifest.hashes.contains_key("scene"));
    }

    #[test]
    fn writing_is_reproducible() {
        let p = sample();
        assert_eq!(write(&p).unwrap(), write(&read(&write(&p).unwrap()).unwrap()).unwrap());
    }

    #[test]
    fn unknown_manifest_fields_survive() {
        let mut p = sample();
        p.manifest.extra.insert("added_in_1_1".into(), serde_json::json!({"x": 1}));
        let back = read(&write(&p).unwrap()).unwrap();
        assert_eq!(back.manifest.extra["added_in_1_1"], serde_json::json!({"x": 1}));
    }

    #[test]
    fn newer_major_is_refused() {
        let mut p = sample();
        p.manifest.format = "2.0".into();
        // write() stamps the current version, so build the newer file by hand
        let mut bytes = write(&p).unwrap();
        let mut proj = read(&bytes).unwrap();
        proj.manifest.format = "2.0".into();
        let mut entries = proj.other.clone();
        entries.insert(MANIFEST.into(), serde_json::to_vec(&proj.manifest).unwrap());
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (k, v) in &entries {
            zip.start_file(k.as_str(), SimpleFileOptions::default()).unwrap();
            zip.write_all(v).unwrap();
        }
        bytes = zip.finish().unwrap().into_inner();
        assert!(matches!(read(&bytes), Err(ProjectError::TooNew { .. })));
    }

    #[test]
    fn python_can_find_arrays_by_name() {
        let bytes = write(&sample()).unwrap();
        let zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let names: Vec<String> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
        for n in [MANIFEST, SCENE, VERTICES, TRIANGLES, TRI_GROUP, TRI_FLAGS, MATERIALS] {
            assert!(names.iter().any(|x| x == n), "{n} missing");
        }
    }
}

/// Every format version has a sample written by the browser build. Native code must
/// read it and write it back byte for byte, which proves both builds write identical files.
#[cfg(test)]
mod format_samples {
    #[test]
    fn browser_sample_round_trips_byte_for_byte() {
        let browser = include_bytes!("../../../validation/formats/1.0-test-room.raproj");
        let project = crate::read(browser).unwrap();
        let report = &project.scene.as_ref().unwrap().doc.report;
        assert_eq!(report.enclosed_volume_m3, Some(7200.0));
        assert_eq!(crate::write(&project).unwrap(), browser.to_vec());
    }
}
