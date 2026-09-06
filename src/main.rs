use std::{
    io::{Read, Write},
    path::PathBuf,
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use sift::{IndexRequest, SearchQuery, SnapshotHandle, SnapshotStore};

mod output;

#[derive(Parser)]
#[command(
    version,
    about = "Index text inputs and query immutable local snapshots"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Index UTF-8 files, directories, globs, or stdin.")]
    Index {
        #[arg(num_args = 1.., required_unless_present_any = ["stdin", "files0_from"], conflicts_with = "stdin")]
        files: Vec<PathBuf>,
        #[arg(
            long,
            help = "Source root; relative inputs resolve against it (default: current directory)"
        )]
        root: Option<PathBuf>,
        #[arg(
            long,
            help = "Disable all root-local ignore files (does not include hidden files)"
        )]
        no_ignore: bool,
        #[arg(long, default_value = "default", conflicts_with = "stdin")]
        root_name: String,
        #[arg(long, conflicts_with = "stdin")]
        extend: Option<PathBuf>,
        #[arg(long, help = "Disable .gitignore and .git/info/exclude rules only")]
        no_gitignore: bool,
        #[arg(long, help = "Include hidden entries during discovery")]
        hidden: bool,
        #[arg(long, value_parser = ["-"], conflicts_with = "stdin", help = "Read NUL-terminated literal file paths from stdin")]
        files0_from: Option<String>,
        #[arg(long, conflicts_with_all = ["root", "no_ignore", "no_gitignore", "hidden"], help = "Index stdin as one anonymous UTF-8 document")]
        stdin: bool,
        #[arg(
            long,
            requires = "stdin",
            help = "Display name for stdin, not a filesystem path"
        )]
        name: Option<String>,
    },
    #[command(about = "Search stored content using ordinary text, not regex or FTS syntax.")]
    Query {
        handle: PathBuf,
        query: String,
        #[arg(long, default_value_t = 5)]
        limit: usize,
        #[arg(
            long,
            help = "Exact stored file or subtree, relative to the indexed root"
        )]
        path: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        root: Option<String>,
    },
    #[command(about = "Read metadata from an explicit snapshot directory.")]
    Info {
        handle: PathBuf,
    },
    Delete {
        handle: PathBuf,
    },
    Cleanup,
    Check {
        handle: PathBuf,
    },
}

fn run(cli: Cli) -> sift::Result<()> {
    let output: String = match cli.command {
        Command::Index {
            mut files,
            root,
            no_ignore,
            root_name,
            extend,
            no_gitignore,
            hidden,
            files0_from,
            stdin,
            name,
        } => {
            let store: SnapshotStore = SnapshotStore::from_environment()?;
            let handle: SnapshotHandle = if stdin {
                index_stdin(&store, name)?
            } else {
                let root: PathBuf = root.map(Ok).unwrap_or_else(std::env::current_dir)?;
                if files0_from.is_some() {
                    read_file_list(&root, &mut files)?;
                }
                let request: IndexRequest = IndexRequest { root, files };
                let base: Option<SnapshotHandle> =
                    extend.map(SnapshotHandle::from_path).transpose()?;
                store.index_roots(
                    &[(root_name, request)],
                    &sift::DiscoveryOptions {
                        ignore: ignore_mode(no_ignore, no_gitignore),
                        hidden,
                    },
                    base.as_ref(),
                )?
            };
            let _ = writeln!(
                std::io::stderr().lock(),
                "sift: indexed {} documents",
                handle.info()?.file_count
            );
            return output::handle(handle.as_path());
        }
        Command::Query {
            handle,
            query,
            limit,
            path,
            json,
            root,
        } => {
            let response: sift::QueryResponse =
                SnapshotHandle::from_path(handle)?.query(&SearchQuery {
                    text: query,
                    limit,
                    path,
                    root,
                })?;
            output::query(&response, json)?
        }
        Command::Delete { handle } => {
            SnapshotStore::from_environment()?.delete(&SnapshotHandle::from_path(handle)?)?;
            String::new()
        }
        Command::Cleanup => format!(
            "{}\n",
            SnapshotStore::from_environment()?.cleanup_staging()?
        ),
        Command::Check { handle } => {
            let statuses: Vec<sift::SourceStatus> =
                SnapshotHandle::from_path(handle)?.check_staleness()?;
            format!(
                "{}\n",
                serde_json::to_string(&statuses)
                    .map_err(|e| sift::Error::InvalidMetadata(e.to_string()))?
            )
        }
        Command::Info { handle } => output::info(&SnapshotHandle::from_path(handle)?.info()?),
    };
    std::io::stdout().lock().write_all(output.as_bytes())?;
    Ok(())
}

fn ignore_mode(no_ignore: bool, no_gitignore: bool) -> sift::IgnoreMode {
    if no_ignore {
        sift::IgnoreMode::None
    } else if no_gitignore {
        sift::IgnoreMode::WithoutGit
    } else {
        sift::IgnoreMode::All
    }
}

fn index_stdin(store: &SnapshotStore, name: Option<String>) -> sift::Result<SnapshotHandle> {
    let mut bytes: Vec<u8> = Vec::new();
    std::io::stdin()
        .lock()
        .take(sift::MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > sift::MAX_FILE_BYTES {
        return Err(sift::Error::InvalidOptions(
            "stdin exceeds the 8 MiB limit".into(),
        ));
    }
    let text: String = String::from_utf8(bytes)
        .map_err(|_| sift::Error::InvalidOptions("stdin is not valid UTF-8".into()))?;
    store.index_documents(&[sift::TextDocument {
        name: name.unwrap_or_else(|| "stdin".into()),
        text,
    }])
}

fn read_file_list(root: &std::path::Path, files: &mut Vec<PathBuf>) -> sift::Result<()> {
    use std::io::BufRead;
    let mut input: std::io::StdinLock<'_> = std::io::stdin().lock();
    loop {
        let mut bytes: Vec<u8> = Vec::new();
        if input.read_until(0, &mut bytes)? == 0 {
            break;
        }
        if bytes.pop() != Some(0) {
            return Err(sift::Error::InvalidOptions(
                "file list must end with NUL".into(),
            ));
        }
        if bytes.is_empty() {
            return Err(sift::Error::InvalidOptions("empty file list entry".into()));
        }
        #[cfg(unix)]
        let path: PathBuf = {
            use std::os::unix::ffi::OsStringExt;
            std::ffi::OsString::from_vec(bytes).into()
        };
        #[cfg(not(unix))]
        let path: PathBuf = String::from_utf8(bytes)
            .map_err(|_| sift::Error::InvalidOptions("file path must be UTF-8".into()))?
            .into();
        let absolute: PathBuf = root.join(&path);
        if !std::fs::symlink_metadata(&absolute).is_ok_and(|m| m.is_file()) {
            return Err(sift::Error::Input {
                path,
                reason: "file list entries must name existing regular files".into(),
            });
        }
        files.push(path);
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(
                std::io::stderr().lock(),
                "sift: {}",
                output::terminal_text(&error.to_string(), false)
            );
            if matches!(error, sift::Error::InvalidOptions(_)) {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}
