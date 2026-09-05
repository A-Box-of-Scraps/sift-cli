use std::{io::Write, path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use sift::SnapshotHandle;

#[derive(Parser)]
#[command(
    version,
    about = "Sift snapshot storage (indexing and search not yet implemented)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Read metadata from an explicit snapshot directory.")]
    Info { handle: PathBuf },
}

fn run(cli: Cli) -> sift::Result<()> {
    match cli.command {
        Command::Info { handle } => {
            let info = SnapshotHandle::from_path(handle)?.info()?;
            let output = format!(
                "id: {}\nbackend: {}\nformat: {}\ncreated_at_unix_seconds: {}\npreprocessing: {}\n",
                info.id,
                info.backend,
                info.format_version,
                info.created_at_unix_seconds,
                info.preprocessing_config,
            );
            std::io::stdout().lock().write_all(output.as_bytes())?;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(std::io::stderr().lock(), "sift: {error}");
            ExitCode::FAILURE
        }
    }
}
