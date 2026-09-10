use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

/// Autor y fecha (unix, segundos) del último commit que tocó un archivo.
pub struct FileBlame {
    pub author: String,
    pub timestamp: i64,
}

/// Byte que marca el inicio de una línea de encabezado de commit en la
/// salida de `git log` (no puede aparecer en un nombre de archivo real).
const COMMIT_MARKER: char = '\u{1}';

/// Corre `git log` **una sola vez** sobre todo el repo y arma un mapa
/// ruta -> autor/fecha del último commit que la tocó. Heurística
/// "primer visto = más reciente" caminando el historial de más nuevo a
/// más viejo (ver `parse_git_log_output`) — no es un `git blame` línea por
/// línea real, ni sigue renames. Best-effort: si la carpeta no es un repo
/// git o no hay `git` instalado, devuelve un mapa vacío sin fallar el
/// resto de la generación.
pub fn collect_last_commit_by_path(repo_root: &Path, target_paths: &HashSet<&str>) -> HashMap<String, FileBlame> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("-c")
        .arg("core.quotepath=false")
        .arg("log")
        .arg(format!("--format={COMMIT_MARKER}%H%x09%an%x09%at"))
        .arg("--name-only")
        .arg("--no-renames")
        .output();
    let Ok(output) = output else {
        return HashMap::new();
    };
    if !output.status.success() {
        return HashMap::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_git_log_output(&text, target_paths)
}

/// Parsea la salida de `git log --format=\x01%H\t%an\t%at --name-only`:
/// una línea marcada con `\x01` por cada commit (hash/autor/fecha unix),
/// seguida de las rutas que tocó, de más nuevo a más viejo. Se registra
/// solo la PRIMERA vez que aparece cada ruta (= el commit más reciente que
/// la tocó). `target_paths` acota el trabajo: al completar blame para
/// todas, se corta temprano sin seguir procesando el resto del historial.
fn parse_git_log_output(text: &str, target_paths: &HashSet<&str>) -> HashMap<String, FileBlame> {
    let mut result: HashMap<String, FileBlame> = HashMap::new();
    let mut current: Option<(String, i64)> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(COMMIT_MARKER) {
            let mut parts = rest.splitn(3, '\t');
            let _hash = parts.next();
            let author = parts.next().unwrap_or("").to_string();
            let ts: i64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            current = Some((author, ts));
            continue;
        }
        if line.is_empty() || !target_paths.contains(line) || result.contains_key(line) {
            continue;
        }
        let Some((author, ts)) = &current else { continue };
        result.insert(line.to_string(), FileBlame { author: author.clone(), timestamp: *ts });
        if result.len() >= target_paths.len() {
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_last_commit_per_file_newest_first() {
        let log = "\u{1}abc123\tAna\t1700000000\nsrc/a.rs\nsrc/b.rs\n\n\u{1}def456\tBeto\t1690000000\nsrc/a.rs\nsrc/c.rs\n";
        let targets: HashSet<&str> = ["src/a.rs", "src/b.rs", "src/c.rs"].into_iter().collect();
        let result = parse_git_log_output(log, &targets);
        assert_eq!(result.get("src/a.rs").unwrap().author, "Ana");
        assert_eq!(result.get("src/a.rs").unwrap().timestamp, 1700000000);
        assert_eq!(result.get("src/b.rs").unwrap().author, "Ana");
        assert_eq!(result.get("src/c.rs").unwrap().author, "Beto");
    }

    #[test]
    fn ignores_paths_outside_target_set() {
        let log = "\u{1}abc\tAna\t100\nsrc/a.rs\nnode_modules/x.js\n";
        let targets: HashSet<&str> = ["src/a.rs"].into_iter().collect();
        let result = parse_git_log_output(log, &targets);
        assert!(!result.contains_key("node_modules/x.js"));
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn empty_log_yields_empty_map() {
        let targets: HashSet<&str> = ["src/a.rs"].into_iter().collect();
        assert!(parse_git_log_output("", &targets).is_empty());
    }

    #[test]
    fn commit_with_no_matching_files_is_ignored() {
        let log = "\u{1}abc\tAna\t100\nnode_modules/x.js\n\n\u{1}def\tBeto\t50\nsrc/a.rs\n";
        let targets: HashSet<&str> = ["src/a.rs"].into_iter().collect();
        let result = parse_git_log_output(log, &targets);
        assert_eq!(result.get("src/a.rs").unwrap().author, "Beto");
    }
}
