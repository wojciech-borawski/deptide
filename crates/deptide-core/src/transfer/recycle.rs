use std::path::PathBuf;

use crate::error::{AppError, AppResult};

/// Moves files out of a project so they can be restored later.
///
/// `apply` calls `recycle` at most once per project, with the absolute paths
/// of existing files only, and copies nothing into that project when it fails.
pub trait Recycler {
    fn recycle(&self, paths: &[PathBuf]) -> AppResult<()>;
}

/// The Recycle Bin on Windows, the freedesktop trash on Linux, the Trash on macOS.
pub struct SystemRecycler;

impl Recycler for SystemRecycler {
    fn recycle(&self, paths: &[PathBuf]) -> AppResult<()> {
        // A fresh thread: trash sets up COM itself and panics if the caller's thread already did it differently.
        std::thread::scope(|scope| scope.spawn(|| trash::delete_all(paths)).join())
            .map_err(|_| AppError::new("Moving files to the Recycle Bin crashed"))?
            .map_err(|error| {
                AppError::with_context("Moving files to the Recycle Bin failed", error)
            })
    }
}
