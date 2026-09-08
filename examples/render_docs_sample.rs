use ariadne::config::{IgnoreConfig, PandocConfig as PandocCfg};
use ariadne::extractor::build_graph;
use ariadne::render::docs::{check_available, render_docs, PandocOptions};
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let root = Path::new("fixtures/sample-project");
    let graph = build_graph(root, "sample-project", &IgnoreConfig::default())?;

    let cfg = PandocCfg::default();
    check_available(&cfg.binary)?;

    let pandoc_opts = PandocOptions {
        binary: &cfg.binary,
        pdf_engine: &cfg.pdf_engine,
    };

    let out_dir = Path::new("docs-out");
    std::fs::create_dir_all(out_dir)?;

    let formats = vec![
        "xml".to_string(),
        "rtf".to_string(),
        "latex".to_string(),
        "pdf".to_string(),
    ];

    let written = render_docs(&graph, "sample-project", &formats, out_dir, &pandoc_opts)?;
    for path in written {
        println!("escrito: {}", path.display());
    }
    Ok(())
}
