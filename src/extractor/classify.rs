use crate::schema::NodeType;
use std::path::Path;

pub struct Classification {
    pub node_type: NodeType,
    pub extension: Option<String>,
    pub language: Option<String>,
    pub icon_key: Option<String>,
    pub is_generated: bool,
    pub size_bytes: Option<u64>,
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

    Classification {
        node_type: NodeType::File,
        extension,
        language: language.map(str::to_string),
        icon_key: language.map(str::to_string),
        is_generated,
        size_bytes,
    }
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
