use std::collections::HashSet;

/// Un import tal cual aparece en el código fuente, sin resolver todavía.
pub struct ImportRef {
    pub specifier: String,
}

/// Heurístico de texto (no un parser real de JS/TS/Python): reconoce los
/// patrones habituales de import. Se rompe con imports dinámicos, macros,
/// o sintaxis poco común — es un punto de partida, no un parser completo.
pub fn extract_imports(language: &str, content: &str) -> Vec<ImportRef> {
    let specs = match language {
        "javascript" | "typescript" => extract_js_imports(content),
        "python" => extract_python_imports(content),
        _ => Vec::new(),
    };
    dedupe(specs)
}

fn dedupe(specs: Vec<String>) -> Vec<ImportRef> {
    let mut seen = HashSet::new();
    specs
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .map(|specifier| ImportRef { specifier })
        .collect()
}

fn extract_js_imports(content: &str) -> Vec<String> {
    let mut specs = Vec::new();

    // `... from "X"` / `... from 'X'` (import y export ... from)
    let mut start = 0;
    while let Some(rel) = content[start..].find("from") {
        let idx = start + rel;
        let before_ok = idx == 0 || !is_ident_char(prev_char(content, idx));
        let after_ok = !content[idx + 4..].starts_with(|c: char| is_ident_char(Some(c)));
        if before_ok && after_ok {
            if let Some(spec) = next_quoted_string(&content[idx + 4..]) {
                specs.push(spec);
            }
        }
        start = idx + 4;
    }

    // `require("X")` / `import("X")` (import dinámico)
    for pat in ["require(", "import("] {
        let mut start = 0;
        while let Some(rel) = content[start..].find(pat) {
            let idx = start + rel + pat.len();
            if let Some(spec) = next_quoted_string(&content[idx..]) {
                specs.push(spec);
            }
            start = idx;
        }
    }

    // `import "X"` de solo efecto secundario (sin `from`)
    let mut start = 0;
    while let Some(rel) = content[start..].find("import") {
        let idx = start + rel;
        let before_ok = idx == 0 || !is_ident_char(prev_char(content, idx));
        let rest = content[idx + 6..].trim_start();
        if before_ok && (rest.starts_with('"') || rest.starts_with('\'')) {
            if let Some(spec) = next_quoted_string(rest) {
                specs.push(spec);
            }
        }
        start = idx + 6;
    }

    specs
}

fn is_ident_char(c: Option<char>) -> bool {
    matches!(c, Some(c) if c.is_alphanumeric() || c == '_')
}

fn prev_char(s: &str, byte_idx: usize) -> Option<char> {
    s[..byte_idx].chars().last()
}

fn next_quoted_string(s: &str) -> Option<String> {
    let trimmed = s.trim_start_matches([' ', '(']);
    let mut chars = trimmed.char_indices();
    let (_, quote) = chars.next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &trimmed[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn extract_python_imports(content: &str) -> Vec<String> {
    let mut specs = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("from ") {
            if let Some(module) = rest.split(" import").next() {
                let module = module.trim();
                if !module.is_empty() {
                    specs.push(module.to_string());
                }
            }
        } else if let Some(rest) = line.strip_prefix("import ") {
            for part in rest.split(',') {
                let name = part.trim().split(" as ").next().unwrap_or("").trim();
                if !name.is_empty() {
                    specs.push(name.to_string());
                }
            }
        }
    }
    specs
}

/// Solo resuelve imports **relativos** contra archivos que ya existen en el
/// grafo — imports absolutos/de paquete (`crate::foo`, `import myapp.utils`)
/// necesitarían entender el layout de resolución de módulos del proyecto
/// (tsconfig paths, `src/` layout, etc.) y quedan fuera de alcance por ahora.
pub fn resolve_relative_import(
    importer_rel_path: &str,
    specifier: &str,
    language: &str,
    known_ids: &HashSet<&str>,
) -> Option<String> {
    match language {
        "javascript" | "typescript" => resolve_js_relative(importer_rel_path, specifier, known_ids),
        "python" => resolve_python_relative(importer_rel_path, specifier, known_ids),
        _ => None,
    }
}

const JS_EXTENSIONS: &[&str] = &["", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"];
const JS_INDEX_NAMES: &[&str] = &["index.ts", "index.tsx", "index.js", "index.jsx"];

fn resolve_js_relative(importer_rel_path: &str, spec: &str, known_ids: &HashSet<&str>) -> Option<String> {
    if !(spec.starts_with("./") || spec.starts_with("../")) {
        return None;
    }
    let combined = normalize_path(&parent_dir(importer_rel_path), spec);

    // Coincidencia exacta primero (el specifier ya trae una extensión real).
    if known_ids.contains(combined.as_str()) {
        return Some(combined);
    }

    // TS con módulos ESM importa con extensión ".js" aunque la fuente sea
    // ".ts" (convención del propio TypeScript) — hay que probar swappear
    // antes de tratar el specifier como "sin extensión".
    let without_ext = strip_known_js_extension(&combined);
    for ext in JS_EXTENSIONS {
        let candidate = format!("{without_ext}{ext}");
        if known_ids.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    for index_name in JS_INDEX_NAMES {
        let candidate = format!("{without_ext}/{index_name}");
        if known_ids.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

fn strip_known_js_extension(path: &str) -> &str {
    for ext in JS_EXTENSIONS.iter().skip(1) {
        if let Some(stripped) = path.strip_suffix(ext) {
            return stripped;
        }
    }
    path
}

fn resolve_python_relative(importer_rel_path: &str, spec: &str, known_ids: &HashSet<&str>) -> Option<String> {
    if !spec.starts_with('.') {
        return None;
    }
    let dots = spec.chars().take_while(|&c| c == '.').count();
    let rest = &spec[dots..];

    let mut dir = parent_dir(importer_rel_path);
    for _ in 1..dots {
        dir = parent_dir(&dir);
    }

    let path_part = rest.replace('.', "/");
    let combined = if path_part.is_empty() {
        dir
    } else if dir == "." {
        path_part
    } else {
        format!("{dir}/{path_part}")
    };

    for candidate in [format!("{combined}.py"), format!("{combined}/__init__.py")] {
        if known_ids.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

fn normalize_path(base_dir: &str, spec: &str) -> String {
    let mut segments: Vec<&str> = if base_dir == "." { Vec::new() } else { base_dir.split('/').collect() };
    for part in spec.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    if segments.is_empty() {
        ".".to_string()
    } else {
        segments.join("/")
    }
}

fn parent_dir(rel_path: &str) -> String {
    match rel_path.rfind('/') {
        Some(idx) => rel_path[..idx].to_string(),
        None => ".".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(refs: &[ImportRef]) -> Vec<&str> {
        refs.iter().map(|r| r.specifier.as_str()).collect()
    }

    #[test]
    fn extracts_js_named_and_default_imports() {
        let content = r#"
import React from "react";
import { useState } from './hooks';
export { helper } from "../lib/helper";
"#;
        let found = extract_imports("javascript", content);
        assert_eq!(specs(&found), vec!["react", "./hooks", "../lib/helper"]);
    }

    #[test]
    fn extracts_require_and_dynamic_import() {
        let content = r#"
const fs = require("fs");
const mod = await import('./lazy');
"#;
        let found = extract_imports("javascript", content);
        assert_eq!(specs(&found), vec!["fs", "./lazy"]);
    }

    #[test]
    fn extracts_side_effect_import() {
        let content = "import './styles.css';\n";
        let found = extract_imports("javascript", content);
        assert_eq!(specs(&found), vec!["./styles.css"]);
    }

    #[test]
    fn extracts_python_imports() {
        let content = "import os\nimport numpy as np, sys\nfrom . import utils\nfrom .helpers import thing\nfrom ..pkg.sub import other\nfrom collections import OrderedDict\n";
        let found = extract_imports("python", content);
        assert_eq!(
            specs(&found),
            vec!["os", "numpy", "sys", ".", ".helpers", "..pkg.sub", "collections"]
        );
    }

    #[test]
    fn resolves_js_relative_sibling_file() {
        let mut known = HashSet::new();
        known.insert("src/utils.ts");
        let resolved = resolve_relative_import("src/app.ts", "./utils", "typescript", &known);
        assert_eq!(resolved.as_deref(), Some("src/utils.ts"));
    }

    #[test]
    fn resolves_ts_esm_dot_js_specifier_to_ts_source() {
        // TypeScript con módulos ESM importa con extensión ".js" aunque la
        // fuente real sea ".ts" — descubierto probando Ariadne contra sí mismo.
        let mut known = HashSet::new();
        known.insert("src/data.ts");
        let resolved = resolve_relative_import("src/main.ts", "./data.js", "typescript", &known);
        assert_eq!(resolved.as_deref(), Some("src/data.ts"));
    }

    #[test]
    fn resolves_js_relative_index_file() {
        let mut known = HashSet::new();
        known.insert("src/lib/index.js");
        let resolved = resolve_relative_import("src/app.js", "./lib", "javascript", &known);
        assert_eq!(resolved.as_deref(), Some("src/lib/index.js"));
    }

    #[test]
    fn resolves_js_relative_parent_dir() {
        let mut known = HashSet::new();
        known.insert("lib/helper.js");
        let resolved = resolve_relative_import("src/app.js", "../lib/helper", "javascript", &known);
        assert_eq!(resolved.as_deref(), Some("lib/helper.js"));
    }

    #[test]
    fn does_not_resolve_bare_package_specifier() {
        let known = HashSet::new();
        assert_eq!(resolve_relative_import("src/app.js", "react", "javascript", &known), None);
    }

    #[test]
    fn resolves_python_same_package_relative() {
        let mut known = HashSet::new();
        known.insert("pkg/utils.py");
        let resolved = resolve_relative_import("pkg/app.py", ".utils", "python", &known);
        assert_eq!(resolved.as_deref(), Some("pkg/utils.py"));
    }

    #[test]
    fn resolves_python_parent_package_relative() {
        let mut known = HashSet::new();
        known.insert("pkg/helpers/__init__.py");
        let resolved = resolve_relative_import("pkg/sub/app.py", "..helpers", "python", &known);
        assert_eq!(resolved.as_deref(), Some("pkg/helpers/__init__.py"));
    }

    #[test]
    fn resolves_python_dot_only_import() {
        let mut known = HashSet::new();
        known.insert("pkg/__init__.py");
        let resolved = resolve_relative_import("pkg/app.py", ".", "python", &known);
        assert_eq!(resolved.as_deref(), Some("pkg/__init__.py"));
    }

    #[test]
    fn does_not_resolve_absolute_python_import() {
        let known = HashSet::new();
        assert_eq!(resolve_relative_import("pkg/app.py", "myapp.utils", "python", &known), None);
    }
}
