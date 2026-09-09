use crate::config::IgnoreConfig;
use crate::extractor::classify::classify;
use crate::extractor::imports::{extract_imports, resolve_relative_import};
use crate::extractor::manifests::{is_manifest_file, parse_manifest};
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
    for entry in &entries {
        let filename = entry.abs_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if entry.is_dir || !is_manifest_file(filename) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&entry.abs_path) else {
            continue;
        };
        let manifest_depth = entry.rel_path.split('/').count() as u32;
        for dep in parse_manifest(filename, &content) {
            let lib_id = format!("{}::lib::{}", entry.rel_path, dep.name);
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

    // Referencias entre archivos (imports): heurística de texto + resolución
    // de imports *relativos* únicamente (ver extractor::imports). Un import
    // absoluto/de paquete (`crate::foo`, `import myapp.utils`) necesitaría
    // conocer el layout de resolución de módulos del proyecto y queda fuera
    // de alcance por ahora.
    let known_ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let abs_path_by_id: HashMap<&str, &Path> = entries
        .iter()
        .map(|e| (e.rel_path.as_str(), e.abs_path.as_path()))
        .collect();
    let mut seen_import_edges: HashSet<(String, String)> = HashSet::new();

    for node in &nodes {
        let Some(lang) = node.metadata.language.as_deref() else {
            continue;
        };
        if !matches!(lang, "javascript" | "typescript" | "python") {
            continue;
        }
        let Some(&abs_path) = abs_path_by_id.get(node.id.as_str()) else {
            continue;
        };
        let Ok(content) = std::fs::read_to_string(abs_path) else {
            continue;
        };

        for import_ref in extract_imports(lang, &content) {
            let Some(target) = resolve_relative_import(&node.id, &import_ref.specifier, lang, &known_ids) else {
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
