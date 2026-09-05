use std::path::{Path, PathBuf};

use crate::{Error, Result};

pub fn data_directory(xdg_data_home: Option<&Path>, home: Option<&Path>) -> Result<PathBuf> {
    let base = xdg_data_home
        .filter(|path| path.is_absolute())
        .map(Path::to_path_buf)
        .or_else(|| {
            home.filter(|path| path.is_absolute())
                .map(|path| path.join(".local/share"))
        })
        .ok_or(Error::MissingDataDirectory)?;
    Ok(base.join("sift-cli"))
}
