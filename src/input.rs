use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};

use crate::{Document, Error, Result, RootInfo};

pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct IndexRequest {
    pub root: PathBuf,
    pub files: Vec<PathBuf>,
}

pub(crate) struct SelectedInput {
    pub root: RootInfo,
    pub files: Vec<SelectedFile>,
}

pub(crate) struct SelectedFile {
    pub absolute: PathBuf,
    pub logical: String,
}

fn invalid(path: &Path, reason: impl ToString) -> Error {
    Error::Input {
        path: path.to_path_buf(),
        reason: reason.to_string(),
    }
}

pub(crate) fn select(request: &IndexRequest) -> Result<SelectedInput> {
    if request.files.is_empty() {
        return Err(Error::InvalidOptions(
            "at least one file is required".into(),
        ));
    }
    let location =
        fs::canonicalize(&request.root).map_err(|error| invalid(&request.root, error))?;
    if !location.is_dir() {
        return Err(invalid(&location, "root must be a directory"));
    }
    let location_text = location
        .to_str()
        .ok_or_else(|| invalid(&location, "root path must be UTF-8"))?;
    let root = RootInfo {
        id: blake3::hash(location_text.as_bytes()).to_hex().to_string(),
        name: "default".into(),
        location,
    };
    let mut files = BTreeMap::new();
    for input in &request.files {
        let relative = if input.is_absolute() {
            input
                .strip_prefix(&root.location)
                .map_err(|_| invalid(input, "file is outside the selected root"))?
        } else {
            input.as_path()
        };
        let mut absolute = root.location.clone();
        let mut parts = Vec::new();
        for component in relative.components() {
            match component {
                Component::CurDir => continue,
                Component::Normal(name) => {
                    parts.push(
                        name.to_str()
                            .ok_or_else(|| invalid(input, "file path must be UTF-8"))?,
                    );
                    absolute.push(name);
                    if fs::symlink_metadata(&absolute)
                        .map_err(|error| invalid(input, error))?
                        .file_type()
                        .is_symlink()
                    {
                        return Err(invalid(input, "symlink inputs are not supported"));
                    }
                }
                _ => {
                    return Err(invalid(
                        input,
                        "parent traversal and paths outside the root are not supported",
                    ));
                }
            }
        }
        if !fs::metadata(&absolute)
            .map_err(|error| invalid(input, error))?
            .is_file()
        {
            return Err(invalid(
                input,
                "expected a regular file; directory traversal is not implemented",
            ));
        }
        files.insert(parts.join("/"), absolute);
    }
    Ok(SelectedInput {
        root,
        files: files
            .into_iter()
            .map(|(logical, absolute)| SelectedFile { absolute, logical })
            .collect(),
    })
}

pub(crate) fn read(root: &RootInfo, source: &SelectedFile) -> Result<Document> {
    let file = open_file(root, source).map_err(|error| invalid(&source.absolute, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| invalid(&source.absolute, error))?;
    if !metadata.is_file() {
        return Err(invalid(&source.absolute, "expected a regular file"));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(invalid(&source.absolute, "file exceeds the 8 MiB limit"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| invalid(&source.absolute, error))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(invalid(&source.absolute, "file exceeds the 8 MiB limit"));
    }
    if bytes.contains(&0) {
        return Err(invalid(
            &source.absolute,
            "NUL-containing files are not supported",
        ));
    }
    let content_hash = blake3::hash(&bytes).to_hex().to_string();
    let text = String::from_utf8(bytes)
        .map_err(|_| invalid(&source.absolute, "file is not valid UTF-8"))?;
    Ok(Document {
        root_id: root.id.clone(),
        path: source.logical.clone(),
        content_hash,
        text,
    })
}

#[cfg(target_os = "linux")]
fn open_file(root: &RootInfo, source: &SelectedFile) -> std::io::Result<File> {
    use rustix::fs::{Mode, OFlags, openat};

    // Resolve through held directory descriptors, not a check-then-open path.
    let mut file = File::open(&root.location)?;
    let mut components = source.logical.split('/').peekable();
    while let Some(component) = components.next() {
        let mut flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK;
        if components.peek().is_some() {
            flags |= OFlags::DIRECTORY;
        }
        file = openat(&file, component, flags, Mode::empty())?.into();
    }
    Ok(file)
}

#[cfg(not(target_os = "linux"))]
fn open_file(_root: &RootInfo, source: &SelectedFile) -> std::io::Result<File> {
    File::open(&source.absolute)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn rejects_symlinks_introduced_after_selection() {
        for replace_directory in [false, true] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("root");
            fs::create_dir_all(root.join("src")).unwrap();
            fs::write(root.join("src/a.txt"), "original").unwrap();
            let selected = select(&IndexRequest {
                root: root.clone(),
                files: vec!["src/a.txt".into()],
            })
            .unwrap();
            if replace_directory {
                fs::rename(root.join("src"), root.join("moved")).unwrap();
                symlink("moved", root.join("src")).unwrap();
            } else {
                fs::rename(root.join("src/a.txt"), root.join("src/moved.txt")).unwrap();
                symlink("moved.txt", root.join("src/a.txt")).unwrap();
            }
            assert!(matches!(
                read(&selected.root, &selected.files[0]),
                Err(Error::Input { .. })
            ));
        }
    }
}
