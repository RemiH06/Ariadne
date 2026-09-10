use crate::config::{AriadneConfig, HtmlConfig};
use crate::extractor::build_graph;
use crate::render::docs::{build_doc_pages, check_available, render_docs, PandocOptions};
use crate::render::html::render_html;
use crate::utils::slugify;
use anyhow::{bail, Context, Result};
use clap::Args;
use std::path::PathBuf;

/// Genera el diagrama HTML interactivo y/o documentos (PDF/LaTeX/RTF/XML)
/// de un proyecto, a partir de `conf.ariadne` y overrides de línea de comandos.
#[derive(Args, Debug)]
pub struct GenerateArgs {
    /// Ruta al proyecto a analizar (por defecto: [project].path en conf.ariadne, o ".")
    pub path: Option<PathBuf>,

    /// Ruta a conf.ariadne (por defecto: se busca en el directorio actual)
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Directorio de salida (override de [output].dir)
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Formatos a generar, separados por coma: html,pdf,xml,rtf,latex (override de [output].formats)
    #[arg(long, value_delimiter = ',')]
    pub formats: Option<Vec<String>>,

    /// Preset de paleta para el HTML: "light" o "dark"
    #[arg(long)]
    pub theme: Option<String>,

    /// Color de fondo del HTML (override puntual sobre el theme/conf.ariadne)
    #[arg(long)]
    pub bg: Option<String>,

    /// Profundidad máxima inicial mostrada en el diagrama (override de [filters].max_depth)
    #[arg(long = "max-depth")]
    pub max_depth: Option<u32>,

    /// Ocultar archivos generados por defecto (override de [filters].hide_generated)
    #[arg(long = "hide-generated")]
    pub hide_generated: bool,

    /// Extensiones a ocultar por defecto, separadas por coma (override de [filters].hide_extensions)
    #[arg(long = "hide-ext", value_delimiter = ',')]
    pub hide_ext: Option<Vec<String>>,

    /// Nombre/título del proyecto (override de [project].name)
    #[arg(long)]
    pub title: Option<String>,
}

pub fn run(args: GenerateArgs) -> Result<()> {
    let mut cfg = AriadneConfig::load(args.config.as_deref())?;
    cfg.output.html.apply_theme_preset();
    apply_overrides(&mut cfg, &args);

    if cfg.output.formats.is_empty() {
        bail!("no hay formatos en [output].formats — agrega al menos uno (html, pdf, xml, rtf, latex)");
    }

    let root = args
        .path
        .clone()
        .unwrap_or_else(|| PathBuf::from(&cfg.project.path));
    if !root.exists() {
        bail!("la ruta del proyecto no existe: {}", root.display());
    }

    let title = if !cfg.project.name.is_empty() {
        cfg.project.name.clone()
    } else {
        root.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "proyecto".to_string())
    };

    println!("Analizando {}...", root.display());
    let mut graph = build_graph(&root, &title, &cfg.ignore)
        .with_context(|| format!("no se pudo analizar el proyecto en {}", root.display()))?;
    println!("{} nodos encontrados.", graph.nodes.len());

    // Páginas de documentación (mapeo explícito de conf.ariadne, ver
    // [[docs.pages]]) — build_graph no sabe nada de esto, es responsabilidad
    // del orquestador. Falla fuerte si una entrada está mal (nodo
    // inexistente o archivo no encontrado), antes de escribir nada.
    let doc_pages = build_doc_pages(&graph, &root, &cfg.docs.pages)?;
    for page in &doc_pages {
        if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == page.node_id) {
            node.metadata.doc_slug = Some(page.slug.clone());
        }
    }

    let out_dir = PathBuf::from(&cfg.output.dir);
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("no se pudo crear el directorio de salida: {}", out_dir.display()))?;

    let slug = slugify(&title);

    if cfg.output.formats.iter().any(|f| f == "html") {
        let html = render_html(&graph, &title, &cfg.output.html, &cfg.filters, &doc_pages)?;
        let out_path = out_dir.join(format!("{slug}.html"));
        std::fs::write(&out_path, html)
            .with_context(|| format!("no se pudo escribir {}", out_path.display()))?;
        println!("escrito: {}", out_path.display());
    }

    let doc_formats: Vec<String> = cfg
        .output
        .formats
        .iter()
        .filter(|f| f.as_str() != "html")
        .cloned()
        .collect();

    if !doc_formats.is_empty() {
        check_available(&cfg.output.pandoc.binary)?;
        let pandoc_opts = PandocOptions {
            binary: &cfg.output.pandoc.binary,
            pdf_engine: &cfg.output.pandoc.pdf_engine,
        };
        let written = render_docs(&graph, &title, &doc_formats, &out_dir, &pandoc_opts)?;
        for path in written {
            println!("escrito: {}", path.display());
        }
    }

    Ok(())
}

fn apply_overrides(cfg: &mut AriadneConfig, args: &GenerateArgs) {
    if let Some(title) = &args.title {
        cfg.project.name = title.clone();
    }
    if let Some(out) = &args.out {
        cfg.output.dir = out.to_string_lossy().to_string();
    }
    if let Some(formats) = &args.formats {
        cfg.output.formats = formats.clone();
    }
    if let Some(theme) = &args.theme {
        cfg.output.html = match theme.as_str() {
            "light" => HtmlConfig::light_preset(),
            _ => HtmlConfig::default(),
        };
    }
    if let Some(bg) = &args.bg {
        cfg.output.html.background = bg.clone();
    }
    if let Some(max_depth) = args.max_depth {
        cfg.filters.max_depth = Some(max_depth);
    }
    if args.hide_generated {
        cfg.filters.hide_generated = true;
    }
    if let Some(hide_ext) = &args.hide_ext {
        cfg.filters.hide_extensions = hide_ext.clone();
    }
}
