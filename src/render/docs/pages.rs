use crate::config::DocPageConfig;
use crate::schema::Graph;
use anyhow::{bail, Result};

/// Una página de documentación resuelta: su nodo y la URL a la que
/// redirige "Ir a documentación" — ver `build_doc_pages`.
#[derive(Debug)]
pub struct DocPage {
    pub node_id: String,
    pub url: String,
}

/// Resuelve cada `[[docs.pages]]` contra el grafo. Es un mapeo explícito
/// (no hay auto-descubrimiento): una entrada con `node` que no matchea
/// ningún nodo del grafo es un error — si el usuario lo escribió mal,
/// mejor fallar fuerte al generar que servir una estrella sin destino.
/// Ariadne no lee ni renderiza nada de la `url` en sí — es responsabilidad
/// del usuario que exista y tenga contenido.
pub fn build_doc_pages(graph: &Graph, pages_cfg: &[DocPageConfig]) -> Result<Vec<DocPage>> {
    let mut pages = Vec::with_capacity(pages_cfg.len());
    for entry in pages_cfg {
        if !graph.nodes.iter().any(|n| n.id == entry.node) {
            bail!("[[docs.pages]] referencia un nodo que no existe en el grafo: \"{}\"", entry.node);
        }
        pages.push(DocPage {
            node_id: entry.node.clone(),
            url: entry.url.clone(),
        });
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{GraphNode, NodeMetadata, NodeType};

    fn sample_graph() -> Graph {
        Graph {
            schema_version: "1.0.0".to_string(),
            root: ".".to_string(),
            generated_at: "2026-01-01T00:00:00+00:00".to_string(),
            source_path: ".".to_string(),
            nodes: vec![GraphNode {
                id: "src/auth.py".to_string(),
                node_type: NodeType::File,
                label: "auth.py".to_string(),
                path: "src/auth.py".to_string(),
                parent_id: Some("src".to_string()),
                depth: 2,
                metadata: NodeMetadata::default(),
            }],
            edges: Vec::new(),
        }
    }

    #[test]
    fn resolves_known_node_with_its_url() {
        let cfg = vec![DocPageConfig {
            node: "src/auth.py".to_string(),
            url: "docs/auth.html".to_string(),
        }];
        let pages = build_doc_pages(&sample_graph(), &cfg).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].node_id, "src/auth.py");
        assert_eq!(pages[0].url, "docs/auth.html");
    }

    #[test]
    fn errors_on_unknown_node() {
        let cfg = vec![DocPageConfig {
            node: "does/not/exist.py".to_string(),
            url: "docs/x.html".to_string(),
        }];
        let err = build_doc_pages(&sample_graph(), &cfg).unwrap_err();
        assert!(err.to_string().contains("does/not/exist.py"));
    }

    #[test]
    fn empty_config_yields_empty_pages() {
        assert!(build_doc_pages(&sample_graph(), &[]).unwrap().is_empty());
    }
}
