mod bundle;
mod tree;
mod walker;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Print directory tree honouring ignore files
    Tree {
        /// Directory to walk
        dir: PathBuf,
    },
    /// Collect sources into a single bundle
    Bundle {
        /// Directory to walk
        dir: PathBuf,
        /// Output bundle file
        #[arg(default_value = "rs-bundle.txt")]
        out: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Tree { dir } => tree::run(&dir),
        Commands::Bundle { dir, out } => bundle::run(&dir, &out),
    }
}
