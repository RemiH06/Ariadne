export type NodeType =
  | "root"
  | "directory"
  | "file"
  | "class"
  | "object"
  | "attribute"
  | "method"
  | "library"
  | "import";

export type EdgeType = "contains" | "depends_on";

export type FileCategory = "test" | "config" | "docs" | "styles" | "markup" | "script";

export interface NodeMetadata {
  extension?: string;
  size_bytes?: number;
  line_count?: number;
  child_count?: number;
  is_generated?: boolean;
  icon_key?: string;
  language?: string;
  /** Rol del archivo para el color del nodo — complementa al ícono (que ya
   * identifica el lenguaje) con algo que el ícono no dice. */
  category?: FileCategory;
  [key: string]: unknown;
}

export interface GraphNode {
  id: string;
  node_type: NodeType;
  label: string;
  path: string;
  parent_id: string | null;
  depth: number;
  metadata: NodeMetadata;
}

export interface GraphEdge {
  id: string;
  edge_type: EdgeType;
  source: string;
  target: string;
}

export interface Graph {
  schema_version: string;
  root: string;
  generated_at: string;
  source_path: string;
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface ColorsConfig {
  default: string;
  directory: string;
  file: string;
  class: string;
  object: string;
  attribute: string;
  method: string;
  library: string;
  import: string;
  test: string;
  config: string;
  docs: string;
  styles: string;
  markup: string;
  script: string;
}

export interface IconsConfig {
  enabled: boolean;
  set: string;
}

export interface HtmlRenderConfig {
  theme: string;
  background: string;
  link_color: string;
  text_color: string;
  colors: ColorsConfig;
  icons: IconsConfig;
}

export interface FilterDefaults {
  max_depth?: number;
  hide_generated: boolean;
  hide_extensions: string[];
  hide_paths: string[];
}

export interface RenderConfig {
  title: string;
  html: HtmlRenderConfig;
  filters: FilterDefaults;
}

/** Nodo del árbol anidado que consume d3-hierarchy (reconstruido desde el Graph plano). */
export interface TreeNode {
  data: GraphNode;
  children?: TreeNode[];
}
