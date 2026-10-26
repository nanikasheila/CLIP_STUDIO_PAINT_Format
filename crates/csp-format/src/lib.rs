//! Read, verify and write CLIP STUDIO PAINT files (.clip / .cmc).
//!
//! The format is not published; everything here was learnt from real files
//! and is written down in `docs/spec/`. Input files are never modified.

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod animation;
#[doc(hidden)]
pub mod binary;
pub mod blank;
pub mod clip;
pub mod cmc;
pub mod cmc_pages;
pub mod cmt;
pub mod container;
pub mod copy;
pub mod correction;
pub mod create;
mod curve;
pub mod database;
pub mod edit;
pub mod effect;
pub mod error;
pub mod gradient;
pub mod image;
pub mod material;
pub mod model;
pub mod offscreen;
pub mod render;
pub mod schema;
pub mod story;
#[doc(hidden)]
pub mod stroke;
pub mod text;
pub mod tone;
pub mod vector;
pub mod vector_build;
pub mod verify;
pub mod version;
pub mod work;
mod zlib;

/// The SQLite binding this crate's database API uses (`rusqlite`), so that
/// callers use the same version.
pub use rusqlite;

pub use clip::ClipFile;
pub use cmc::CmcFile;
pub use create::NewCanvas;
pub use error::{Error, Result};
pub use image::{GrayImage, RgbaImage};
pub use model::{Canvas, Layer, LayerKind, LayerTree};
pub use version::VersionInfo;
pub use work::NewWork;
