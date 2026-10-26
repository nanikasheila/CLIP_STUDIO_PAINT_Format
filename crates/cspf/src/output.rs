//! Outputs never replace an input: every command only reads its inputs.

use std::path::Path;

use csp_format::{Error, Result};

/// An error when `out` is the same file as one of `inputs`, by any path:
/// relative or absolute, through a symbolic link or as a hard link. A
/// missing `out` is a new file and always passes.
pub fn check(out: &Path, inputs: &[&Path]) -> Result<()> {
    if !out.exists() {
        return Ok(());
    }
    for input in inputs {
        if same_file::is_same_file(out, input).unwrap_or(false) {
            return Err(Error::format(format!(
                "{} is the input {}; write the output to another file (inputs are never replaced)",
                out.display(),
                input.display()
            )));
        }
    }
    Ok(())
}

/// [`check`], then write `bytes` to `out`.
pub fn write(out: &Path, bytes: impl AsRef<[u8]>, inputs: &[&Path]) -> Result<()> {
    check(out, inputs)?;
    std::fs::write(out, bytes)?;
    Ok(())
}
