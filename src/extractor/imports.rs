use std::collections::{HashMap, HashSet};

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
        "rust" => extract_rust_imports(content),
        "go" => extract_go_imports(content),
        "java" | "kotlin" => extract_java_kotlin_imports(content),
        "php" => extract_php_imports(content),
        "ruby" => extract_ruby_imports(content),
        "c" | "cplusplus" => extract_c_imports(content),
        "elixir" => extract_elixir_imports(content),
        "haskell" => extract_haskell_imports(content),
        "julia" => extract_julia_imports(content),
        "r" => extract_r_imports(content),
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

/// `mod foo;` declara un submódulo en un archivo hermano — se marca con el
/// prefijo `mod:` para distinguirlo de un `use` en la resolución. Formas de
/// visibilidad (`pub`, `pub(crate)`, `pub(super)`) se ignoran; `mod foo { .. }`
/// inline (sin `;`) no declara un archivo aparte, así que no cuenta.
fn extract_rust_mod_decl(line: &str) -> Option<String> {
    let after_vis = ["pub(crate) ", "pub(super) ", "pub "]
        .iter()
        .find_map(|p| line.strip_prefix(p))
        .unwrap_or(line);
    let rest = after_vis.strip_prefix("mod ")?.trim();
    let name = rest.strip_suffix(';')?.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some(name.to_string())
}

fn extract_rust_use_path(line: &str) -> Option<String> {
    let after_vis = line.strip_prefix("pub ").unwrap_or(line);
    let rest = after_vis.strip_prefix("use ")?;
    let end = rest.find([';', '{']).unwrap_or(rest.len());
    let mut path = rest[..end].trim();
    if let Some(as_idx) = path.find(" as ") {
        path = &path[..as_idx];
    }
    let path = path.trim_end_matches("::").trim();
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

fn extract_rust_imports(content: &str) -> Vec<String> {
    let mut specs = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if let Some(name) = extract_rust_mod_decl(line) {
            specs.push(format!("mod:{name}"));
        } else if let Some(path) = extract_rust_use_path(line) {
            specs.push(path);
        }
    }
    specs
}

fn extract_go_imports(content: &str) -> Vec<String> {
    let mut specs = Vec::new();
    let mut in_block = false;
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("import ") {
            let rest = rest.trim();
            if rest == "(" {
                in_block = true;
            } else if let Some(spec) = quoted_substring(rest) {
                specs.push(spec);
            }
            continue;
        }
        if in_block {
            if line.starts_with(')') {
                in_block = false;
            } else if let Some(spec) = quoted_substring(line) {
                specs.push(spec);
            }
        }
    }
    specs
}

/// Busca la primera comilla (simple o doble, lo que aparezca primero) y
/// devuelve lo que hay hasta la siguiente del mismo tipo. Go siempre usa
/// dobles; Ruby acepta ambas indistintamente.
fn quoted_substring(s: &str) -> Option<String> {
    let (start, quote) = s.char_indices().find(|(_, c)| *c == '"' || *c == '\'')?;
    let rest = &s[start + quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

/// Java y Kotlin comparten sintaxis de import (`import a.b.C;`, Kotlin sin
/// `;` obligatorio). `import static X.Y.z;` (Java) también cuenta — la
/// clase de todas formas resuelve igual por convención paquete=carpeta.
fn extract_java_kotlin_imports(content: &str) -> Vec<String> {
    let mut specs = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        let Some(rest) = line.strip_prefix("import ") else { continue };
        let rest = rest.strip_prefix("static ").unwrap_or(rest).trim();
        let end = rest.find(';').unwrap_or(rest.len());
        let path = rest[..end].trim().trim_end_matches(".*");
        if !path.is_empty() {
            specs.push(path.to_string());
        }
    }
    specs
}

fn extract_php_imports(content: &str) -> Vec<String> {
    let mut specs = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        let Some(rest) = line.strip_prefix("use ") else { continue };
        let end = rest.find(';').unwrap_or(rest.len());
        let mut path = rest[..end].trim();
        if let Some(as_idx) = path.find(" as ") {
            path = &path[..as_idx];
        }
        let path = path.trim_start_matches('\\').trim();
        if !path.is_empty() {
            specs.push(path.to_string());
        }
    }
    specs
}

/// `require_relative` es siempre relativo al archivo (se marca `rel:`);
/// `require` sin relativo se resuelve luego contra `lib/` o contra Gemfile.
fn extract_ruby_imports(content: &str) -> Vec<String> {
    let mut specs = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("require_relative ") {
            if let Some(spec) = quoted_substring(rest) {
                specs.push(format!("rel:{spec}"));
            }
        } else if let Some(rest) = line.strip_prefix("require ") {
            if let Some(spec) = quoted_substring(rest) {
                specs.push(spec);
            }
        }
    }
    specs
}

/// Solo `#include "..."` (comillas) — es relativo al archivo actual por
/// definición del propio lenguaje. `#include <...>` (librería del sistema o
/// de un include-path externo) se deja fuera, no hay una convención de
/// proyecto confiable para resolverlo.
fn extract_c_imports(content: &str) -> Vec<String> {
    let mut specs = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        let Some(rest) = line.strip_prefix("#include") else { continue };
        let rest = rest.trim_start();
        if !rest.starts_with('"') {
            continue;
        }
        if let Some(spec) = quoted_substring(rest) {
            specs.push(spec);
        }
    }
    specs
}

/// `alias`/`import`/`use`/`require Foo.Bar` — cualquiera de las cuatro
/// formas de Elixir para referenciar otro módulo por su nombre completo.
fn extract_elixir_imports(content: &str) -> Vec<String> {
    let mut specs = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        for kw in ["alias ", "import ", "use ", "require "] {
            let Some(rest) = line.strip_prefix(kw) else { continue };
            let end = rest.find([',', '(', '{']).unwrap_or(rest.len());
            let mut path = rest[..end].trim().trim_end_matches('.');
            if let Some(idx) = path.find(' ') {
                path = &path[..idx];
            }
            let path = path.trim_end_matches(';');
            if path.chars().next().is_some_and(|c| c.is_uppercase()) {
                specs.push(path.to_string());
            }
            break;
        }
    }
    specs
}

fn extract_haskell_imports(content: &str) -> Vec<String> {
    let mut specs = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        let Some(rest) = line.strip_prefix("import ") else { continue };
        let rest = rest.strip_prefix("qualified ").unwrap_or(rest).trim();
        let end = rest.find([' ', '(']).unwrap_or(rest.len());
        let path = rest[..end].trim();
        if !path.is_empty() {
            specs.push(path.to_string());
        }
    }
    specs
}

/// `include("foo.jl")` es una referencia a archivo relativa (se marca
/// `include:`); `using`/`import X` referencian un paquete registrado en
/// `Project.toml`, igual que un import externo de cualquier otro lenguaje.
fn extract_julia_imports(content: &str) -> Vec<String> {
    let mut specs = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("include(") {
            if let Some(spec) = next_quoted_string(rest) {
                specs.push(format!("include:{spec}"));
            }
        } else if let Some(rest) = line.strip_prefix("using ").or_else(|| line.strip_prefix("import ")) {
            let name = rest.split([',', ':', ' ']).next().unwrap_or("").trim();
            if !name.is_empty() {
                specs.push(name.to_string());
            }
        }
    }
    specs
}

/// `source("foo.R")` es una referencia a archivo relativa (se marca
/// `source:`); `library(x)`/`require(x)` referencian un paquete de R, que
/// ya parseamos desde `DESCRIPTION` como cualquier otro manifiesto.
fn extract_r_imports(content: &str) -> Vec<String> {
    let mut specs = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("source(") {
            if let Some(spec) = next_quoted_string(rest) {
                specs.push(format!("source:{spec}"));
            }
        } else if let Some(rest) = line.strip_prefix("library(").or_else(|| line.strip_prefix("require(")) {
            let name_part = rest.split(')').next().unwrap_or("").trim();
            let name = name_part.trim_matches(|c| c == '"' || c == '\'').trim();
            if !name.is_empty() {
                specs.push(name.to_string());
            }
        }
    }
    specs
}

/// Nombre de paquete de "primer nivel" de un specifier, para cruzarlo contra
/// las dependencias declaradas en el manifiesto más cercano. En Python es el
/// primer segmento antes del punto (`ortools.constraint_solver` -> `ortools`);
/// en JS/TS es el paquete completo si es scoped (`@types/node`) o el primer
/// segmento si no (`lodash/debounce` -> `lodash`).
pub fn top_level_package_name(language: &str, specifier: &str) -> String {
    if matches!(language, "python" | "julia" | "r") {
        return specifier.split('.').next().unwrap_or(specifier).to_string();
    }
    if language == "rust" {
        return specifier.split("::").next().unwrap_or(specifier).to_string();
    }
    if language == "elixir" {
        // El nombre del paquete Hex es por convención el primer segmento del
        // módulo en snake_case (p. ej. `Phoenix.Controller` -> `phoenix`,
        // `ExAws.S3` -> `ex_aws`), no el átomo Erlang usado en mix.exs.
        let first = specifier.split('.').next().unwrap_or(specifier);
        return camel_to_snake(first);
    }
    if specifier.starts_with('@') {
        let mut parts = specifier.splitn(3, '/');
        let scope = parts.next().unwrap_or(specifier);
        return match parts.next() {
            Some(name) => format!("{scope}/{name}"),
            None => specifier.to_string(),
        };
    }
    specifier.split('/').next().unwrap_or(specifier).to_string()
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
        "ruby" => {
            let spec = specifier.strip_prefix("rel:")?;
            resolve_ruby_relative(importer_rel_path, spec, known_ids)
        }
        "c" | "cplusplus" => resolve_c_include(importer_rel_path, specifier, known_ids),
        "julia" => {
            let spec = specifier.strip_prefix("include:")?;
            resolve_julia_include(importer_rel_path, spec, known_ids)
        }
        "r" => {
            let spec = specifier.strip_prefix("source:")?;
            resolve_r_source(importer_rel_path, spec, known_ids)
        }
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

/// Resuelve un import **absoluto** de Python contra paquetes propios del
/// proyecto — no contra archivos por ruta relativa como `resolve_python_relative`.
/// La heurística: cualquier carpeta con `__init__.py` es un paquete, y su
/// nombre es el nombre de la carpeta (la misma convención que usa el propio
/// Python/pip). `mapo_core.db` con un paquete registrado `mapo_core` ->
/// `.../mapo_core/db.py`. Si el mismo nombre de paquete aparece en más de un
/// lugar del proyecto, gana el que se haya registrado último (ambigüedad
/// rara en la práctica: dos paquetes instalables con el mismo nombre no
/// pueden coexistir realmente).
pub fn resolve_python_absolute(specifier: &str, package_roots: &HashMap<String, String>, known_ids: &HashSet<&str>) -> Option<String> {
    if specifier.starts_with('.') {
        return None;
    }
    let mut parts = specifier.split('.');
    let top = parts.next()?;
    let package_dir = package_roots.get(top)?;
    let rest: Vec<&str> = parts.collect();

    let combined = if rest.is_empty() {
        package_dir.clone()
    } else {
        format!("{package_dir}/{}", rest.join("/"))
    };

    for candidate in [format!("{combined}.py"), format!("{combined}/__init__.py")] {
        if known_ids.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

/// Resuelve un `mod:` o un `use` de Rust. `crate::` necesita `crate_root`
/// (la carpeta `src/` del Cargo.toml más cercano, calculada en build_graph);
/// `self::`/`super::` son siempre resolubles sin contexto externo. Un `use`
/// de un crate externo (`serde::...`) no matchea ninguno de los tres
/// prefijos y cae al mecanismo genérico de manifiesto más abajo.
pub fn resolve_rust(importer_rel_path: &str, specifier: &str, crate_root: Option<&str>, known_ids: &HashSet<&str>) -> Option<String> {
    if let Some(name) = specifier.strip_prefix("mod:") {
        let dir = parent_dir(importer_rel_path);
        return resolve_rust_dir_path(&dir, name, known_ids);
    }
    if let Some(rest) = specifier.strip_prefix("crate::") {
        return resolve_rust_dir_path(crate_root?, rest, known_ids);
    }
    if let Some(rest) = specifier.strip_prefix("self::") {
        let dir = parent_dir(importer_rel_path);
        return resolve_rust_dir_path(&dir, rest, known_ids);
    }
    if let Some(rest) = specifier.strip_prefix("super::") {
        let dir = parent_dir(&parent_dir(importer_rel_path));
        return resolve_rust_dir_path(&dir, rest, known_ids);
    }
    None
}

fn resolve_rust_dir_path(base_dir: &str, dotted: &str, known_ids: &HashSet<&str>) -> Option<String> {
    let path_part = dotted.replace("::", "/");
    let combined = if path_part.is_empty() {
        base_dir.to_string()
    } else if base_dir == "." {
        path_part
    } else {
        format!("{base_dir}/{path_part}")
    };
    for candidate in [format!("{combined}.rs"), format!("{combined}/mod.rs")] {
        if known_ids.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

/// En Go un import referencia un **paquete** (carpeta), no un archivo
/// puntual — así que esto conecta el archivo importador con un nodo
/// `directory`, no con otro archivo. `module_name` sale de la línea
/// `module ...` de `go.mod`.
pub fn resolve_go_package(specifier: &str, module_name: &str, known_ids: &HashSet<&str>) -> Option<String> {
    let rest = specifier.strip_prefix(module_name)?;
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    let dir_id = if rest.is_empty() { ".".to_string() } else { rest.to_string() };
    known_ids.contains(dir_id.as_str()).then_some(dir_id)
}

/// Java/Kotlin: el paquete declarado determina la ruta bajo un "source
/// root" (`src/main/java`, `src/main/kotlin`, etc., detectados en
/// build_graph). `com.foo.MyClass` -> `{root}/com/foo/MyClass.{ext}`.
pub fn resolve_java_kotlin_absolute(specifier: &str, source_roots: &[String], extension: &str, known_ids: &HashSet<&str>) -> Option<String> {
    let path_part = specifier.replace('.', "/");
    for root in source_roots {
        let combined = if root == "." { path_part.clone() } else { format!("{root}/{path_part}") };
        let candidate = format!("{combined}.{extension}");
        if known_ids.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

/// PSR-4 de `composer.json`: prefijo de namespace -> carpeta, relativa al
/// directorio del propio `composer.json`. `manifest_dir` es ese directorio.
pub fn resolve_php_absolute(specifier: &str, psr4_map: &[(String, String)], manifest_dir: &str, known_ids: &HashSet<&str>) -> Option<String> {
    for (prefix, dir) in psr4_map {
        let Some(rest) = specifier.strip_prefix(prefix.as_str()) else { continue };
        let rest = rest.strip_prefix('\\').unwrap_or(rest);
        let path_part = rest.replace('\\', "/");
        let combined_dir = if manifest_dir == "." { dir.clone() } else { format!("{manifest_dir}/{dir}") };
        let combined = if path_part.is_empty() { combined_dir } else { format!("{combined_dir}/{path_part}") };
        let candidate = format!("{combined}.php");
        if known_ids.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

const RUBY_EXTENSIONS: &[&str] = &["", ".rb"];

/// `require_relative` (se le quitó el prefijo `rel:` antes de llamar acá) es
/// siempre relativo al archivo, sin necesitar `./` — Ruby lo asume.
pub fn resolve_ruby_relative(importer_rel_path: &str, spec: &str, known_ids: &HashSet<&str>) -> Option<String> {
    let combined = normalize_path(&parent_dir(importer_rel_path), spec);
    for ext in RUBY_EXTENSIONS {
        let candidate = format!("{combined}{ext}");
        if known_ids.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

/// `#include "foo.h"` ya trae la extensión escrita y es relativo al archivo
/// actual sin necesitar `./` — así lo define el propio C/C++.
pub fn resolve_c_include(importer_rel_path: &str, spec: &str, known_ids: &HashSet<&str>) -> Option<String> {
    let combined = normalize_path(&parent_dir(importer_rel_path), spec);
    known_ids.contains(combined.as_str()).then_some(combined)
}

/// Elixir: cualquier carpeta llamada `lib` (convención de Mix) es una raíz
/// de módulos. `Foo.BarBaz` -> `foo/bar_baz` (CamelCase a snake_case, la
/// misma conversión que usa `Macro.underscore/1`) -> `{root}/foo/bar_baz.ex`.
pub fn resolve_elixir_absolute(specifier: &str, lib_roots: &[String], known_ids: &HashSet<&str>) -> Option<String> {
    let path_part = specifier.split('.').map(camel_to_snake).collect::<Vec<_>>().join("/");
    for root in lib_roots {
        let combined = if root == "." { path_part.clone() } else { format!("{root}/{path_part}") };
        let candidate = format!("{combined}.ex");
        if known_ids.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

fn camel_to_snake(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::with_capacity(s.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev_lower = i > 0 && chars[i - 1].is_lowercase();
            let starts_new_word = i > 0 && chars[i - 1].is_uppercase() && chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if prev_lower || starts_new_word {
                result.push('_');
            }
            result.extend(c.to_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Haskell: `Foo.Bar` -> `Foo/Bar.hs` (sin cambio de mayúsculas, a
/// diferencia de Elixir) bajo alguna de las carpetas fuente detectadas
/// (`src/`, o la raíz del proyecto como respaldo).
pub fn resolve_haskell_absolute(specifier: &str, source_roots: &[String], known_ids: &HashSet<&str>) -> Option<String> {
    let path_part = specifier.replace('.', "/");
    for root in source_roots {
        let combined = if root == "." { path_part.clone() } else { format!("{root}/{path_part}") };
        let candidate = format!("{combined}.hs");
        if known_ids.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

const JULIA_EXTENSIONS: &[&str] = &["", ".jl"];

/// `include(...)` (se le quitó el prefijo `include:` antes de llamar acá)
/// es relativo al archivo actual, sin necesitar `./`.
pub fn resolve_julia_include(importer_rel_path: &str, spec: &str, known_ids: &HashSet<&str>) -> Option<String> {
    let combined = normalize_path(&parent_dir(importer_rel_path), spec);
    for ext in JULIA_EXTENSIONS {
        let candidate = format!("{combined}{ext}");
        if known_ids.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

const R_EXTENSIONS: &[&str] = &["", ".R", ".r"];

/// `source(...)` (se le quitó el prefijo `source:` antes de llamar acá).
/// A diferencia de los demás, `source()` en R por convención suele ser
/// relativo al directorio de trabajo del proyecto, no al archivo que lo
/// llama — se prueban ambas interpretaciones.
pub fn resolve_r_source(importer_rel_path: &str, spec: &str, known_ids: &HashSet<&str>) -> Option<String> {
    let file_relative = normalize_path(&parent_dir(importer_rel_path), spec);
    let root_relative = normalize_path(".", spec);
    for combined in [file_relative, root_relative] {
        for ext in R_EXTENSIONS {
            let candidate = format!("{combined}{ext}");
            if known_ids.contains(candidate.as_str()) {
                return Some(candidate);
            }
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
    fn top_level_name_python_dotted() {
        assert_eq!(top_level_package_name("python", "ortools.constraint_solver"), "ortools");
        assert_eq!(top_level_package_name("python", "numpy"), "numpy");
    }

    #[test]
    fn top_level_name_elixir_hex_package() {
        assert_eq!(top_level_package_name("elixir", "Phoenix.Controller"), "phoenix");
        assert_eq!(top_level_package_name("elixir", "ExAws.S3"), "ex_aws");
        assert_eq!(top_level_package_name("elixir", "Jason"), "jason");
    }

    #[test]
    fn top_level_name_js_plain_and_subpath() {
        assert_eq!(top_level_package_name("javascript", "react"), "react");
        assert_eq!(top_level_package_name("javascript", "lodash/debounce"), "lodash");
    }

    #[test]
    fn top_level_name_js_scoped_package() {
        assert_eq!(top_level_package_name("typescript", "@types/d3-hierarchy"), "@types/d3-hierarchy");
        assert_eq!(top_level_package_name("typescript", "@types/d3-hierarchy/extra"), "@types/d3-hierarchy");
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

    #[test]
    fn resolves_python_absolute_import_via_package_root() {
        let mut known = HashSet::new();
        known.insert("mapo_core/src/mapo_core/db.py");
        known.insert("mapo_core/src/mapo_core/__init__.py");
        let mut roots = HashMap::new();
        roots.insert("mapo_core".to_string(), "mapo_core/src/mapo_core".to_string());

        let resolved = resolve_python_absolute("mapo_core.db", &roots, &known);
        assert_eq!(resolved.as_deref(), Some("mapo_core/src/mapo_core/db.py"));
    }

    #[test]
    fn resolves_python_absolute_bare_package_to_init() {
        let mut known = HashSet::new();
        known.insert("mapo_core/src/mapo_core/__init__.py");
        let mut roots = HashMap::new();
        roots.insert("mapo_core".to_string(), "mapo_core/src/mapo_core".to_string());

        let resolved = resolve_python_absolute("mapo_core", &roots, &known);
        assert_eq!(resolved.as_deref(), Some("mapo_core/src/mapo_core/__init__.py"));
    }

    #[test]
    fn does_not_resolve_unknown_top_level_package() {
        let known = HashSet::new();
        let roots = HashMap::new();
        assert_eq!(resolve_python_absolute("numpy", &roots, &known), None);
    }

    // --- Rust ---

    #[test]
    fn extracts_rust_mod_and_use() {
        let content = "mod foo;\npub mod bar;\npub(crate) mod baz;\nuse crate::a::B;\nuse self::c::D;\nuse super::E;\nuse std::collections::HashMap;\nmod inline { fn x() {} }\n";
        let specs = extract_imports("rust", content);
        let found: Vec<&str> = specs.iter().map(|s| s.specifier.as_str()).collect();
        assert_eq!(
            found,
            vec!["mod:foo", "mod:bar", "mod:baz", "crate::a::B", "self::c::D", "super::E", "std::collections::HashMap"]
        );
    }

    #[test]
    fn resolves_rust_mod_sibling() {
        let mut known = HashSet::new();
        known.insert("src/extractor/foo.rs");
        let resolved = resolve_rust("src/extractor/mod.rs", "mod:foo", None, &known);
        assert_eq!(resolved.as_deref(), Some("src/extractor/foo.rs"));
    }

    #[test]
    fn resolves_rust_crate_root_path() {
        let mut known = HashSet::new();
        known.insert("src/extractor/build_graph.rs");
        let resolved = resolve_rust("src/main.rs", "crate::extractor::build_graph", Some("src"), &known);
        assert_eq!(resolved.as_deref(), Some("src/extractor/build_graph.rs"));
    }

    #[test]
    fn resolves_rust_super_path() {
        let mut known = HashSet::new();
        known.insert("src/schema.rs");
        let resolved = resolve_rust("src/extractor/build_graph.rs", "super::schema", None, &known);
        assert_eq!(resolved.as_deref(), Some("src/schema.rs"));
    }

    // --- Go ---

    #[test]
    fn extracts_go_single_and_block_imports() {
        let content = "import \"fmt\"\nimport (\n\t\"myproject/internal/utils\"\n\t\"github.com/foo/bar\"\n)\n";
        let specs = extract_imports("go", content);
        let found: Vec<&str> = specs.iter().map(|s| s.specifier.as_str()).collect();
        assert_eq!(found, vec!["fmt", "myproject/internal/utils", "github.com/foo/bar"]);
    }

    #[test]
    fn resolves_go_package_to_directory() {
        let mut known = HashSet::new();
        known.insert("internal/utils");
        let resolved = resolve_go_package("myproject/internal/utils", "myproject", &known);
        assert_eq!(resolved.as_deref(), Some("internal/utils"));
    }

    #[test]
    fn resolves_go_package_root_itself() {
        let mut known = HashSet::new();
        known.insert(".");
        let resolved = resolve_go_package("myproject", "myproject", &known);
        assert_eq!(resolved.as_deref(), Some("."));
    }

    // --- Java / Kotlin ---

    #[test]
    fn extracts_java_kotlin_imports() {
        let content = "import com.foo.MyClass;\nimport static com.foo.Utils.helper;\nimport com.foo.pkg.*;\n";
        let specs = extract_imports("java", content);
        let found: Vec<&str> = specs.iter().map(|s| s.specifier.as_str()).collect();
        assert_eq!(found, vec!["com.foo.MyClass", "com.foo.Utils.helper", "com.foo.pkg"]);
    }

    #[test]
    fn resolves_java_absolute_via_source_root() {
        let mut known = HashSet::new();
        known.insert("src/main/java/com/foo/MyClass.java");
        let roots = vec!["src/main/java".to_string()];
        let resolved = resolve_java_kotlin_absolute("com.foo.MyClass", &roots, "java", &known);
        assert_eq!(resolved.as_deref(), Some("src/main/java/com/foo/MyClass.java"));
    }

    // --- PHP ---

    #[test]
    fn extracts_php_use_statements() {
        let content = "use App\\Models\\User;\nuse App\\Http\\Controllers\\HomeController as Home;\n";
        let specs = extract_imports("php", content);
        let found: Vec<&str> = specs.iter().map(|s| s.specifier.as_str()).collect();
        assert_eq!(found, vec!["App\\Models\\User", "App\\Http\\Controllers\\HomeController"]);
    }

    #[test]
    fn resolves_php_via_psr4_map() {
        let mut known = HashSet::new();
        known.insert("src/Models/User.php");
        let psr4 = vec![("App".to_string(), "src".to_string())];
        let resolved = resolve_php_absolute("App\\Models\\User", &psr4, ".", &known);
        assert_eq!(resolved.as_deref(), Some("src/Models/User.php"));
    }

    // --- Ruby ---

    #[test]
    fn extracts_ruby_requires() {
        let content = "require_relative 'helpers/foo'\nrequire 'json'\n";
        let specs = extract_imports("ruby", content);
        let found: Vec<&str> = specs.iter().map(|s| s.specifier.as_str()).collect();
        assert_eq!(found, vec!["rel:helpers/foo", "json"]);
    }

    #[test]
    fn resolves_ruby_require_relative() {
        let mut known = HashSet::new();
        known.insert("lib/helpers/foo.rb");
        let resolved = resolve_relative_import("lib/main.rb", "rel:helpers/foo", "ruby", &known);
        assert_eq!(resolved.as_deref(), Some("lib/helpers/foo.rb"));
    }

    // --- C/C++ ---

    #[test]
    fn extracts_c_quote_includes_only() {
        let content = "#include \"foo.h\"\n#include <stdio.h>\n#include \"sub/bar.h\"\n";
        let specs = extract_imports("c", content);
        let found: Vec<&str> = specs.iter().map(|s| s.specifier.as_str()).collect();
        assert_eq!(found, vec!["foo.h", "sub/bar.h"]);
    }

    #[test]
    fn resolves_c_include_relative() {
        let mut known = HashSet::new();
        known.insert("src/foo.h");
        let resolved = resolve_relative_import("src/main.c", "foo.h", "c", &known);
        assert_eq!(resolved.as_deref(), Some("src/foo.h"));
    }

    // --- Elixir ---

    #[test]
    fn extracts_elixir_alias_import_use_require() {
        let content = "alias MapoWeb.PageController\nimport Ecto.Query\nuse MapoWeb, :controller\nrequire Logger\nalias Foo.{Bar, Baz}\n";
        let specs = extract_imports("elixir", content);
        let found: Vec<&str> = specs.iter().map(|s| s.specifier.as_str()).collect();
        assert_eq!(found, vec!["MapoWeb.PageController", "Ecto.Query", "MapoWeb", "Logger", "Foo"]);
    }

    #[test]
    fn camel_to_snake_handles_acronyms() {
        assert_eq!(camel_to_snake("PageController"), "page_controller");
        assert_eq!(camel_to_snake("MapoWeb"), "mapo_web");
        assert_eq!(camel_to_snake("HTTPClient"), "http_client");
    }

    #[test]
    fn resolves_elixir_absolute_via_lib_root() {
        let mut known = HashSet::new();
        known.insert("lib/mapo_web/page_controller.ex");
        let roots = vec!["lib".to_string()];
        let resolved = resolve_elixir_absolute("MapoWeb.PageController", &roots, &known);
        assert_eq!(resolved.as_deref(), Some("lib/mapo_web/page_controller.ex"));
    }

    // --- Haskell ---

    #[test]
    fn extracts_haskell_imports() {
        let content = "import Data.List\nimport qualified Data.Map as Map\nimport MyApp.Utils (helper)\n";
        let specs = extract_imports("haskell", content);
        let found: Vec<&str> = specs.iter().map(|s| s.specifier.as_str()).collect();
        assert_eq!(found, vec!["Data.List", "Data.Map", "MyApp.Utils"]);
    }

    #[test]
    fn resolves_haskell_absolute_via_source_root() {
        let mut known = HashSet::new();
        known.insert("src/MyApp/Utils.hs");
        let roots = vec!["src".to_string()];
        let resolved = resolve_haskell_absolute("MyApp.Utils", &roots, &known);
        assert_eq!(resolved.as_deref(), Some("src/MyApp/Utils.hs"));
    }

    // --- Julia ---

    #[test]
    fn extracts_julia_include_and_using() {
        let content = "include(\"utils.jl\")\nusing DataFrames\nimport JSON: parse\n";
        let specs = extract_imports("julia", content);
        let found: Vec<&str> = specs.iter().map(|s| s.specifier.as_str()).collect();
        assert_eq!(found, vec!["include:utils.jl", "DataFrames", "JSON"]);
    }

    #[test]
    fn resolves_julia_include_relative() {
        let mut known = HashSet::new();
        known.insert("src/utils.jl");
        let resolved = resolve_relative_import("src/Main.jl", "include:utils.jl", "julia", &known);
        assert_eq!(resolved.as_deref(), Some("src/utils.jl"));
    }

    // --- R ---

    #[test]
    fn extracts_r_source_and_library() {
        let content = "source(\"helpers.R\")\nlibrary(dplyr)\nrequire(\"ggplot2\")\n";
        let specs = extract_imports("r", content);
        let found: Vec<&str> = specs.iter().map(|s| s.specifier.as_str()).collect();
        assert_eq!(found, vec!["source:helpers.R", "dplyr", "ggplot2"]);
    }

    #[test]
    fn resolves_r_source_file_relative() {
        let mut known = HashSet::new();
        known.insert("R/helpers.R");
        let resolved = resolve_relative_import("R/main.R", "source:helpers.R", "r", &known);
        assert_eq!(resolved.as_deref(), Some("R/helpers.R"));
    }

    #[test]
    fn resolves_r_source_root_relative_fallback() {
        let mut known = HashSet::new();
        known.insert("R/helpers.R");
        // el archivo que llama a source() está en la raíz del proyecto, pero
        // el path del source() ya es relativo a la raíz, no al archivo
        let resolved = resolve_relative_import("main.R", "source:R/helpers.R", "r", &known);
        assert_eq!(resolved.as_deref(), Some("R/helpers.R"));
    }
}
