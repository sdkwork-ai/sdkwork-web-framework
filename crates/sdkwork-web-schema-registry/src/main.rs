use std::fs;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use sdkwork_web_schema_registry::{
    FrontendContractComposer, FrontendContractSnapshot, SchemaRegistryComposer,
};

#[derive(Debug, Parser)]
#[command(
    name = "sdkwork-schema-registry",
    about = "Compose SDKWork schema registries"
)]
struct Cli {
    #[arg(long, default_value = ".")]
    app_root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Compose {
        #[command(subcommand)]
        target: ComposeTarget,
    },
    Check {
        #[command(subcommand)]
        target: CheckTarget,
    },
}

#[derive(Debug, Subcommand)]
enum ComposeTarget {
    Tables {
        #[arg(long)]
        registry: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = false)]
        require_dependencies: bool,
    },
    FrontendContracts {
        #[arg(long)]
        index: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum CheckTarget {
    Tables {
        #[arg(long)]
        registry: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = false)]
        require_dependencies: bool,
    },
    FrontendContracts {
        #[arg(long)]
        index: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Compose { target } => match target {
            ComposeTarget::Tables {
                registry,
                output,
                require_dependencies,
            } => {
                let rendered = SchemaRegistryComposer::new(&cli.app_root, registry)
                    .require_dependency_registries(require_dependencies)
                    .render_yaml()?;
                write_output(&output, &rendered)?;
            }
            ComposeTarget::FrontendContracts { index, output } => {
                let rendered = FrontendContractComposer::new(&cli.app_root, index).render_yaml()?;
                write_output(&output, &rendered)?;
            }
        },
        Command::Check { target } => match target {
            CheckTarget::Tables {
                registry,
                output,
                require_dependencies,
            } => {
                let rendered = SchemaRegistryComposer::new(&cli.app_root, registry)
                    .require_dependency_registries(require_dependencies)
                    .render_yaml()?;
                ensure_matches(&output, &rendered)?;
            }
            CheckTarget::FrontendContracts { index, output } => {
                FrontendContractSnapshot {
                    index_path: index,
                    snapshot_path: output,
                }
                .check_stale()?;
            }
        },
    }
    Ok(())
}

fn write_output(path: &PathBuf, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)
}

fn ensure_matches(path: &PathBuf, expected: &str) -> Result<(), Box<dyn std::error::Error>> {
    let actual = fs::read_to_string(path)?;
    if actual != expected {
        return Err(format!("generated snapshot is stale: {}", path.display()).into());
    }
    Ok(())
}
