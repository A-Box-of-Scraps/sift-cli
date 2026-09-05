use std::{io::Write, path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use sift::{IndexRequest, SearchQuery, SnapshotHandle, SnapshotStore};

mod output;

#[derive(Parser)]
#[command(
    version,
    about = "Index explicit text files and query immutable local snapshots"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Index explicit UTF-8 files (no directory traversal or glob expansion).")]
    Index {
        #[arg(required = true, num_args = 1..)]
        files: Vec<PathBuf>,
        #[arg(
            long,
            help = "Source root; relative inputs resolve against it (default: current directory)"
        )]
        root: Option<PathBuf>,
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
    },
    #[command(about = "Read metadata from an explicit snapshot directory.")]
    Info { handle: PathBuf },
}

fn run(cli: Cli) -> sift::Result<()> {
    let output = match cli.command {
        Command::Index { files, root } => {
            let request = IndexRequest {
                root: root.map(Ok).unwrap_or_else(std::env::current_dir)?,
                files,
            };
            let handle = SnapshotStore::from_environment()?.index(&request)?;
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(handle.as_path().as_os_str().as_encoded_bytes())?;
            stdout.write_all(b"\n")?;
            return Ok(());
        }
        Command::Query {
            handle,
            query,
            limit,
            path,
            json,
        } => {
            let response = SnapshotHandle::from_path(handle)?.query(&SearchQuery {
                text: query,
                limit,
                path,
            })?;
            output::query(&response, json)?
        }
        Command::Info { handle } => {
            let info = SnapshotHandle::from_path(handle)?.info()?;
            output::info(&info)
        }
    };
    std::io::stdout().lock().write_all(output.as_bytes())?;
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(std::io::stderr().lock(), "sift: {error}");
            if matches!(error, sift::Error::InvalidOptions(_)) {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}
