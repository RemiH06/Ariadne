use crate::config::IgnoreConfig;
use crate::extractor::classify::classify;
use crate::extractor::imports::{
    extract_imports, resolve_elixir_absolute, resolve_go_package, resolve_haskell_absolute, resolve_java_kotlin_absolute,
    resolve_php_absolute, resolve_python_absolute, resolve_relative_import, resolve_rust, top_level_package_name,
};
use crate::extractor::manifests::{is_manifest_file, manifest_import_language, parse_composer_psr4, parse_go_module_name, parse_manifest};
use crate::extractor::walk::walk;
use crate::schema::{EdgeType, Graph, GraphEdge, GraphNode, NodeMetadata, NodeType, SCHEMA_VERSION};
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

    let mut counts: HashMap<String, u32> = HashMap::new();
    for node in &nodes {
        if let Some(parent) = &node.parent_id {
            *counts.entry(parent.clone()).or_insert(0) += 1;
        }
    }
    for node in &mut nodes {
        if matches!(node.node_type, NodeType::Directory | NodeType::Root) {
            node.metadata.child_count = counts.get(&node.id).copied();
        }
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
    let abs_path_by_id: HashMap<&str, &Path> = entries
        .iter()
        .map(|e| (e.rel_path.as_str(), e.abs_path.as_path()))
        .collect();
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
    // externa). Cada uno es una simplificación de raíz única — sin soporte
    // de workspaces Cargo, módulos Go o paquetes PHP múltiples en un mismo
    // repo todavía.
    let rust_crate_root: Option<String> = known_ids.contains("Cargo.toml").then(|| "src".to_string());
    let go_module_name: Option<String> = entries
        .iter()
        .find(|e| e.rel_path == "go.mod")
        .and_then(|e| std::fs::read_to_string(&e.abs_path).ok())
        .and_then(|content| parse_go_module_name(&content));
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
    let (php_psr4_map, php_manifest_dir): (Vec<(String, String)>, String) = entries
        .iter()
        .find(|e| e.rel_path == "composer.json")
        .and_then(|e| std::fs::read_to_string(&e.abs_path).ok().map(|content| (parse_composer_psr4(&content), parent_of(&e.rel_path))))
        .unwrap_or_default();

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
                        "rust" => resolve_rust(&node.id, &import_ref.specifier, rust_crate_root.as_deref(), &known_ids),
                        "go" => go_module_name.as_deref().and_then(|module| resolve_go_package(&import_ref.specifier, module, &known_ids)),
                        "java" => resolve_java_kotlin_absolute(&import_ref.specifier, &java_source_roots, "java", &known_ids),
                        "kotlin" => resolve_java_kotlin_absolute(&import_ref.specifier, &kotlin_source_roots, "kt", &known_ids),
                        "php" => resolve_php_absolute(&import_ref.specifier, &php_psr4_map, &php_manifest_dir, &known_ids),
                        "elixir" => resolve_elixir_absolute(&import_ref.specifier, &elixir_lib_roots, &known_ids),
                        "haskell" => resolve_haskell_absolute(&import_ref.specifier, &haskell_source_roots, &known_ids),
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
