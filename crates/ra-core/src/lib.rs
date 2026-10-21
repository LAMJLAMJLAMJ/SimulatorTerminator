//! Types shared by every engine crate.
//!
//! Conventions: SI units (metres, seconds, Hz), Z up and right-handed as in
//! Rhino and SketchUp. Nothing in this crate does I/O.

pub mod bands;
pub mod material;
pub mod scene;

pub use bands::{Band, BandSet};
pub use material::Material;
pub use scene::{Group, SceneArrays, SceneMeta, TRI_FLAG_CURVED};
