use crate::config::DocPageConfig;
use crate::schema::Graph;
use crate::utils::slugify;
use anyhow::{bail, Context, Result};
use pulldown_cmark::{html, Parser};
use std::path::Path;

/// Una página de documentación ya resuelta: su nodo, un slug único (ancla
/// en el HTML) y el Markdown ya convertido a HTML — ver `build_doc_pages`.
#[derive(Debug)]
pub struct DocPage {
    pub node_id: String,
    pub slug: String,
    pub title: String,
    pub html: String,
}

/// Lee cada `[[docs.pages]]`, resuelve su nodo contra el grafo y su archivo
/// Markdown contra `project_root`, y devuelve las páginas listas para
/// embeber. Es un mapeo explícito (no hay auto-descubrimiento): una entrada
/// con `node` que no matchea ningún nodo del grafo, o `file` que no se
/// puede leer, es un error — si el usuario lo escribió mal, mejor fallar
/// fuerte al generar que servir un link roto en silencio.
pub fn build_doc_pages(graph: &Graph, project_root: &Path, pages_cfg: &[DocPageConfig]) -> Result<Vec<DocPage>> {
    let mut pages = Vec::with_capacity(pages_cfg.len());
    for entry in pages_cfg {
        let Some(node) = graph.nodes.iter().find(|n| n.id == entry.node) else {
            bail!("[[docs.pages]] referencia un nodo que no existe en el grafo: \"{}\"", entry.node);
        };
        let md_path = project_root.join(&entry.file);
        let content = std::fs::read_to_string(&md_path)
            .with_context(|| format!("no se pudo leer la página de documentación \"{}\" (nodo \"{}\")", md_path.display(), entry.node))?;

        pages.push(DocPage {
            node_id: node.id.clone(),
            slug: slugify(&node.id),
            title: node.label.clone(),
            html: markdown_to_html(&content),
        });
    }
    Ok(pages)
}

fn markdown_to_html(markdown: &str) -> String {
    let parser = Parser::new(markdown);
    let mut html_out = String::new();
    html::push_html(&mut html_out, parser);
    html_out
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
    fn converts_markdown_and_matches_node() {
        let dir = tempdir();
        std::fs::write(dir.join("auth.md"), "# Auth\n\nExplica el login.\n").unwrap();
        let cfg = vec![DocPageConfig {
            node: "src/auth.py".to_string(),
            file: "auth.md".to_string(),
        }];
        let pages = build_doc_pages(&sample_graph(), &dir, &cfg).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].node_id, "src/auth.py");
        assert_eq!(pages[0].title, "auth.py");
        assert_eq!(pages[0].slug, "src-auth-py");
        assert!(pages[0].html.contains("<h1>Auth</h1>"));
        assert!(pages[0].html.contains("Explica el login."));
        cleanup(&dir);
    }

    #[test]
    fn errors_on_unknown_node() {
        let dir = tempdir();
        std::fs::write(dir.join("x.md"), "x").unwrap();
        let cfg = vec![DocPageConfig {
            node: "does/not/exist.py".to_string(),
            file: "x.md".to_string(),
        }];
        let err = build_doc_pages(&sample_graph(), &dir, &cfg).unwrap_err();
        assert!(err.to_string().contains("does/not/exist.py"));
        cleanup(&dir);
    }

    #[test]
    fn errors_on_missing_file() {
        let dir = tempdir();
        let cfg = vec![DocPageConfig {
            node: "src/auth.py".to_string(),
            file: "missing.md".to_string(),
        }];
        let err = build_doc_pages(&sample_graph(), &dir, &cfg).unwrap_err();
        assert!(err.to_string().contains("missing.md"));
        cleanup(&dir);
    }

    // Sin dependencia de `tempfile`: arma un directorio único por test —
    // `cargo test` corre los tests de este módulo en threads paralelos
    // dentro del mismo proceso, así que un path basado solo en el pid
    // colisionaría entre tests.
    fn tempdir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ariadne-docs-pages-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &std::path::Path) {
        let _ = std::fs::remove_dir_all(dir);
    }
}
