use crate::config::IgnoreConfig;
use anyhow::{Context, Result};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

pub struct WalkedEntry {
    pub abs_path: PathBuf,
    pub rel_path: String,
    pub is_dir: bool,
}

/// Recorre `root` respetando el `.gitignore` del proyecto objetivo (y
/// globales/exclude de git si aplica), más los patrones adicionales de
/// `[ignore].extra` en `conf.ariadne`. Los archivos/carpetas ocultos se
/// omiten por defecto.
pub fn walk(root: &Path, ignore_cfg: &IgnoreConfig) -> Result<Vec<WalkedEntry>> {
    let extra_matcher = build_extra_matcher(root, &ignore_cfg.extra)?;

    let walker = WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    let mut entries = Vec::new();
    for result in walker {
        let entry = result.context("error recorriendo el árbol de archivos")?;
        let path = entry.path();
        if path == root {
            continue;
        }

        let rel = path.strip_prefix(root).unwrap_or(path);
        let rel_posix = to_posix(rel);
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

        if extra_matcher.matched(&rel_posix, is_dir).is_ignore() {
            continue;
        }

        entries.push(WalkedEntry {
            abs_path: path.to_path_buf(),
            rel_path: rel_posix,
            is_dir,
        });
    }

    Ok(entries)
}

fn build_extra_matcher(root: &Path, patterns: &[String]) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(root);
    for pattern in patterns {
        builder
            .add_line(None, pattern)
            .with_context(|| format!("patrón inválido en [ignore].extra: {pattern}"))?;
    }
    builder
        .build()
        .context("no se pudo construir el filtro de exclusión adicional")
}

fn to_posix(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}
