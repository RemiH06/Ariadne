mod generate;
mod init_agent_skill;

use clap::{Parser, Subcommand};

pub use generate::GenerateArgs;
pub use init_agent_skill::InitAgentSkillArgs;

#[derive(Parser)]
#[command(name = "ariadne", version, about = "Generador de diagramas y documentación de proyectos")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Generate(GenerateArgs),
    /// Copia la guía de Ariadne para agentes de IA a .claude/skills/ariadne/
    /// del proyecto actual (o el que se le indique)
    InitAgentSkill(InitAgentSkillArgs),
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Generate(args) => generate::run(args),
        Command::InitAgentSkill(args) => init_agent_skill::run(args),
    }
}
