//! Read and write CLIP STUDIO PAINT's files other than canvases: sub tools
//! (`.sut`, `EditImageTool.todb`), auto actions (`.las`, `.laf`), colour
//! sets (`.cls`), gradient sets (`.sgs`), material folders and the
//! envelope of encrypted material data.
//!
//! Requirements and validation are in `docs/assets/`. Encrypted payloads are
//! kept as bytes and never decrypted.

#![deny(unsafe_code)]

pub mod action;
pub mod c2f;
pub mod colorset;
pub mod error;
pub mod gradient;
pub mod kind;
pub mod material;
pub mod serial;
pub mod shortcut;
pub mod subtool;
pub mod tar;
#[doc(hidden)]
pub mod testing;

pub use error::{Error, Result};
