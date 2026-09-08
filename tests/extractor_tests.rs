use ariadne::config::IgnoreConfig;
use ariadne::extractor::build_graph;
use ariadne::schema::{Graph, NodeType};
use std::path::Path;

fn sample_graph() -> Graph {
    let root = Path::new("fixtures/sample-project");
    build_graph(root, "sample-project", &IgnoreConfig::default()).expect("build_graph debería funcionar")
}

#[test]
fn respects_target_gitignore() {
    let g = sample_graph();
    let ids: Vec<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
    assert!(!ids.iter().any(|id| id.starts_with("node_modules")));
    assert!(!ids.contains(&"config.secret"));
}

#[test]
fn includes_expected_files() {
    let g = sample_graph();
    let ids: Vec<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
    for expected in [
        "README.md",
        "src",
        "src/main.rs",
        "src/lib.py",
        "src/big.js",
        "src/medium.py",
        "dist",
        "dist/bundle.min.js",
    ] {
        assert!(ids.contains(&expected), "falta el nodo esperado: {expected}");
    }
}

#[test]
fn flags_generated_files() {
    let g = sample_graph();
    let bundle = g
        .nodes
        .iter()
        .find(|n| n.id == "dist/bundle.min.js")
        .expect("dist/bundle.min.js debería existir");
    assert_eq!(bundle.metadata.is_generated, Some(true));

    let readme = g
        .nodes
        .iter()
        .find(|n| n.id == "README.md")
        .expect("README.md debería existir");
    assert_eq!(readme.metadata.is_generated, Some(false));
}

#[test]
fn detects_language_and_icon() {
    let g = sample_graph();
    let main_rs = g
        .nodes
        .iter()
        .find(|n| n.id == "src/main.rs")
        .expect("src/main.rs debería existir");
    assert_eq!(main_rs.node_type, NodeType::File);
    assert_eq!(main_rs.metadata.language.as_deref(), Some("rust"));
    assert_eq!(main_rs.metadata.icon_key.as_deref(), Some("rust"));
}

#[test]
fn computes_depth_and_child_count() {
    let g = sample_graph();

    let src = g.nodes.iter().find(|n| n.id == "src").unwrap();
    assert_eq!(src.depth, 1);
    assert_eq!(src.metadata.child_count, Some(4));

    let main_rs = g.nodes.iter().find(|n| n.id == "src/main.rs").unwrap();
    assert_eq!(main_rs.depth, 2);

    let root = g.nodes.iter().find(|n| n.id == ".").unwrap();
    assert_eq!(root.node_type, NodeType::Root);
    assert_eq!(root.metadata.child_count, Some(3));
}

#[test]
fn computes_line_count_and_caps_at_2000() {
    let g = sample_graph();

    let medium = g.nodes.iter().find(|n| n.id == "src/medium.py").unwrap();
    assert_eq!(medium.metadata.line_count, Some(60));

    let big = g.nodes.iter().find(|n| n.id == "src/big.js").unwrap();
    assert_eq!(big.metadata.line_count, Some(2000), "un archivo de 2500 líneas debe cortarse en 2000");

    let src_dir = g.nodes.iter().find(|n| n.id == "src").unwrap();
    assert_eq!(src_dir.metadata.line_count, None, "los directorios no tienen line_count");
}
