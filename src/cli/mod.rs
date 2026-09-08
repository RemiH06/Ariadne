mod generate;

use clap::{Parser, Subcommand};

pub use generate::GenerateArgs;

#[derive(Parser)]
#[command(name = "ariadne", version, about = "Generador de diagramas y documentación de proyectos")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Generate(GenerateArgs),
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Generate(args) => generate::run(args),
    }
}
