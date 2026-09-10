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

/// Un commit del historial corto embebido para "ver historial" — ver
/// extractor::git_blame. No es el historial completo, solo los últimos
/// N commits que tocaron el archivo (N decidido al generar el grafo).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommitInfo {
    pub short_hash: String,
    pub author: String,
    /// RFC3339.
    pub timestamp: String,
    pub subject: String,
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
    /// Autor del último commit que tocó este archivo/carpeta — heurística
    /// vía `git log` (ver extractor::git_blame), no un `git blame` línea
    /// por línea real. `None` si el proyecto no es un repo git, no hay
    /// `git` instalado, o el nodo no tiene commits propios.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_author: Option<String>,
    /// Fecha (RFC3339) del último commit — ver `last_author`. Para
    /// carpetas y la raíz es el máximo entre sus hijos directos (el
    /// archivo modificado más recientemente en ese subárbol).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    /// Últimos commits que tocaron este archivo, más reciente primero — ver
    /// `CommitInfo`. Solo se puebla para nodos `File` (no carpetas/raíz);
    /// alimenta la acción "ver historial" al seleccionar un nodo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_commits: Option<Vec<CommitInfo>>,
    /// Slug de la página de documentación asociada a este nodo, si hay
    /// alguna declarada en `[[docs.pages]]` de `conf.ariadne` — ver
    /// `render::docs`. El cliente usa esto para marcar el nodo con una
    /// estrella y ofrecer "ir a documentación".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_slug: Option<String>,
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
