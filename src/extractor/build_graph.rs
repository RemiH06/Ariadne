use crate::config::IgnoreConfig;
use crate::extractor::classes::extract_classes;
use crate::extractor::classify::classify;
use crate::extractor::git_blame;
use crate::extractor::imports::{
    extract_imports, resolve_elixir_absolute, resolve_go_package, resolve_haskell_absolute, resolve_java_kotlin_absolute,
    resolve_php_absolute, resolve_python_absolute, resolve_relative_import, resolve_rust, resolve_ts_alias, top_level_package_name,
};
use crate::extractor::manifests::{
    cargo_toml_has_package, is_manifest_file, manifest_import_language, parse_composer_psr4, parse_go_module_name, parse_manifest,
    parse_tsconfig_paths, TsPathConfig,
};
use crate::extractor::walk::walk;
use crate::schema::{CommitInfo, EdgeType, Graph, GraphEdge, GraphNode, NodeMetadata, NodeType, SCHEMA_VERSION};
use anyhow::Result;
use chrono::Utc;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub fn build_graph(root: &Path, project_name: &str, ignore_cfg: &IgnoreConfig) -> Result<Graph> {
    let entries = walk(root, ignore_cfg)?;

    // Fase paralela: clasificar cada entrada (extensión, lenguaje, heurística
    // de generado, tamaño) de forma independiente entre sí.
    let mut nodes: Vec<GraphNode> = entries
        .par_iter()
        .map(|entry| {
            let c = classify(&entry.abs_path, &entry.rel_path, entry.is_dir);
            let depth = entry.rel_path.split('/').count() as u32;
            let label = entry
                .abs_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| entry.rel_path.clone());

            GraphNode {
                id: entry.rel_path.clone(),
                node_type: c.node_type,
                label,
                path: entry.rel_path.clone(),
                parent_id: Some(parent_of(&entry.rel_path)),
                depth,
                metadata: NodeMetadata {
                    extension: c.extension,
                    size_bytes: c.size_bytes,
                    line_count: c.line_count,
                    child_count: None,
                    is_generated: Some(c.is_generated),
                    icon_key: c.icon_key,
                    language: c.language,
                    category: c.category.map(str::to_string),
                    shape: c.shape.map(str::to_string),
                    last_author: None,
                    last_modified: None,
                    recent_commits: None,
                    extra: Default::default(),
                },
            }
        })
        .collect();

    let root_label = if project_name.is_empty() {
        root.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string())
    } else {
        project_name.to_string()
    };

    nodes.insert(
        0,
        GraphNode {
            id: ".".to_string(),
            node_type: NodeType::Root,
            label: root_label,
            path: ".".to_string(),
            parent_id: None,
            depth: 0,
            metadata: NodeMetadata::default(),
        },
    );

    // Manifiestos de dependencias (Cargo.toml, package.json, etc.): cada
    // dependencia declarada se agrega como un nodo `library`, hijo del
    // archivo manifiesto — así el árbol muestra de dónde sale cada una.
    // También se registra en `manifest_registry` (nombre declarado -> id del
    // nodo library) para poder conectar imports de paquetes externos más
    // abajo, sin volver a parsear el manifiesto.
    let mut manifest_registry: Vec<(String, &'static str, HashMap<String, String>)> = Vec::new();

    for entry in &entries {
        let filename = entry.abs_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if entry.is_dir || !is_manifest_file(filename) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&entry.abs_path) else {
            continue;
        };
        let manifest_depth = entry.rel_path.split('/').count() as u32;
        let lang_group = manifest_import_language(filename);
        let mut deps_by_name: HashMap<String, String> = HashMap::new();

        for dep in parse_manifest(filename, &content) {
            let lib_id = format!("{}::lib::{}", entry.rel_path, dep.name);
            if lang_group.is_some() {
                deps_by_name.insert(dep.name.clone(), lib_id.clone());
            }
            let mut extra = serde_json::Map::new();
            if let Some(version) = &dep.version {
                extra.insert("version".to_string(), serde_json::Value::String(version.clone()));
            }
            nodes.push(GraphNode {
                id: lib_id.clone(),
                node_type: NodeType::Library,
                label: dep.name,
                path: lib_id,
                parent_id: Some(entry.rel_path.clone()),
                depth: manifest_depth + 1,
                metadata: NodeMetadata {
                    extra,
                    ..Default::default()
                },
            });
        }

        if let Some(lang_group) = lang_group {
            manifest_registry.push((parent_of(&entry.rel_path), lang_group, deps_by_name));
        }
    }

    // Ruta absoluta por id — se usa acá y más abajo para leer el contenido
    // de cada archivo (clases y luego referencias entre archivos).
    let abs_path_by_id: HashMap<&str, &Path> = entries.iter().map(|e| (e.rel_path.as_str(), e.abs_path.as_path())).collect();

    // Clases/métodos/atributos: heurística de texto por lenguaje (ver
    // extractor::classes), igual que las referencias entre archivos — no un
    // parser real. Se insertan ANTES de calcular `child_count`/aristas
    // `Contains` de abajo, así ese mecanismo genérico ya existente las
    // recoge solo (un nodo Method/Attribute es "hijo" de su Class exactamente
    // igual que un archivo es "hijo" de su carpeta).
    let mut class_nodes: Vec<GraphNode> = Vec::new();
    for node in nodes.iter().filter(|n| n.node_type == NodeType::File) {
        let Some(lang) = node.metadata.language.as_deref() else {
            continue;
        };
        let Some(&abs_path) = abs_path_by_id.get(node.id.as_str()) else {
            continue;
        };
        let Ok(content) = std::fs::read_to_string(abs_path) else {
            continue;
        };

        for class in extract_classes(lang, &content) {
            let class_id = format!("{}::class::{}", node.id, class.name);
            class_nodes.push(GraphNode {
                id: class_id.clone(),
                node_type: NodeType::Class,
                label: class.name,
                path: class_id.clone(),
                parent_id: Some(node.id.clone()),
                depth: node.depth + 1,
                metadata: NodeMetadata::default(),
            });
            for method in class.methods {
                let method_id = format!("{class_id}::method::{method}");
                class_nodes.push(GraphNode {
                    id: method_id.clone(),
                    node_type: NodeType::Method,
                    label: method,
                    path: method_id,
                    parent_id: Some(class_id.clone()),
                    depth: node.depth + 2,
                    metadata: NodeMetadata::default(),
                });
            }
            for attribute in class.attributes {
                let attr_id = format!("{class_id}::attr::{attribute}");
                class_nodes.push(GraphNode {
                    id: attr_id.clone(),
                    node_type: NodeType::Attribute,
                    label: attribute,
                    path: attr_id,
                    parent_id: Some(class_id.clone()),
                    depth: node.depth + 2,
                    metadata: NodeMetadata::default(),
                });
            }
        }
    }
    nodes.extend(class_nodes);

    // Historial corto de commits por archivo (heurística vía `git log`, ver
    // extractor::git_blame) — best-effort, no falla el resto de la
    // generación si la carpeta no es un repo git. `MAX_RECENT_COMMITS`
    // alimenta tanto "ver historial" (todas las entradas) como el
    // autor/fecha del último commit (la primera entrada de cada lista).
    // Carpetas/raíz heredan el máximo (más reciente) entre sus hijos
    // directos, calculado de abajo hacia arriba a partir de los archivos
    // ya poblados.
    const MAX_RECENT_COMMITS: usize = 5;
    let file_rel_paths: HashSet<&str> = entries.iter().filter(|e| !e.is_dir).map(|e| e.rel_path.as_str()).collect();
    let history_by_path = git_blame::collect_commit_history_by_path(root, &file_rel_paths, MAX_RECENT_COMMITS);

    let mut last_modified_ts: HashMap<String, i64> = HashMap::new();
    let mut last_author_by_id: HashMap<String, String> = HashMap::new();
    for node in nodes.iter_mut().filter(|n| n.node_type == NodeType::File) {
        let Some(history) = history_by_path.get(node.id.as_str()) else { continue };
        let Some(latest) = history.first() else { continue };
        last_modified_ts.insert(node.id.clone(), latest.timestamp);
        last_author_by_id.insert(node.id.clone(), latest.author.clone());
        node.metadata.recent_commits = Some(
            history
                .iter()
                .map(|c| CommitInfo {
                    short_hash: c.short_hash.clone(),
                    author: c.author.clone(),
                    timestamp: format_git_timestamp(c.timestamp),
                    subject: c.subject.clone(),
                })
                .collect(),
        );
    }

    let mut children_by_parent: HashMap<String, Vec<String>> = HashMap::new();
    for node in &nodes {
        if let Some(parent) = &node.parent_id {
            children_by_parent.entry(parent.clone()).or_default().push(node.id.clone());
        }
    }

    let mut dir_ids_by_depth: Vec<(String, u32)> = nodes
        .iter()
        .filter(|n| matches!(n.node_type, NodeType::Directory | NodeType::Root))
        .map(|n| (n.id.clone(), n.depth))
        .collect();
    dir_ids_by_depth.sort_by_key(|(_, depth)| std::cmp::Reverse(*depth));

    for (dir_id, _) in &dir_ids_by_depth {
        let Some(children) = children_by_parent.get(dir_id) else { continue };
        let best = children
            .iter()
            .filter_map(|child_id| last_modified_ts.get(child_id).map(|&ts| (ts, child_id.clone())))
            .max_by_key(|(ts, _)| *ts);
        let Some((ts, child_id)) = best else { continue };
        last_modified_ts.insert(dir_id.clone(), ts);
        if let Some(author) = last_author_by_id.get(&child_id).cloned() {
            last_author_by_id.insert(dir_id.clone(), author);
        }
    }

    for node in nodes.iter_mut() {
        if let Some(&ts) = last_modified_ts.get(node.id.as_str()) {
            node.metadata.last_modified = Some(format_git_timestamp(ts));
        }
        if let Some(author) = last_author_by_id.get(node.id.as_str()) {
            node.metadata.last_author = Some(author.clone());
        }
    }

    // `child_count` se puebla para CUALQUIER nodo con hijos, no solo
    // carpetas/raíz — un archivo con clases o una clase con métodos/
    // atributos también tiene hijos, y el cliente lo necesita ahí: es el
    // respaldo que usa `hasChildren()` para saber si un nodo colapsado
    // (sin `children` en el árbol filtrado) sigue teniendo algo que
    // volver a expandir. Sin esto, un click para volver a expandir un nodo
    // de este tipo se malinterpretaba como "es una hoja" y disparaba focus
    // en vez de expandir.
    let mut counts: HashMap<String, u32> = HashMap::new();
    for node in &nodes {
        if let Some(parent) = &node.parent_id {
            *counts.entry(parent.clone()).or_insert(0) += 1;
        }
    }
    for node in &mut nodes {
        node.metadata.child_count = counts.get(&node.id).copied();
    }

    let mut edges: Vec<GraphEdge> = nodes
        .iter()
        .filter_map(|n| {
            n.parent_id.as_ref().map(|parent| GraphEdge {
                id: format!("{parent}->{}", n.id),
                edge_type: EdgeType::Contains,
                source: parent.clone(),
                target: n.id.clone(),
            })
        })
        .collect();

    // Referencias entre archivos (imports): heurística de texto (ver
    // extractor::imports). Un import relativo (`./foo`, `.bar`) se resuelve
    // contra otro archivo del proyecto; uno absoluto de Python se cruza
    // contra los paquetes propios del proyecto (carpetas con `__init__.py`);
    // lo que quede sin resolver se cruza contra las dependencias del
    // manifiesto más cercano para conectar con el nodo `library`
    // correspondiente. Alias de resolución de JS/TS (`tsconfig.json` paths)
    // y imports absolutos de otros lenguajes (`crate::foo`) quedan fuera de
    // alcance por ahora.
    let known_ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    // Paquetes propios de Python: cualquier carpeta con `__init__.py`, con
    // su nombre de carpeta como nombre de paquete (misma convención que usa
    // el propio Python/pip).
    let python_package_roots: HashMap<String, String> = nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Directory)
        .filter(|n| known_ids.contains(format!("{}/__init__.py", n.id).as_str()))
        .map(|n| (n.label.clone(), n.id.clone()))
        .collect();

    // Contexto por lenguaje para resolver imports/referencias *absolutas* a
    // módulos propios del proyecto (no relativas, no de una librería
    // externa). Rust/Go/PHP se registran por CADA manifiesto encontrado en
    // el árbol (no solo el de la raíz) y se resuelven contra el más cercano
    // al archivo importador (`nearest_ancestor`, mismo criterio que
    // `nearest_manifest_deps` ya usa para dependencias externas) — así
    // funcionan workspaces multi-`Cargo.toml`, multi-módulo Go y
    // multi-paquete PHP en un mismo repo.
    let rust_crate_roots: Vec<(String, String)> = entries
        .iter()
        .filter(|e| e.rel_path == "Cargo.toml" || e.rel_path.ends_with("/Cargo.toml"))
        .filter_map(|e| {
            let content = std::fs::read_to_string(&e.abs_path).ok()?;
            if !cargo_toml_has_package(&content) {
                return None;
            }
            let cargo_dir = parent_of(&e.rel_path);
            let crate_root = if cargo_dir == "." { "src".to_string() } else { format!("{cargo_dir}/src") };
            Some((cargo_dir, crate_root))
        })
        .collect();
    let go_modules: Vec<(String, String)> = entries
        .iter()
        .filter(|e| e.rel_path == "go.mod" || e.rel_path.ends_with("/go.mod"))
        .filter_map(|e| {
            let content = std::fs::read_to_string(&e.abs_path).ok()?;
            let module_name = parse_go_module_name(&content)?;
            Some((parent_of(&e.rel_path), module_name))
        })
        .collect();
    let php_manifests: Vec<(String, Vec<(String, String)>)> = entries
        .iter()
        .filter(|e| e.rel_path == "composer.json" || e.rel_path.ends_with("/composer.json"))
        .filter_map(|e| {
            let content = std::fs::read_to_string(&e.abs_path).ok()?;
            let psr4 = parse_composer_psr4(&content);
            (!psr4.is_empty()).then(|| (parent_of(&e.rel_path), psr4))
        })
        .collect();
    let ts_configs: Vec<(String, TsPathConfig)> = entries
        .iter()
        .filter(|e| e.rel_path == "tsconfig.json" || e.rel_path.ends_with("/tsconfig.json"))
        .filter_map(|e| {
            let content = std::fs::read_to_string(&e.abs_path).ok()?;
            let tsconfig_dir = parent_of(&e.rel_path);
            let config = parse_tsconfig_paths(&content, &tsconfig_dir)?;
            Some((tsconfig_dir, config))
        })
        .collect();
    let java_source_roots: Vec<String> = nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Directory)
        .filter(|n| n.id.ends_with("src/main/java") || n.id.ends_with("src/test/java"))
        .map(|n| n.id.clone())
        .collect();
    let kotlin_source_roots: Vec<String> = nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Directory)
        .filter(|n| n.id.ends_with("src/main/kotlin") || n.id.ends_with("src/test/kotlin"))
        .map(|n| n.id.clone())
        .collect();
    let haskell_source_roots: Vec<String> = {
        let mut roots: Vec<String> = nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Directory)
            .filter(|n| n.label == "src")
            .map(|n| n.id.clone())
            .collect();
        roots.push(".".to_string());
        roots
    };
    let elixir_lib_roots: Vec<String> = nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Directory)
        .filter(|n| n.label == "lib")
        .map(|n| n.id.clone())
        .collect();
    let mut seen_import_edges: HashSet<(String, String)> = HashSet::new();
    const REFERENCE_LANGUAGES: &[&str] = &[
        "javascript", "typescript", "python", "rust", "go", "java", "kotlin", "php", "ruby", "c", "cplusplus", "elixir", "haskell",
        "julia", "r",
    ];

    for node in &nodes {
        let Some(lang) = node.metadata.language.as_deref() else {
            continue;
        };
        if !REFERENCE_LANGUAGES.contains(&lang) {
            continue;
        }
        let Some(&abs_path) = abs_path_by_id.get(node.id.as_str()) else {
            continue;
        };
        let Ok(content) = std::fs::read_to_string(abs_path) else {
            continue;
        };

        let lang_group = match lang {
            "javascript" | "typescript" => "javascript",
            "java" | "kotlin" => "java",
            _ => lang,
        };
        let file_dir = parent_of(&node.id);
        let nearest_deps = nearest_manifest_deps(&manifest_registry, lang_group, &file_dir);

        for import_ref in extract_imports(lang, &content) {
            let target = resolve_relative_import(&node.id, &import_ref.specifier, lang, &known_ids)
                .or_else(|| {
                    // No era relativo: probar contra convenciones propias
                    // del proyecto para módulos/paquetes absolutos, antes de
                    // asumir que se trata de una librería externa.
                    match lang {
                        "python" => resolve_python_absolute(&import_ref.specifier, &python_package_roots, &known_ids),
                        "rust" => {
                            let crate_root = nearest_ancestor(&rust_crate_roots, &file_dir).map(|(_, root)| root.as_str());
                            resolve_rust(&node.id, &import_ref.specifier, crate_root, &known_ids)
                        }
                        "go" => nearest_ancestor(&go_modules, &file_dir)
                            .and_then(|(dir, module)| resolve_go_package(&import_ref.specifier, module, dir, &known_ids)),
                        "java" => resolve_java_kotlin_absolute(&import_ref.specifier, &java_source_roots, "java", &known_ids),
                        "kotlin" => resolve_java_kotlin_absolute(&import_ref.specifier, &kotlin_source_roots, "kt", &known_ids),
                        "php" => nearest_ancestor(&php_manifests, &file_dir)
                            .and_then(|(dir, psr4)| resolve_php_absolute(&import_ref.specifier, psr4, dir, &known_ids)),
                        "elixir" => resolve_elixir_absolute(&import_ref.specifier, &elixir_lib_roots, &known_ids),
                        "haskell" => resolve_haskell_absolute(&import_ref.specifier, &haskell_source_roots, &known_ids),
                        "javascript" | "typescript" => nearest_ancestor(&ts_configs, &file_dir)
                            .and_then(|(_, config)| resolve_ts_alias(&import_ref.specifier, config, &known_ids)),
                        _ => None,
                    }
                })
                .or_else(|| {
                    // Tampoco es un módulo propio del proyecto: probar si
                    // coincide con una dependencia del manifiesto más
                    // cercano, para conectar con el nodo `library` ya
                    // detectado. Rust admite tanto guion como guion bajo en
                    // el nombre del crate (Cargo.toml vs. `use`).
                    let pkg_name = top_level_package_name(lang, &import_ref.specifier);
                    nearest_deps.and_then(|deps| {
                        deps.get(&pkg_name).cloned().or_else(|| {
                            if lang != "rust" {
                                return None;
                            }
                            let alt = if pkg_name.contains('_') { pkg_name.replace('_', "-") } else { pkg_name.replace('-', "_") };
                            deps.get(&alt).cloned()
                        })
                    })
                });

            let Some(target) = target else {
                continue;
            };
            if target == node.id || !seen_import_edges.insert((node.id.clone(), target.clone())) {
                continue;
            }
            edges.push(GraphEdge {
                id: format!("{}=>{}", node.id, target),
                edge_type: EdgeType::DependsOn,
                source: node.id.clone(),
                target,
            });
        }
    }

    let source_path = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());

    Ok(Graph {
        schema_version: SCHEMA_VERSION.to_string(),
        root: ".".to_string(),
        generated_at: Utc::now().to_rfc3339(),
        source_path,
        nodes,
        edges,
    })
}

fn parent_of(rel_path: &str) -> String {
    match rel_path.rfind('/') {
        Some(idx) => rel_path[..idx].to_string(),
        None => ".".to_string(),
    }
}

fn format_git_timestamp(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.to_rfc3339()).unwrap_or_default()
}

/// Versión genérica de "manifiesto ancestro más cercano" — el mismo
/// criterio que `nearest_manifest_deps` de abajo, sin el filtro extra de
/// `lang_group`. Se usa para workspaces multi-manifiesto (Rust/Go/PHP) y
/// para alias de `tsconfig.json`: cada archivo se resuelve contra el
/// manifiesto de su propio directorio ancestro más profundo, no contra uno
/// global fijo — así conviven varios `Cargo.toml`/`go.mod`/`composer.json`/
/// `tsconfig.json` en el mismo repo.
fn nearest_ancestor<'a, T>(entries: &'a [(String, T)], file_dir: &str) -> Option<&'a (String, T)> {
    entries.iter().filter(|(dir, _)| is_ancestor_dir(dir, file_dir)).max_by_key(|(dir, _)| dir.len())
}

/// El manifiesto más cercano (el de directorio ancestro más profundo) del
/// mismo grupo de lenguaje que gobierna a `file_dir` — igual que Node.js
/// busca el `package.json` más cercano subiendo directorios.
fn nearest_manifest_deps<'a>(
    registry: &'a [(String, &'static str, HashMap<String, String>)],
    lang_group: &str,
    file_dir: &str,
) -> Option<&'a HashMap<String, String>> {
    registry
        .iter()
        .filter(|(dir, lg, _)| *lg == lang_group && is_ancestor_dir(dir, file_dir))
        .max_by_key(|(dir, _, _)| dir.len())
        .map(|(_, _, deps)| deps)
}

fn is_ancestor_dir(ancestor: &str, dir: &str) -> bool {
    if ancestor == "." {
        return true;
    }
    dir == ancestor || dir.starts_with(&format!("{ancestor}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_ancestor_picks_deepest_matching_dir() {
        let entries = vec![
            (".".to_string(), "root".to_string()),
            ("services/api".to_string(), "api-crate".to_string()),
        ];
        let found = nearest_ancestor(&entries, "services/api/internal/utils");
        assert_eq!(found.map(|(_, v)| v.as_str()), Some("api-crate"));
    }

    #[test]
    fn nearest_ancestor_falls_back_to_root_when_no_deeper_match() {
        let entries = vec![(".".to_string(), "root".to_string()), ("packages/web".to_string(), "web".to_string())];
        let found = nearest_ancestor(&entries, "packages/cli/src");
        assert_eq!(found.map(|(_, v)| v.as_str()), Some("root"));
    }

    #[test]
    fn nearest_ancestor_none_when_no_manifest_registered() {
        let entries: Vec<(String, String)> = vec![("packages/web".to_string(), "web".to_string())];
        let found = nearest_ancestor(&entries, "packages/cli/src");
        assert!(found.is_none());
    }
}
