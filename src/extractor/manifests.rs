use serde_json::Value as JsonValue;
use std::collections::HashSet;

/// Una dependencia declarada en un manifiesto, con la versión tal cual
/// aparece ahí (puede ser un rango/especificador, no un valor pineado).
pub struct LibraryDep {
    pub name: String,
    pub version: Option<String>,
}

enum ManifestKind {
    CargoToml,
    PackageJson,
    RequirementsTxt,
    PyprojectToml,
    GoMod,
    MixExs,
    PomXml,
    Gradle,
    RDescription,
    ComposerJson,
    ProjectToml,
}

fn manifest_kind_for(filename: &str) -> Option<ManifestKind> {
    match filename {
        "Cargo.toml" => Some(ManifestKind::CargoToml),
        "package.json" => Some(ManifestKind::PackageJson),
        "requirements.txt" => Some(ManifestKind::RequirementsTxt),
        "pyproject.toml" => Some(ManifestKind::PyprojectToml),
        "go.mod" => Some(ManifestKind::GoMod),
        "mix.exs" => Some(ManifestKind::MixExs),
        "pom.xml" => Some(ManifestKind::PomXml),
        "build.gradle" | "build.gradle.kts" => Some(ManifestKind::Gradle),
        "DESCRIPTION" => Some(ManifestKind::RDescription),
        "composer.json" => Some(ManifestKind::ComposerJson),
        "Project.toml" => Some(ManifestKind::ProjectToml),
        _ => None,
    }
}

pub fn is_manifest_file(filename: &str) -> bool {
    manifest_kind_for(filename).is_some()
}

/// Con qué lenguaje de import (ver extractor::imports) se corresponde un
/// manifiesto — permite cruzar los imports de un archivo contra las
/// dependencias declaradas en el manifiesto más cercano. `None` para
/// lenguajes donde todavía no extraemos imports.
pub fn manifest_import_language(filename: &str) -> Option<&'static str> {
    match manifest_kind_for(filename)? {
        ManifestKind::PackageJson => Some("javascript"),
        ManifestKind::RequirementsTxt | ManifestKind::PyprojectToml => Some("python"),
        ManifestKind::ComposerJson => Some("php"),
        ManifestKind::ProjectToml => Some("julia"),
        ManifestKind::RDescription => Some("r"),
        ManifestKind::CargoToml => Some("rust"),
        ManifestKind::GoMod => Some("go"),
        ManifestKind::MixExs => Some("elixir"),
        ManifestKind::PomXml | ManifestKind::Gradle => Some("java"),
    }
}

/// Extrae las dependencias declaradas en un manifiesto reconocido. Cada
/// formato tiene su propio parser; los basados en datos (TOML/JSON) son
/// exactos, los basados en código (mix.exs, Gradle) son heurísticos por
/// regex/escaneo manual — no hay parser real de Elixir/Kotlin/Groovy acá,
/// solo reconocimiento del patrón usual de declarar dependencias.
pub fn parse_manifest(filename: &str, content: &str) -> Vec<LibraryDep> {
    let deps = match manifest_kind_for(filename) {
        Some(ManifestKind::CargoToml) => parse_cargo_toml(content),
        Some(ManifestKind::PackageJson) => parse_package_json(content),
        Some(ManifestKind::RequirementsTxt) => parse_requirements_txt(content),
        Some(ManifestKind::PyprojectToml) => parse_pyproject_toml(content),
        Some(ManifestKind::GoMod) => parse_go_mod(content),
        Some(ManifestKind::MixExs) => parse_mix_exs(content),
        Some(ManifestKind::PomXml) => parse_pom_xml(content),
        Some(ManifestKind::Gradle) => parse_gradle(content),
        Some(ManifestKind::RDescription) => parse_r_description(content),
        Some(ManifestKind::ComposerJson) => parse_composer_json(content),
        Some(ManifestKind::ProjectToml) => parse_project_toml(content),
        None => Vec::new(),
    };
    dedupe(deps)
}

fn dedupe(deps: Vec<LibraryDep>) -> Vec<LibraryDep> {
    let mut seen = HashSet::new();
    deps.into_iter().filter(|d| seen.insert(d.name.clone())).collect()
}

/// Si `Cargo.toml` declara `[package]` es un crate real (su carpeta tiene un
/// `src/` propio); si solo tiene `[workspace]` es un manifiesto raíz de
/// workspace puro, y no debe usarse como raíz de `crate::` de ningún archivo
/// (los archivos reales viven bajo los `Cargo.toml` de cada miembro).
pub fn cargo_toml_has_package(content: &str) -> bool {
    let Ok(value) = toml::from_str::<toml::Value>(content) else {
        return false;
    };
    value.get("package").is_some()
}

fn parse_cargo_toml(content: &str) -> Vec<LibraryDep> {
    let Ok(value) = toml::from_str::<toml::Value>(content) else {
        return Vec::new();
    };
    let mut deps = Vec::new();
    for table_name in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(table) = value.get(table_name).and_then(|v| v.as_table()) else {
            continue;
        };
        for (name, spec) in table {
            let version = match spec {
                toml::Value::String(s) => Some(s.clone()),
                toml::Value::Table(t) => t.get("version").and_then(|v| v.as_str()).map(String::from),
                _ => None,
            };
            deps.push(LibraryDep { name: name.clone(), version });
        }
    }
    deps
}

fn parse_package_json(content: &str) -> Vec<LibraryDep> {
    let Ok(value) = serde_json::from_str::<JsonValue>(content) else {
        return Vec::new();
    };
    let mut deps = Vec::new();
    for field in ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"] {
        let Some(obj) = value.get(field).and_then(|v| v.as_object()) else {
            continue;
        };
        for (name, version) in obj {
            deps.push(LibraryDep {
                name: name.clone(),
                version: version.as_str().map(String::from),
            });
        }
    }
    deps
}

fn parse_requirements_txt(content: &str) -> Vec<LibraryDep> {
    let mut deps = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with('-') {
            continue;
        }
        let cut = line.find(|c: char| "=<>!~;[".contains(c)).unwrap_or(line.len());
        let name = line[..cut].trim();
        if name.is_empty() {
            continue;
        }
        let version = if cut < line.len() { Some(line[cut..].trim().to_string()) } else { None };
        deps.push(LibraryDep { name: name.to_string(), version });
    }
    deps
}

fn parse_pyproject_toml(content: &str) -> Vec<LibraryDep> {
    let Ok(value) = toml::from_str::<toml::Value>(content) else {
        return Vec::new();
    };
    let mut deps = Vec::new();

    // PEP 621: [project] dependencies = ["requests>=2.0", ...]
    if let Some(list) = value
        .get("project")
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_array())
    {
        for item in list {
            let Some(spec) = item.as_str() else { continue };
            let cut = spec.find(|c: char| "=<>!~;[ ".contains(c)).unwrap_or(spec.len());
            let name = spec[..cut].trim();
            if name.is_empty() {
                continue;
            }
            let version = if cut < spec.len() { Some(spec[cut..].trim().to_string()) } else { None };
            deps.push(LibraryDep { name: name.to_string(), version });
        }
    }

    // Poetry: [tool.poetry.dependencies] nombre = "version" (o tabla con version=)
    if let Some(table) = value
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        for (name, spec) in table {
            if name == "python" {
                continue;
            }
            let version = match spec {
                toml::Value::String(s) => Some(s.clone()),
                toml::Value::Table(t) => t.get("version").and_then(|v| v.as_str()).map(String::from),
                _ => None,
            };
            deps.push(LibraryDep { name: name.clone(), version });
        }
    }

    deps
}

fn parse_go_mod(content: &str) -> Vec<LibraryDep> {
    let mut deps = Vec::new();
    let mut in_require_block = false;
    for raw_line in content.lines() {
        let line = raw_line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("require (") {
            in_require_block = true;
            continue;
        }
        if in_require_block && line == ")" {
            in_require_block = false;
            continue;
        }
        let entry = if in_require_block {
            Some(line)
        } else {
            line.strip_prefix("require ").map(str::trim)
        };
        let Some(entry) = entry else { continue };
        let mut parts = entry.split_whitespace();
        let Some(module) = parts.next() else { continue };
        let version = parts.next().map(str::to_string);
        deps.push(LibraryDep { name: module.to_string(), version });
    }
    deps
}

/// La línea `module ...` de `go.mod` — el prefijo de import que identifica
/// paquetes propios del proyecto (cualquier import que empiece con esto es
/// interno, el resto es una dependencia externa).
pub fn parse_go_module_name(content: &str) -> Option<String> {
    for raw_line in content.lines() {
        let line = raw_line.split("//").next().unwrap_or("").trim();
        if let Some(rest) = line.strip_prefix("module ") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

/// Heurístico, no un parser de Elixir: busca tuplas `{:nombre, "versión"}`
/// en todo el archivo (el patrón habitual de declarar deps en `mix.exs`),
/// descartando átomos que claramente son config y no paquetes.
const MIX_NON_PACKAGE_ATOMS: &[&str] = &["mod", "extra_applications", "applications", "included_applications", "env", "licenses", "links", "package"];

fn parse_mix_exs(content: &str) -> Vec<LibraryDep> {
    let mut deps = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' && i + 1 < chars.len() && chars[i + 1] == ':' {
            let name_start = i + 2;
            let mut j = name_start;
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            if j > name_start {
                let name: String = chars[name_start..j].iter().collect();
                let mut k = j;
                let mut version = None;
                while k < chars.len() && chars[k] != '}' && chars[k] != '{' {
                    if chars[k] == '"' {
                        let vstart = k + 1;
                        let mut vend = vstart;
                        while vend < chars.len() && chars[vend] != '"' {
                            vend += 1;
                        }
                        version = Some(chars[vstart..vend].iter().collect());
                        break;
                    }
                    k += 1;
                }
                if version.is_some() && !MIX_NON_PACKAGE_ATOMS.contains(&name.as_str()) {
                    deps.push(LibraryDep { name, version });
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    deps
}

fn parse_pom_xml(content: &str) -> Vec<LibraryDep> {
    let mut deps = Vec::new();
    for block in content.split("<dependency>").skip(1) {
        let block = block.split("</dependency>").next().unwrap_or("");
        let group = extract_xml_tag(block, "groupId");
        let artifact = extract_xml_tag(block, "artifactId");
        let version = extract_xml_tag(block, "version");
        let Some(artifact) = artifact else { continue };
        let name = match group {
            Some(g) => format!("{g}:{artifact}"),
            None => artifact,
        };
        deps.push(LibraryDep { name, version });
    }
    deps
}

fn extract_xml_tag(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&close)? + start;
    Some(block[start..end].trim().to_string())
}

/// Heurístico, no un parser de Gradle: reconoce declaraciones típicas como
/// `implementation("group:artifact:version")` (Kotlin DSL) o
/// `implementation 'group:artifact:version'` (Groovy).
const GRADLE_DEP_KEYWORDS: &[&str] = &[
    "implementation",
    "api",
    "compileOnly",
    "runtimeOnly",
    "testImplementation",
    "testRuntimeOnly",
    "annotationProcessor",
    "classpath",
];

fn parse_gradle(content: &str) -> Vec<LibraryDep> {
    let mut deps = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if !GRADLE_DEP_KEYWORDS.iter().any(|k| line.starts_with(k)) {
            continue;
        }
        for quote in ['"', '\''] {
            let Some(start) = line.find(quote) else { continue };
            let Some(len) = line[start + 1..].find(quote) else { continue };
            let coord = &line[start + 1..start + 1 + len];
            let parts: Vec<&str> = coord.split(':').collect();
            if parts.len() >= 2 {
                let name = format!("{}:{}", parts[0], parts[1]);
                let version = parts.get(2).map(|s| s.to_string());
                deps.push(LibraryDep { name, version });
            }
            break;
        }
    }
    deps
}

const R_DEP_FIELDS: &[&str] = &["Imports", "Depends", "Suggests", "LinkingTo"];

fn parse_r_description(content: &str) -> Vec<LibraryDep> {
    let mut deps = Vec::new();
    let mut current_field: Option<String> = None;
    let mut buffer = String::new();

    let flush = |field: &Option<String>, buffer: &str, deps: &mut Vec<LibraryDep>| {
        let Some(field) = field else { return };
        if !R_DEP_FIELDS.contains(&field.as_str()) {
            return;
        }
        for entry in buffer.split(',') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            let name_end = entry.find('(').unwrap_or(entry.len());
            let name = entry[..name_end].trim();
            if name.is_empty() || name.eq_ignore_ascii_case("R") {
                continue;
            }
            let version = if name_end < entry.len() {
                Some(entry[name_end..].trim_matches(|c| c == '(' || c == ')').trim().to_string())
            } else {
                None
            };
            deps.push(LibraryDep { name: name.to_string(), version });
        }
    };

    for line in content.lines() {
        if line.starts_with(char::is_whitespace) && current_field.is_some() {
            buffer.push(' ');
            buffer.push_str(line.trim());
        } else if let Some((field, rest)) = line.split_once(':') {
            flush(&current_field, &buffer, &mut deps);
            current_field = Some(field.trim().to_string());
            buffer = rest.trim().to_string();
        }
    }
    flush(&current_field, &buffer, &mut deps);

    deps
}

fn parse_composer_json(content: &str) -> Vec<LibraryDep> {
    let Ok(value) = serde_json::from_str::<JsonValue>(content) else {
        return Vec::new();
    };
    let mut deps = Vec::new();
    for field in ["require", "require-dev"] {
        let Some(obj) = value.get(field).and_then(|v| v.as_object()) else {
            continue;
        };
        for (name, version) in obj {
            if name == "php" || name.starts_with("ext-") {
                continue; // no son paquetes de Composer, son requisitos de runtime
            }
            deps.push(LibraryDep {
                name: name.clone(),
                version: version.as_str().map(String::from),
            });
        }
    }
    deps
}

/// Mapeo PSR-4 de `composer.json` (`autoload.psr-4` y `autoload-dev.psr-4`):
/// prefijo de namespace -> carpeta relativa al manifiesto. Se usa para
/// resolver imports *absolutos* de PHP (`use App\Models\User;`) contra
/// archivos propios del proyecto, no para nodos `library` — por eso vive
/// aparte de `parse_manifest`/`LibraryDep`.
pub fn parse_composer_psr4(content: &str) -> Vec<(String, String)> {
    let Ok(value) = serde_json::from_str::<JsonValue>(content) else {
        return Vec::new();
    };
    let mut mappings = Vec::new();
    for field in ["autoload", "autoload-dev"] {
        let Some(psr4) = value.get(field).and_then(|a| a.get("psr-4")).and_then(|p| p.as_object()) else {
            continue;
        };
        for (prefix, dir) in psr4 {
            let Some(dir) = dir.as_str() else { continue };
            let prefix = prefix.trim_end_matches('\\').to_string();
            let dir = dir.trim_end_matches('/').to_string();
            mappings.push((prefix, dir));
        }
    }
    mappings
}

/// Quita comentarios `//` y `/* */` de un JSON, respetando strings entre
/// comillas dobles (`tsconfig.json` reales casi siempre son JSONC, y
/// `serde_json` rechaza comentarios). No hay comillas simples ni backticks
/// en JSON, así que el enmascarado es más simple que el de código fuente.
fn strip_jsonc_comments(content: &str) -> String {
    let chars: Vec<char> = content.chars().collect();
    let mut result = String::with_capacity(chars.len());
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            result.push(c);
            if c == '\\' && i + 1 < chars.len() {
                result.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            result.push(c);
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        result.push(c);
        i += 1;
    }
    result
}

/// Contexto de resolución de alias de `tsconfig.json` ya combinado con el
/// directorio del propio `tsconfig.json` — `base_dir` es `baseUrl` relativo
/// a la raíz del proyecto (no relativo al tsconfig), y cada patrón de
/// `paths` sigue siendo relativo a `base_dir`. Sin soporte de `extends`
/// (limitación aceptada, igual criterio que el resto del extractor).
pub struct TsPathConfig {
    pub base_dir: String,
    pub paths: Vec<(String, Vec<String>)>,
}

/// `tsconfig_dir` es el directorio del propio `tsconfig.json` relativo a la
/// raíz del proyecto (`"."` si está en la raíz). Devuelve `None` si no hay
/// `compilerOptions.paths` declarado (nada que resolver) o si el JSON (tras
/// quitar comentarios) no parsea.
pub fn parse_tsconfig_paths(content: &str, tsconfig_dir: &str) -> Option<TsPathConfig> {
    let stripped = strip_jsonc_comments(content);
    let value: JsonValue = serde_json::from_str(&stripped).ok()?;
    let compiler_options = value.get("compilerOptions")?;
    let paths_obj = compiler_options.get("paths")?.as_object()?;
    if paths_obj.is_empty() {
        return None;
    }

    let base_url = compiler_options.get("baseUrl").and_then(|v| v.as_str()).unwrap_or(".");
    let base_dir = crate::extractor::imports::normalize_path(tsconfig_dir, base_url);

    let mut paths = Vec::new();
    for (pattern, targets) in paths_obj {
        let Some(targets_arr) = targets.as_array() else { continue };
        let targets: Vec<String> = targets_arr.iter().filter_map(|t| t.as_str().map(String::from)).collect();
        if !targets.is_empty() {
            paths.push((pattern.clone(), targets));
        }
    }
    if paths.is_empty() {
        return None;
    }
    Some(TsPathConfig { base_dir, paths })
}

fn parse_project_toml(content: &str) -> Vec<LibraryDep> {
    let Ok(value) = toml::from_str::<toml::Value>(content) else {
        return Vec::new();
    };
    // [deps] en Project.toml mapea nombre -> UUID (no versión); la versión,
    // si está fijada, vive aparte en [compat] como nombre -> rango.
    let Some(deps_table) = value.get("deps").and_then(|d| d.as_table()) else {
        return Vec::new();
    };
    let compat_table = value.get("compat").and_then(|c| c.as_table());

    deps_table
        .keys()
        .map(|name| {
            let version = compat_table
                .and_then(|c| c.get(name))
                .and_then(|v| v.as_str())
                .map(String::from);
            LibraryDep { name: name.clone(), version }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(deps: &[LibraryDep]) -> Vec<&str> {
        deps.iter().map(|d| d.name.as_str()).collect()
    }

    #[test]
    fn parses_cargo_toml() {
        let content = r#"
[package]
name = "demo"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
anyhow = "1.0"

[dev-dependencies]
tempfile = "3"
"#;
        let deps = parse_manifest("Cargo.toml", content);
        // las tablas de `dependencies`/`dev-dependencies` no preservan el
        // orden de declaración (sin la feature `preserve_order`), así que
        // comparamos como conjunto, no como secuencia.
        let mut found = names(&deps);
        found.sort_unstable();
        assert_eq!(found, vec!["anyhow", "serde", "tempfile"]);

        let serde_dep = deps.iter().find(|d| d.name == "serde").unwrap();
        assert_eq!(serde_dep.version.as_deref(), Some("1.0"));
        let anyhow_dep = deps.iter().find(|d| d.name == "anyhow").unwrap();
        assert_eq!(anyhow_dep.version.as_deref(), Some("1.0"));
    }

    #[test]
    fn parses_package_json() {
        let content = r#"{
            "dependencies": { "react": "^18.0.0" },
            "devDependencies": { "vitest": "^1.0.0" }
        }"#;
        let deps = parse_manifest("package.json", content);
        assert_eq!(names(&deps), vec!["react", "vitest"]);
        assert_eq!(deps[0].version.as_deref(), Some("^18.0.0"));
    }

    #[test]
    fn parses_requirements_txt() {
        let content = "# comment\nrequests==2.31.0\nflask>=2.0\n-r other.txt\nnumpy\n";
        let deps = parse_manifest("requirements.txt", content);
        assert_eq!(names(&deps), vec!["requests", "flask", "numpy"]);
        assert_eq!(deps[0].version.as_deref(), Some("==2.31.0"));
        assert_eq!(deps[2].version, None);
    }

    #[test]
    fn parses_pyproject_toml_pep621_and_poetry() {
        let pep621 = r#"
[project]
dependencies = ["requests>=2.0", "click"]
"#;
        assert_eq!(names(&parse_manifest("pyproject.toml", pep621)), vec!["requests", "click"]);

        let poetry = r#"
[tool.poetry.dependencies]
python = "^3.11"
fastapi = "^0.100"
"#;
        assert_eq!(names(&parse_manifest("pyproject.toml", poetry)), vec!["fastapi"]);
    }

    #[test]
    fn parses_go_mod() {
        let content = "module example.com/foo\n\ngo 1.21\n\nrequire (\n\tgithub.com/a/b v1.2.3\n\tgithub.com/c/d v4.5.6\n)\n\nrequire github.com/e/f v0.1.0\n";
        let deps = parse_manifest("go.mod", content);
        assert_eq!(names(&deps), vec!["github.com/a/b", "github.com/c/d", "github.com/e/f"]);
        assert_eq!(deps[0].version.as_deref(), Some("v1.2.3"));
    }

    #[test]
    fn parses_go_module_name() {
        let content = "module example.com/foo\n\ngo 1.21\n";
        assert_eq!(parse_go_module_name(content).as_deref(), Some("example.com/foo"));
    }

    #[test]
    fn parses_mix_exs_deps() {
        let content = r#"
defp deps do
  [
    {:phoenix, "~> 1.7"},
    {:ecto, "~> 3.10"},
    {:mod, {MyApp, []}}
  ]
end
"#;
        let deps = parse_manifest("mix.exs", content);
        assert_eq!(names(&deps), vec!["phoenix", "ecto"]);
    }

    #[test]
    fn parses_pom_xml() {
        let content = r#"
<project>
  <dependencies>
    <dependency>
      <groupId>org.springframework</groupId>
      <artifactId>spring-core</artifactId>
      <version>5.3.0</version>
    </dependency>
  </dependencies>
</project>
"#;
        let deps = parse_manifest("pom.xml", content);
        assert_eq!(names(&deps), vec!["org.springframework:spring-core"]);
        assert_eq!(deps[0].version.as_deref(), Some("5.3.0"));
    }

    #[test]
    fn parses_gradle_kts() {
        let content = r#"
dependencies {
    implementation("org.jetbrains.kotlin:kotlin-stdlib:1.9.0")
    testImplementation("junit:junit:4.13.2")
}
"#;
        let deps = parse_manifest("build.gradle.kts", content);
        assert_eq!(names(&deps), vec!["org.jetbrains.kotlin:kotlin-stdlib", "junit:junit"]);
    }

    #[test]
    fn parses_r_description() {
        let content = "Package: demo\nImports:\n    dplyr (>= 1.0.0),\n    ggplot2,\n    R (>= 4.0)\nSuggests: testthat\n";
        let deps = parse_manifest("DESCRIPTION", content);
        assert_eq!(names(&deps), vec!["dplyr", "ggplot2", "testthat"]);
        assert_eq!(deps[0].version.as_deref(), Some(">= 1.0.0"));
    }

    #[test]
    fn cargo_toml_with_package_table_is_a_crate() {
        assert!(cargo_toml_has_package("[package]\nname = \"demo\"\n"));
    }

    #[test]
    fn cargo_toml_workspace_only_is_not_a_crate() {
        assert!(!cargo_toml_has_package("[workspace]\nmembers = [\"crates/*\"]\n"));
    }

    #[test]
    fn parses_tsconfig_paths_with_base_url_and_jsonc_comments() {
        let content = r#"{
            // comentario de línea
            "compilerOptions": {
                "baseUrl": ".",
                /* comentario de bloque */
                "paths": {
                    "@app/*": ["src/app/*"],
                    "@utils": ["src/utils/index.ts"]
                }
            }
        }"#;
        let config = parse_tsconfig_paths(content, ".").expect("debería parsear paths");
        assert_eq!(config.base_dir, ".");
        let mut paths = config.paths;
        paths.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(paths[0].0, "@app/*");
        assert_eq!(paths[0].1, vec!["src/app/*".to_string()]);
        assert_eq!(paths[1].0, "@utils");
    }

    #[test]
    fn parses_tsconfig_paths_combines_base_url_with_tsconfig_dir() {
        let content = r#"{"compilerOptions": {"baseUrl": "src", "paths": {"@app/*": ["app/*"]}}}"#;
        let config = parse_tsconfig_paths(content, "packages/web").expect("debería parsear paths");
        assert_eq!(config.base_dir, "packages/web/src");
    }

    #[test]
    fn tsconfig_without_paths_yields_none() {
        let content = r#"{"compilerOptions": {"target": "es2020"}}"#;
        assert!(parse_tsconfig_paths(content, ".").is_none());
    }

    #[test]
    fn unknown_filename_yields_no_deps() {
        assert!(parse_manifest("random.txt", "anything").is_empty());
        assert!(!is_manifest_file("random.txt"));
        assert!(is_manifest_file("Cargo.toml"));
    }

    #[test]
    fn parses_composer_json_deps() {
        let content = r#"{
            "require": { "php": ">=8.1", "guzzlehttp/guzzle": "^7.0" },
            "require-dev": { "phpunit/phpunit": "^10.0" }
        }"#;
        let deps = parse_manifest("composer.json", content);
        assert_eq!(names(&deps), vec!["guzzlehttp/guzzle", "phpunit/phpunit"]);
    }

    #[test]
    fn parses_composer_psr4_autoload() {
        let content = r#"{
            "autoload": { "psr-4": { "App\\": "src/" } },
            "autoload-dev": { "psr-4": { "Tests\\": "tests/" } }
        }"#;
        let mut mappings = parse_composer_psr4(content);
        mappings.sort();
        assert_eq!(mappings, vec![("App".to_string(), "src".to_string()), ("Tests".to_string(), "tests".to_string())]);
    }

    #[test]
    fn parses_project_toml_deps_with_compat_version() {
        let content = r#"
name = "Demo"

[deps]
DataFrames = "a93c6f00-e57d-5684-b7b6-d8193f3e46c0"
JSON = "682c06a0-de6a-54ab-a142-c8b1cf79cde6"

[compat]
DataFrames = "1.6"
"#;
        let deps = parse_manifest("Project.toml", content);
        let mut found = names(&deps);
        found.sort_unstable();
        assert_eq!(found, vec!["DataFrames", "JSON"]);
        let dataframes = deps.iter().find(|d| d.name == "DataFrames").unwrap();
        assert_eq!(dataframes.version.as_deref(), Some("1.6"));
        let json_dep = deps.iter().find(|d| d.name == "JSON").unwrap();
        assert_eq!(json_dep.version, None);
    }
}
