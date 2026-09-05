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

#[derive(Clone, Debug, Default)]
pub struct DiscoveryOptions {
    pub no_ignore: bool,
    pub no_gitignore: bool,
    pub hidden: bool,
}

pub(crate) struct SelectedInput {
    pub root: RootInfo,
    pub files: Vec<SelectedFile>,
    pub documents: Vec<Document>,
}

pub(crate) struct SelectedFile {
    pub absolute: PathBuf,
    pub logical: String,
    pub explicit: bool,
}

fn invalid(path: &Path, reason: impl ToString) -> Error {
    Error::Input {
        path: path.to_path_buf(),
        reason: reason.to_string(),
    }
}

pub(crate) fn select(request: &IndexRequest, options: &DiscoveryOptions) -> Result<SelectedInput> {
    if request.files.is_empty() {
        return Err(Error::InvalidOptions(
            "at least one input is required".into(),
        ));
    }
    let location = fs::canonicalize(&request.root).map_err(|e| invalid(&request.root, e))?;
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
    let mut selections = Vec::new();
    for input in &request.files {
        let relative = if input.is_absolute() {
            input
                .strip_prefix(&root.location)
                .map_err(|_| invalid(input, "file is outside the selected root"))?
        } else {
            input.as_path()
        };
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(invalid(
                input,
                "parent traversal and paths outside the root are not supported",
            ));
        }
        let relative: PathBuf = relative
            .components()
            .filter(|c| *c != Component::CurDir)
            .collect();
        let text = relative
            .to_str()
            .ok_or_else(|| invalid(input, "file path must be UTF-8"))?;
        let absolute = root.location.join(&relative);
        if fs::symlink_metadata(&absolute).is_ok() {
            let mut checked = root.location.clone();
            for part in relative.components() {
                checked.push(part);
                if fs::symlink_metadata(&checked)
                    .map_err(|e| invalid(input, e))?
                    .file_type()
                    .is_symlink()
                {
                    return Err(invalid(input, "symlink inputs are not supported"));
                }
            }
            if absolute.is_file() {
                let logical = relative
                    .components()
                    .filter_map(|c| match c {
                        Component::Normal(n) => n.to_str(),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                files.insert(
                    logical.clone(),
                    SelectedFile {
                        absolute,
                        logical,
                        explicit: true,
                    },
                );
            } else if absolute.is_dir() {
                selections.push((relative, None));
            } else {
                return Err(invalid(input, "expected a regular file or directory"));
            }
        } else if text.contains(['*', '?', '[', '{']) {
            let matcher = globset::GlobBuilder::new(text)
                .literal_separator(true)
                .backslash_escape(false)
                .build()
                .map_err(|e| invalid(input, e))?
                .compile_matcher();
            selections.push((relative, Some(matcher)));
        } else {
            return Err(invalid(input, "input does not exist or cannot be accessed"));
        }
    }
    if !selections.is_empty() {
        let mut matched = vec![false; selections.len()];
        let mut walker = ignore::WalkBuilder::new(&root.location);
        walker
            .hidden(!options.hidden)
            .follow_links(false)
            .parents(false)
            .ignore(!options.no_ignore)
            .git_ignore(!options.no_ignore && !options.no_gitignore)
            .git_exclude(!options.no_ignore && !options.no_gitignore)
            .git_global(false)
            .require_git(false);
        for entry in walker.build() {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    report_skip(&e.to_string());
                    continue;
                }
            };
            if let Some(error) = entry.error() {
                report_skip(&error.to_string());
            }
            let relative = entry.path().strip_prefix(&root.location).unwrap();
            let mut selected = false;
            for (i, (directory, glob)) in selections.iter().enumerate() {
                let hit = match glob {
                    Some(glob) => relative.ancestors().any(|p| glob.is_match(p)),
                    None => relative.starts_with(directory),
                };
                if hit {
                    matched[i] = true;
                    selected = true;
                }
            }
            if !selected || entry.file_type().is_some_and(|t| t.is_dir()) {
                continue;
            }
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                report_skip(&format!(
                    "{:?}: not a regular file (symlinks disabled)",
                    entry.path()
                ));
                continue;
            }
            let Some(logical) = relative.to_str() else {
                report_skip(&format!("{:?}: file path must be UTF-8", entry.path()));
                continue;
            };
            files
                .entry(logical.to_owned())
                .or_insert_with(|| SelectedFile {
                    absolute: entry.path().to_path_buf(),
                    logical: logical.to_owned(),
                    explicit: false,
                });
        }
        for (i, (pattern, glob)) in selections.iter().enumerate() {
            if glob.is_some() && !matched[i] {
                return Err(invalid(pattern, "glob matched no visible entries"));
            }
        }
    }
    if files.is_empty() {
        return Err(Error::InvalidOptions("selection contains no files".into()));
    }
    Ok(SelectedInput {
        root,
        files: files.into_values().collect(),
        documents: Vec::new(),
    })
}

pub(crate) fn report_skip(message: &str) {
    use std::io::Write;
    let _ = writeln!(
        std::io::stderr().lock(),
        "sift: skipped {}",
        message.escape_default()
    );
}

pub(crate) fn read(root: &RootInfo, source: &SelectedFile) -> Result<Document> {
    read_with(root, source, || {})
}

fn read_with(
    root: &RootInfo,
    source: &SelectedFile,
    before_read: impl FnOnce(),
) -> Result<Document> {
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
    before_read();
    let mut bytes = Vec::new();
    (&file)
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| invalid(&source.absolute, error))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(invalid(&source.absolute, "file exceeds the 8 MiB limit"));
    }
    let after = file.metadata().map_err(|e| invalid(&source.absolute, e))?;
    if metadata.len() != after.len() || metadata.modified().ok() != after.modified().ok() {
        return Err(invalid(&source.absolute, "file changed during ingestion"));
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
    let mut file = File::open("/")?;
    for component in root.location.components() {
        if let Component::Normal(name) = component {
            file = openat(
                &file,
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::empty(),
            )?
            .into();
        }
    }
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
    fn detects_mutation_and_read_failures() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("a.txt");
        fs::write(&path, "before").unwrap();
        let selected = select(
            &IndexRequest {
                root: temporary.path().into(),
                files: vec!["a.txt".into()],
            },
            &DiscoveryOptions::default(),
        )
        .unwrap();
        assert!(
            read_with(&selected.root, &selected.files[0], || fs::write(
                &path,
                "changed length"
            )
            .unwrap())
            .is_err()
        );
        fs::remove_file(&path).unwrap();
        assert!(read(&selected.root, &selected.files[0]).is_err());
    }

    #[test]
    fn rejects_symlinks_introduced_after_selection() {
        for replacement in ["file", "directory", "root"] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("root");
            fs::create_dir_all(root.join("src")).unwrap();
            fs::write(root.join("src/a.txt"), "original").unwrap();
            let selected = select(
                &IndexRequest {
                    root: root.clone(),
                    files: vec!["src/a.txt".into()],
                },
                &DiscoveryOptions::default(),
            )
            .unwrap();
            if replacement == "root" {
                fs::rename(&root, temporary.path().join("moved")).unwrap();
                symlink("moved", &root).unwrap();
            } else if replacement == "directory" {
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
