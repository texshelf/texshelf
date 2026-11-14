mod build;
mod stats;
mod util;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about = "Builds the texshelf index and template archives")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate every manifest under templates/ without fetching anything
    Check {
        #[arg(default_value = ".")]
        registry: PathBuf,
    },
    /// Fetch sources, pack archives, render previews and write index.json
    Build(build::Args),
    /// Write stats.json with GitHub star counts for each template's repository
    Stats(stats::Args),
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Check { registry } => {
            let manifests = build::load_manifests(&registry)?;
            println!("{} manifests are valid", manifests.len());
            Ok(())
        }
        Command::Build(args) => build::run(args),
        Command::Stats(args) => stats::run(args),
    }
}
