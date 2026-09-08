mod pandoc;

use crate::schema::{Graph, GraphNode, NodeType};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use pandoc::{check_available, PandocOptions};

/// Construye el Markdown canónico que alimenta a Pandoc para todos los
/// formatos de documento (PDF, LaTeX, RTF, XML). Es la única fuente de
/// verdad para esos renderers: cambia acá, no en cada formato.
pub fn graph_to_markdown(graph: &Graph, title: &str) -> String {
    let mut children_of: HashMap<&str, Vec<&GraphNode>> = HashMap::new();
    for n in &graph.nodes {
        if let Some(parent) = &n.parent_id {
            children_of.entry(parent.as_str()).or_default().push(n);
        }
    }
    for kids in children_of.values_mut() {
        kids.sort_by(|a, b| a.label.cmp(&b.label));
    }

    let mut out = String::new();
    out.push_str(&format!("# {title}\n\n"));
    out.push_str(&format!("_Generado por Ariadne el {}_\n\n", graph.generated_at));

    write_children(&mut out, &graph.root, &children_of, 0);

    out
}

fn write_children(
    out: &mut String,
    parent_id: &str,
    children_of: &HashMap<&str, Vec<&GraphNode>>,
    depth: usize,
) {
    let Some(kids) = children_of.get(parent_id) else {
        return;
    };

    for node in kids {
        let indent = "  ".repeat(depth);
        // Sin emoji: pdflatex (motor por defecto de MiKTeX) no trae glifos de
        // emoji y rompe la compilación a PDF. "carpeta/" en negrita alcanza
        // como señal visual y es seguro en LaTeX/RTF/DocBook/HTML por igual.
        let name = match node.node_type {
            NodeType::Directory => format!("**{}/**", node.label),
            _ => node.label.clone(),
        };

        let mut annotations = Vec::new();
        if let Some(lang) = &node.metadata.language {
            annotations.push(lang.clone());
        }
        if let Some(lines) = node.metadata.line_count {
            let word = if lines == 1 { "línea" } else { "líneas" };
            annotations.push(format!("{lines} {word}"));
        }
        if node.metadata.is_generated == Some(true) {
            annotations.push("generado".to_string());
        }
        let suffix = if annotations.is_empty() {
            String::new()
        } else {
            format!(" _({})_", annotations.join(", "))
        };

        out.push_str(&format!("{indent}- {name}{suffix}\n"));
        write_children(out, &node.id, children_of, depth + 1);
    }
}

/// Genera, para cada formato de `formats` distinto de "html" (que lo
/// produce el renderer HTML propio, no Pandoc), el documento correspondiente
/// en `out_dir` a partir de un único Markdown canónico. Devuelve las rutas
/// escritas.
pub fn render_docs(
    graph: &Graph,
    title: &str,
    formats: &[String],
    out_dir: &Path,
    pandoc_opts: &PandocOptions,
) -> Result<Vec<PathBuf>> {
    let markdown = graph_to_markdown(graph, title);
    let slug = slugify(title);
    let mut written = Vec::new();

    for format in formats {
        if format == "html" {
            continue;
        }
        let ext = match format.as_str() {
            "pdf" => "pdf",
            "xml" => "xml",
            "rtf" => "rtf",
            "latex" => "tex",
            other => anyhow::bail!("formato desconocido en [output].formats: {other}"),
        };
        let out_path = out_dir.join(format!("{slug}.{ext}"));
        pandoc::convert(&markdown, format, &out_path, pandoc_opts)?;
        written.push(out_path);
    }

    Ok(written)
}

fn slugify(input: &str) -> String {
    let normalized: String = input
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let collapsed = normalized
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        "ariadne".to_string()
    } else {
        collapsed
    }
}
