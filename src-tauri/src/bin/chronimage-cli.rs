//! Chronimage CLI — developer + power-user entry point.
//!
//! Subcommands (Phase 0 stubs):
//! - `chronimage-cli doctor` — environment + GPU + model availability report
//! - `chronimage-cli migrate` — run pending catalog migrations
//! - `chronimage-cli import --dry-run <path>` — preview a scan without writing
//! - `chronimage-cli ai audit` — load every installed model and report perf

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "chronimage", version, about = "Chronimage CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Print environment report (OS, CPU, GPU, models, DB location).
    Doctor,

    /// Run any pending SQLite migrations against the user catalog.
    Migrate,

    /// Preview a scan of the given path without writing to the catalog.
    Import {
        path: std::path::PathBuf,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// AI model operations.
    Ai {
        #[command(subcommand)]
        sub: AiCmd,
    },
}

#[derive(Debug, Subcommand)]
enum AiCmd {
    /// Load every installed model and report throughput.
    Audit,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Doctor => {
            println!("chronimage-cli doctor — Phase 0 stub");
            println!("  os: {}", std::env::consts::OS);
            println!("  arch: {}", std::env::consts::ARCH);
            println!("  version: {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Cmd::Migrate => {
            let db_path = chronimage::util::paths::catalog_db_path()?;
            println!(
                "chronimage-cli migrate — applying migrations to {}",
                db_path.display()
            );
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let pool = chronimage::catalog::db::open_pool(
                    chronimage::catalog::db::PoolOptions::new(db_path),
                )
                .await?;
                pool.close().await;
                Ok::<_, Box<dyn std::error::Error>>(())
            })?;
            println!("chronimage-cli migrate — done");
            Ok(())
        }
        Cmd::Import { path, dry_run } => {
            println!(
                "chronimage-cli import path={} dry_run={} — Phase 0 stub",
                path.display(),
                dry_run
            );
            Ok(())
        }
        Cmd::Ai { sub } => match sub {
            AiCmd::Audit => {
                println!("chronimage-cli ai audit — Phase 0 stub (no models installed)");
                Ok(())
            }
        },
    }
}
