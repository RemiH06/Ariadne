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
    pub category: Option<&'static str>,
    pub shape: Option<&'static str>,
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
        let name_lower = abs_path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let icon_key = standard_folder_kind(&name_lower).map(|kind| format!("folder-{kind}"));
        return Classification {
            node_type: NodeType::Directory,
            extension: None,
            language: None,
            icon_key,
            is_generated: path_has_generated_segment(rel_path),
            size_bytes: None,
            line_count: None,
            category: None,
            shape: None,
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

    let category = file_category(&label, rel_path, extension.as_deref());
    let shape = file_shape(extension.as_deref());

    Classification {
        node_type: NodeType::File,
        extension,
        language: language.map(str::to_string),
        icon_key: language.map(str::to_string),
        is_generated,
        size_bytes,
        line_count,
        category,
        shape,
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

/// Rol del archivo para el color del nodo. "test" pisa a cualquier otra
/// categoría (un archivo puede tener extensión de config y ser un test de
/// config, por ejemplo, y ahí nos interesa más que es un test).
fn file_category(filename_lower: &str, rel_path: &str, ext: Option<&str>) -> Option<&'static str> {
    if is_test_name(filename_lower, rel_path) {
        return Some("test");
    }
    match ext {
        Some("json") | Some("yaml") | Some("yml") | Some("toml") | Some("ini") | Some("env") => Some("config"),
        Some("md") | Some("markdown") | Some("txt") | Some("rst") => Some("docs"),
        Some("css") | Some("scss") | Some("sass") | Some("less") => Some("styles"),
        Some("html") | Some("htm") => Some("markup"),
        Some("sh") | Some("bash") | Some("ps1") | Some("bat") | Some("cmd") => Some("script"),
        _ => None,
    }
}

/// Familia visual por formato — decide la FORMA del nodo en el cliente
/// (independiente de `category`, que decide el color). `None` (código
/// fuente genérico y cualquier extensión no reconocida) es un círculo por
/// defecto en el cliente.
fn file_shape(ext: Option<&str>) -> Option<&'static str> {
    match ext {
        Some("db") | Some("sqlite") | Some("sqlite3") | Some("csv") | Some("tsv") | Some("parquet") | Some("xlsx") | Some("xls") => {
            Some("data")
        }
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("svg") | Some("webp") | Some("bmp") | Some("ico") | Some("tiff")
        | Some("avif") => Some("image"),
        Some("txt") | Some("log") | Some("text") => Some("text"),
        Some("html") | Some("htm") | Some("xml") | Some("md") | Some("markdown") | Some("rst") | Some("adoc") => Some("markup"),
        _ => None,
    }
}

/// Nombre de carpeta reconocible por convención — decide el ÍCONO de una
/// carpeta (el color/forma de directorio se queda uniforme a propósito).
/// Comparación exacta contra el nombre de la carpeta, no substring, para no
/// marcar por accidente algo como `src-legacy` o `config_old`.
fn standard_folder_kind(name_lower: &str) -> Option<&'static str> {
    match name_lower {
        "test" | "tests" | "__tests__" | "spec" | "specs" => Some("test"),
        "config" | "configs" | "conf" | "settings" => Some("config"),
        "src" | "source" | "lib" => Some("src"),
        "docs" | "doc" | "documentation" => Some("docs"),
        _ => None,
    }
}

const TEST_DIR_NAMES: &[&str] = &["test", "tests", "__tests__", "spec"];

fn is_test_name(filename_lower: &str, rel_path: &str) -> bool {
    let stem = filename_lower.rsplit_once('.').map(|(s, _)| s).unwrap_or(filename_lower);

    let stem_marks_test = stem.starts_with("test_")
        || stem.starts_with("test-")
        || stem == "test"
        || stem.ends_with("_test")
        || stem.ends_with("-test")
        || stem.ends_with(".test")
        || stem.ends_with("_spec")
        || stem.ends_with("-spec")
        || stem.ends_with(".spec");

    stem_marks_test
        || rel_path
            .split('/')
            .any(|seg| TEST_DIR_NAMES.iter().any(|d| seg.eq_ignore_ascii_case(d)))
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
        "ex" | "exs" => "elixir",
        "jl" => "julia",
        "r" => "r",
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

    #[test]
    fn detects_test_files_by_name() {
        for name in ["test_utils.py", "user_test.go", "user.test.js", "user.spec.ts"] {
            assert_eq!(is_test_name(name, name), true, "{name} debería detectarse como test");
        }
        assert_eq!(is_test_name("utils.py", "utils.py"), false);
    }

    #[test]
    fn detects_test_files_by_directory() {
        assert!(is_test_name("helpers.py", "src/tests/helpers.py"));
        assert!(is_test_name("helpers.py", "src/__tests__/helpers.py"));
        assert!(!is_test_name("helpers.py", "src/helpers.py"));
    }

    #[test]
    fn categorizes_files_by_extension() {
        assert_eq!(file_category("config.json", "config.json", Some("json")), Some("config"));
        assert_eq!(file_category("readme.md", "readme.md", Some("md")), Some("docs"));
        assert_eq!(file_category("styles.css", "styles.css", Some("css")), Some("styles"));
        assert_eq!(file_category("index.html", "index.html", Some("html")), Some("markup"));
        assert_eq!(file_category("deploy.sh", "deploy.sh", Some("sh")), Some("script"));
        assert_eq!(file_category("main.rs", "main.rs", Some("rs")), None);
    }

    #[test]
    fn classifies_shape_by_format_family() {
        assert_eq!(file_shape(Some("csv")), Some("data"));
        assert_eq!(file_shape(Some("parquet")), Some("data"));
        assert_eq!(file_shape(Some("png")), Some("image"));
        assert_eq!(file_shape(Some("svg")), Some("image"));
        assert_eq!(file_shape(Some("txt")), Some("text"));
        assert_eq!(file_shape(Some("md")), Some("markup"));
        assert_eq!(file_shape(Some("html")), Some("markup"));
        assert_eq!(file_shape(Some("rs")), None);
        assert_eq!(file_shape(None), None);
    }

    #[test]
    fn recognizes_standard_folder_names() {
        assert_eq!(standard_folder_kind("tests"), Some("test"));
        assert_eq!(standard_folder_kind("__tests__"), Some("test"));
        assert_eq!(standard_folder_kind("config"), Some("config"));
        assert_eq!(standard_folder_kind("src"), Some("src"));
        assert_eq!(standard_folder_kind("docs"), Some("docs"));
        assert_eq!(standard_folder_kind("src-legacy"), None);
        assert_eq!(standard_folder_kind("assets"), None);
    }

    #[test]
    fn test_category_wins_over_extension_category() {
        // un test en formato .json debería seguir marcándose como test, no config
        assert_eq!(
            file_category("config_test.json", "config_test.json", Some("json")),
            Some("test")
        );
    }
}
