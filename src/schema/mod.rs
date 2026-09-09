use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: &str = "1.0.0";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Root,
    Directory,
    File,
    // reservados para fases futuras (sin población en v1):
    Class,
    Object,
    Attribute,
    Method,
    Library,
    Import,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    Contains,
    // reservado: DependsOn (aristas library/import futuras)
    DependsOn,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct NodeMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Líneas del archivo, con corte en `LINE_COUNT_CAP` (ver extractor::classify).
    /// `None` para directorios/raíz.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_generated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Rol del archivo para el color del nodo — "test", "config", "docs",
    /// "styles", "markup", "script", o `None` para código fuente genérico.
    /// Complementa al ícono (que ya identifica el lenguaje): el color
    /// transmite algo que el ícono no dice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Familia visual del archivo por formato — "data", "image", "text",
    /// "markup", o `None` para código fuente genérico. Es un eje aparte de
    /// `category`: decide la FORMA del nodo en el cliente (círculo por
    /// defecto, rombo para datos, etc.), no su color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphNode {
    pub id: String,
    pub node_type: NodeType,
    pub label: String,
    pub path: String,
    pub parent_id: Option<String>,
    pub depth: u32,
    pub metadata: NodeMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphEdge {
    pub id: String,
    pub edge_type: EdgeType,
    pub source: String,
    pub target: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Graph {
    pub schema_version: String,
    pub root: String,
    pub generated_at: String,
    pub source_path: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}
