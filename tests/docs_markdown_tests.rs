use ariadne::config::IgnoreConfig;
use ariadne::extractor::build_graph;
use ariadne::render::docs::graph_to_markdown;
use std::path::Path;

#[test]
fn renders_nested_markdown_from_graph() {
    let root = Path::new("fixtures/sample-project");
    let graph = build_graph(root, "sample-project", &IgnoreConfig::default())
        .expect("build_graph debería funcionar");

    let md = graph_to_markdown(&graph, "sample-project");

    assert!(md.starts_with("# sample-project\n"));

    // Los tres hijos directos de la raíz aparecen sin indentación, en el
    // orden alfabético en que build_graph ordena los hijos.
    assert!(md.contains("- README.md"));
    assert!(md.contains("- **dist/**"));
    assert!(md.contains("- **src/**"));
    let pos_readme = md.find("- README.md").unwrap();
    let pos_dist = md.find("- **dist/**").unwrap();
    let pos_src = md.find("- **src/**").unwrap();
    assert!(pos_readme < pos_dist && pos_dist < pos_src);

    // Los archivos anidados llevan un nivel de indentación y sus anotaciones.
    assert!(md.contains("  - main.rs _(rust,"));
    assert!(md.contains("  - bundle.min.js _(javascript, 1 línea, generado)_"));
    assert!(md.contains("  - big.js _(javascript, 2000 líneas)_"));

    // El directorio "dist" hereda la marca de generado.
    assert!(md.contains("- **dist/** _(generado)_"));

    // Sin emoji: pdflatex no trae esos glifos y rompería la compilación a PDF.
    assert!(!md.chars().any(|c| (c as u32) > 0x2500));
}
