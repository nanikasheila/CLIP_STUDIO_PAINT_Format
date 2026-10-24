//! Read, verify and write CLIP STUDIO PAINT files (.clip / .cmc).
//!
//! The format is not published; everything here was learnt from real files
//! and is written down in `docs/spec/`. Input files are never modified.

#![deny(unsafe_code)]

pub mod binary;
pub mod clip;
pub mod cmc;
pub mod container;
pub mod correction;
pub mod curve;
pub mod database;
pub mod edit;
pub mod effect;
pub mod error;
pub mod gradient;
pub mod image;
pub mod model;
pub mod offscreen;
pub mod render;
pub mod schema;
pub mod stroke;
pub mod text;
pub mod vector;
pub mod verify;
pub mod zlib;

pub use clip::ClipFile;
pub use cmc::CmcFile;
pub use error::{Error, Result};
pub use image::{GrayImage, RgbaImage};
pub use model::{Canvas, Layer, LayerKind, LayerTree};
