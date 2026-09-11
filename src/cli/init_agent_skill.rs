use anyhow::{bail, Context, Result};
use clap::Args;
use std::path::PathBuf;

/// Contenido de la skill embebido en el binario — así `cargo install`
/// alcanza para poder copiarla a cualquier proyecto, sin depender de que
/// el repo de Ariadne siga presente en disco.
const AGENT_SKILL_MD: &str = include_str!("../assets/agent_skill.md");

/// Copia la guía de Ariadne para agentes de IA a `.claude/skills/ariadne/SKILL.md`
/// del proyecto actual, para que cualquier sesión de Claude Code ahí sepa
/// cómo invocar Ariadne e interpretar su salida.
#[derive(Args, Debug)]
pub struct InitAgentSkillArgs {
    /// Directorio del proyecto destino (por defecto: el directorio actual)
    pub path: Option<PathBuf>,

    /// Sobreescribir si ya existe un SKILL.md en ese destino
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: InitAgentSkillArgs) -> Result<()> {
    let root = args.path.unwrap_or_else(|| PathBuf::from("."));
    if !root.exists() {
        bail!("la ruta del proyecto no existe: {}", root.display());
    }

    let skill_dir = root.join(".claude").join("skills").join("ariadne");
    let skill_path = skill_dir.join("SKILL.md");

    if skill_path.exists() && !args.force {
        bail!("ya existe {} — usa --force para sobreescribirlo", skill_path.display());
    }

    std::fs::create_dir_all(&skill_dir)
        .with_context(|| format!("no se pudo crear el directorio: {}", skill_dir.display()))?;
    std::fs::write(&skill_path, AGENT_SKILL_MD)
        .with_context(|| format!("no se pudo escribir {}", skill_path.display()))?;

    println!("escrito: {}", skill_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ariadne-init-agent-skill-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_skill_file_with_expected_content() {
        let dir = tempdir();
        run(InitAgentSkillArgs { path: Some(dir.clone()), force: false }).unwrap();
        let content = std::fs::read_to_string(dir.join(".claude/skills/ariadne/SKILL.md")).unwrap();
        assert!(content.contains("name: ariadne"));
        assert!(content.contains("ariadne generate"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_to_overwrite_without_force() {
        let dir = tempdir();
        run(InitAgentSkillArgs { path: Some(dir.clone()), force: false }).unwrap();
        let err = run(InitAgentSkillArgs { path: Some(dir.clone()), force: false }).unwrap_err();
        assert!(err.to_string().contains("--force"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn overwrites_with_force() {
        let dir = tempdir();
        run(InitAgentSkillArgs { path: Some(dir.clone()), force: false }).unwrap();
        run(InitAgentSkillArgs { path: Some(dir.clone()), force: true }).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn errors_on_missing_target_dir() {
        let missing = std::env::temp_dir().join("ariadne-init-agent-skill-does-not-exist");
        let _ = std::fs::remove_dir_all(&missing);
        let err = run(InitAgentSkillArgs { path: Some(missing), force: false }).unwrap_err();
        assert!(err.to_string().contains("no existe"));
    }
}
