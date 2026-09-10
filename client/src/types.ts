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

export type FileShape = "data" | "image" | "text" | "markup";

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
  /** Familia visual por formato — decide la FORMA del nodo (círculo por
   * defecto si no está presente), un eje aparte de `category`/color. */
  shape?: FileShape;
  /** Autor del último commit que tocó este nodo (heurística vía `git log`,
   * no un `git blame` línea por línea real). */
  last_author?: string;
  /** Fecha RFC3339 del último commit — ver `last_author`. */
  last_modified?: string;
  /** Últimos commits que tocaron este archivo, más reciente primero. Solo
   * en nodos `file` — alimenta la acción "ver historial" al seleccionarlo. */
  recent_commits?: CommitInfo[];
  [key: string]: unknown;
}

/** Un commit del historial corto embebido — ver `NodeMetadata.recent_commits`. */
export interface CommitInfo {
  short_hash: string;
  author: string;
  /** RFC3339. */
  timestamp: string;
  subject: string;
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
