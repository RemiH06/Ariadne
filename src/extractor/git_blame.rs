use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

/// Un commit que tocó un archivo — hash corto, autor, fecha (unix,
/// segundos) y asunto (primera línea del mensaje).
pub struct CommitEntry {
    pub short_hash: String,
    pub author: String,
    pub timestamp: i64,
    pub subject: String,
}

/// Byte que marca el inicio de una línea de encabezado de commit en la
/// salida de `git log` (no puede aparecer en un nombre de archivo real).
const COMMIT_MARKER: char = '\u{1}';

/// Corre `git log` **una sola vez** sobre todo el repo y arma un mapa
/// ruta -> hasta `max_per_file` commits que la tocaron, más recientes
/// primero (ver `parse_git_log_output`). Con `max_per_file = 1` da el
/// mismo resultado que "último commit por archivo" (usado para el rollup
/// de autor/fecha a carpetas/raíz); con más, alimenta el historial corto
/// que se muestra al pedir "ver historial" de un archivo. No es un `git
/// blame` línea por línea real, ni sigue renames. Best-effort: si la
/// carpeta no es un repo git o no hay `git` instalado, devuelve un mapa
/// vacío sin fallar el resto de la generación.
pub fn collect_commit_history_by_path(repo_root: &Path, target_paths: &HashSet<&str>, max_per_file: usize) -> HashMap<String, Vec<CommitEntry>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("-c")
        .arg("core.quotepath=false")
        .arg("log")
        .arg(format!("--format={COMMIT_MARKER}%h%x09%an%x09%at%x09%s"))
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
    parse_git_log_output(&text, target_paths, max_per_file)
}

/// Parsea la salida de `git log --format=\x01%h\t%an\t%at\t%s --name-only`:
/// una línea marcada con `\x01` por cada commit (hash/autor/fecha unix/
/// asunto), seguida de las rutas que tocó, de más nuevo a más viejo. Cada
/// ruta acumula hasta `max_per_file` entradas (las más recientes, por
/// orden de aparición). `target_paths` acota el trabajo: al completar
/// `max_per_file` commits para todas, se corta temprano sin seguir
/// procesando el resto del historial (los archivos con menos commits que
/// `max_per_file` en toda su vida simplemente no llegan a esa cuenta, y el
/// loop igual termina naturalmente al agotar el log).
fn parse_git_log_output(text: &str, target_paths: &HashSet<&str>, max_per_file: usize) -> HashMap<String, Vec<CommitEntry>> {
    let mut result: HashMap<String, Vec<CommitEntry>> = HashMap::new();
    let mut current: Option<(String, String, i64, String)> = None;
    let mut done_count = 0usize;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(COMMIT_MARKER) {
            let mut parts = rest.splitn(4, '\t');
            let hash = parts.next().unwrap_or("").to_string();
            let author = parts.next().unwrap_or("").to_string();
            let ts: i64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let subject = parts.next().unwrap_or("").to_string();
            current = Some((hash, author, ts, subject));
            continue;
        }
        if line.is_empty() || !target_paths.contains(line) {
            continue;
        }
        let entries = result.entry(line.to_string()).or_default();
        if entries.len() >= max_per_file {
            continue;
        }
        let Some((hash, author, ts, subject)) = &current else { continue };
        entries.push(CommitEntry {
            short_hash: hash.clone(),
            author: author.clone(),
            timestamp: *ts,
            subject: subject.clone(),
        });
        if entries.len() == max_per_file {
            done_count += 1;
            if done_count >= target_paths.len() {
                break;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_last_commit_per_file_newest_first() {
        let log = "\u{1}abc123\tAna\t1700000000\tfeat: x\nsrc/a.rs\nsrc/b.rs\n\n\u{1}def456\tBeto\t1690000000\tfix: y\nsrc/a.rs\nsrc/c.rs\n";
        let targets: HashSet<&str> = ["src/a.rs", "src/b.rs", "src/c.rs"].into_iter().collect();
        let result = parse_git_log_output(log, &targets, 1);
        assert_eq!(result.get("src/a.rs").unwrap()[0].author, "Ana");
        assert_eq!(result.get("src/a.rs").unwrap()[0].timestamp, 1700000000);
        assert_eq!(result.get("src/a.rs").unwrap()[0].subject, "feat: x");
        assert_eq!(result.get("src/b.rs").unwrap()[0].author, "Ana");
        assert_eq!(result.get("src/c.rs").unwrap()[0].author, "Beto");
    }

    #[test]
    fn ignores_paths_outside_target_set() {
        let log = "\u{1}abc\tAna\t100\tmsg\nsrc/a.rs\nnode_modules/x.js\n";
        let targets: HashSet<&str> = ["src/a.rs"].into_iter().collect();
        let result = parse_git_log_output(log, &targets, 1);
        assert!(!result.contains_key("node_modules/x.js"));
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn empty_log_yields_empty_map() {
        let targets: HashSet<&str> = ["src/a.rs"].into_iter().collect();
        assert!(parse_git_log_output("", &targets, 1).is_empty());
    }

    #[test]
    fn commit_with_no_matching_files_is_ignored() {
        let log = "\u{1}abc\tAna\t100\tmsg1\nnode_modules/x.js\n\n\u{1}def\tBeto\t50\tmsg2\nsrc/a.rs\n";
        let targets: HashSet<&str> = ["src/a.rs"].into_iter().collect();
        let result = parse_git_log_output(log, &targets, 1);
        assert_eq!(result.get("src/a.rs").unwrap()[0].author, "Beto");
    }

    #[test]
    fn accumulates_up_to_max_per_file_newest_first() {
        let log = "\u{1}c3\tAna\t300\tthird\nsrc/a.rs\n\n\u{1}c2\tBeto\t200\tsecond\nsrc/a.rs\n\n\u{1}c1\tAna\t100\tfirst\nsrc/a.rs\n";
        let targets: HashSet<&str> = ["src/a.rs"].into_iter().collect();
        let result = parse_git_log_output(log, &targets, 2);
        let history = result.get("src/a.rs").unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].subject, "third");
        assert_eq!(history[1].subject, "second");
    }

    #[test]
    fn file_with_fewer_commits_than_max_gets_all_of_them() {
        let log = "\u{1}c1\tAna\t100\tonly commit\nsrc/a.rs\n";
        let targets: HashSet<&str> = ["src/a.rs"].into_iter().collect();
        let result = parse_git_log_output(log, &targets, 5);
        assert_eq!(result.get("src/a.rs").unwrap().len(), 1);
    }
}
