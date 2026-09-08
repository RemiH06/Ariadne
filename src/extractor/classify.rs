use crate::schema::NodeType;
use std::io::Read;
use std::path::Path;

/// Tope de líneas contadas por archivo (también el tope al que se escala el
/// tamaño visual del nodo en el cliente). Evita leer archivos enormes
/// completos solo para saber que "son grandes".
pub const LINE_COUNT_CAP: u32 = 2000;

pub struct Classification {
    pub node_type: NodeType,
    pub extension: Option<String>,
    pub language: Option<String>,
    pub icon_key: Option<String>,
    pub is_generated: bool,
    pub size_bytes: Option<u64>,
    pub line_count: Option<u32>,
}

const GENERATED_DIR_NAMES: &[&str] = &[
    "dist",
    "build",
    "out",
    "target",
    "coverage",
    ".next",
    ".nuxt",
    "__pycache__",
    ".pytest_cache",
    "bin",
    "obj",
    ".cache",
];

const GENERATED_FILE_SUFFIXES: &[&str] = &[".min.js", ".min.css", ".map"];

pub fn classify(abs_path: &Path, rel_path: &str, is_dir: bool) -> Classification {
    if is_dir {
        return Classification {
            node_type: NodeType::Directory,
            extension: None,
            language: None,
            icon_key: None,
            is_generated: path_has_generated_segment(rel_path),
            size_bytes: None,
            line_count: None,
        };
    }

    let label = abs_path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let extension = abs_path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase());

    let language: Option<&'static str> = extension.as_deref().and_then(language_for_extension);

    let is_generated = GENERATED_FILE_SUFFIXES.iter().any(|s| label.ends_with(s))
        || path_has_generated_segment(rel_path);

    let size_bytes = std::fs::metadata(abs_path).ok().map(|m| m.len());
    let line_count = std::fs::File::open(abs_path)
        .ok()
        .map(|f| count_lines_capped(f, LINE_COUNT_CAP));

    Classification {
        node_type: NodeType::File,
        extension,
        language: language.map(str::to_string),
        icon_key: language.map(str::to_string),
        is_generated,
        size_bytes,
        line_count,
    }
}

/// Cuenta saltos de línea leyendo en bloques (sin validar UTF-8, así
/// funciona igual sobre binarios) y corta apenas se alcanza `cap` para no
/// leer el archivo completo si es enorme.
fn count_lines_capped<R: Read>(mut reader: R, cap: u32) -> u32 {
    let mut buf = [0u8; 8192];
    let mut count: u32 = 0;
    let mut saw_any_byte = false;
    let mut last_byte_was_newline = true;

    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        saw_any_byte = true;
        for &b in &buf[..n] {
            if b == b'\n' {
                count += 1;
                if count >= cap {
                    return cap;
                }
            }
        }
        last_byte_was_newline = buf[n - 1] == b'\n';
    }

    if saw_any_byte && !last_byte_was_newline {
        count += 1;
    }
    count.min(cap)
}

fn path_has_generated_segment(rel_path: &str) -> bool {
    rel_path
        .split('/')
        .any(|seg| GENERATED_DIR_NAMES.iter().any(|d| seg.eq_ignore_ascii_case(d)))
}

/// Mapea extensión -> nombre de ícono Devicon. Deliberadamente no
/// exhaustivo: es preferible no mostrar ícono a mostrar uno incorrecto.
fn language_for_extension(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "py" => "python",
        "go" => "go",
        "rb" => "ruby",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cplusplus",
        "cs" => "csharp",
        "php" => "php",
        "swift" => "swift",
        "html" | "htm" => "html5",
        "css" => "css3",
        "scss" | "sass" => "sass",
        "md" | "markdown" => "markdown",
        "yml" | "yaml" => "yaml",
        "json" => "json",
        "sh" | "bash" => "bash",
        "ps1" => "powershell",
        "sql" => "postgresql",
        "lua" => "lua",
        "hs" => "haskell",
        "pl" | "pm" => "perl",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn counts_lines_without_trailing_newline() {
        let n = count_lines_capped(Cursor::new(b"a\nb\nc" as &[u8]), 2000);
        assert_eq!(n, 3);
    }

    #[test]
    fn counts_lines_with_trailing_newline() {
        let n = count_lines_capped(Cursor::new(b"a\nb\nc\n" as &[u8]), 2000);
        assert_eq!(n, 3);
    }

    #[test]
    fn empty_file_has_zero_lines() {
        let n = count_lines_capped(Cursor::new(b"" as &[u8]), 2000);
        assert_eq!(n, 0);
    }

    #[test]
    fn stops_early_at_cap() {
        let content = "line\n".repeat(5000);
        let n = count_lines_capped(Cursor::new(content.as_bytes()), 2000);
        assert_eq!(n, 2000);
    }
}
