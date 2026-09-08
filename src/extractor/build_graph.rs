use crate::config::IgnoreConfig;
use crate::extractor::classify::classify;
use crate::extractor::walk::walk;
use crate::schema::{EdgeType, Graph, GraphEdge, GraphNode, NodeMetadata, NodeType, SCHEMA_VERSION};
use anyhow::Result;
use chrono::Utc;
use rayon::prelude::*;
use std::collections::HashMap;
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

    let edges: Vec<GraphEdge> = nodes
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
