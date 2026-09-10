use ariadne::config::{FilterConfig, HtmlConfig, IgnoreConfig};
use ariadne::extractor::build_graph;
use ariadne::render::html::render_html;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let root = Path::new("fixtures/sample-project");
    let graph = build_graph(root, "sample-project", &IgnoreConfig::default())?;

    let html_cfg = HtmlConfig::default();
    let filters = FilterConfig::default();

    let html = render_html(&graph, "sample-project", &html_cfg, &filters, &[])?;
    std::fs::write("sample.html", html)?;
    println!("sample.html escrito ({} nodos)", graph.nodes.len());
    Ok(())
}
