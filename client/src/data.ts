import type { Graph, GraphNode, TreeNode } from "./types.js";

export interface RefEdge {
  source: string;
  target: string;
}

export interface FilterState {
  hideGenerated: boolean;
  maxDepth: number | null;
  hideExtensions: Set<string>;
}

function passesOwnFilter(node: GraphNode, filters: FilterState): boolean {
  if (node.node_type === "root") return true;
  if (filters.hideGenerated && node.metadata.is_generated) return false;
  if (filters.maxDepth !== null && node.depth > filters.maxDepth) return false;
  if (node.metadata.extension && filters.hideExtensions.has(node.metadata.extension)) {
    return false;
  }
  return true;
}

/**
 * Reconstruye el árbol anidado que espera d3-hierarchy a partir del Graph
 * plano, aplicando los filtros activos. Si un nodo se excluye, todo su
 * subárbol se excluye con él (se recorre en orden de profundidad creciente
 * para que la exclusión de un padre se propague a sus hijos).
 */
export function buildFilteredTree(
  graph: Graph,
  filters: FilterState,
  collapsed: ReadonlySet<string>
): TreeNode | null {
  const byId = new Map<string, GraphNode>();
  for (const node of graph.nodes) byId.set(node.id, node);

  const sorted = [...graph.nodes].sort((a, b) => a.depth - b.depth);
  const excluded = new Set<string>();

  for (const node of sorted) {
    const parentExcluded = node.parent_id !== null && excluded.has(node.parent_id);
    if (parentExcluded || !passesOwnFilter(node, filters)) {
      excluded.add(node.id);
    }
  }

  const childrenOf = new Map<string, GraphNode[]>();
  for (const node of sorted) {
    if (node.id === graph.root || excluded.has(node.id)) continue;
    if (node.parent_id === null) continue;
    if (!childrenOf.has(node.parent_id)) childrenOf.set(node.parent_id, []);
    childrenOf.get(node.parent_id)!.push(node);
  }

  const rootNode = byId.get(graph.root);
  if (!rootNode) return null;

  const toTreeNode = (node: GraphNode): TreeNode => {
    const kids = childrenOf.get(node.id) ?? [];
    const tree: TreeNode = { data: node };
    if (kids.length > 0 && !collapsed.has(node.id)) {
      tree.children = kids
        .slice()
        .sort((a, b) => a.label.localeCompare(b.label))
        .map(toTreeNode);
    }
    return tree;
  };

  return toTreeNode(rootNode);
}

/** ids de todos los nodos que quedaron visibles en el árbol ya filtrado y
 * con colapsos aplicados — un nodo colapsado sigue visible, sus
 * descendientes no (nunca se agregaron a `children`). */
export function collectVisibleIds(tree: TreeNode): Set<string> {
  const ids = new Set<string>();
  const walk = (node: TreeNode) => {
    ids.add(node.data.id);
    node.children?.forEach(walk);
  };
  walk(tree);
  return ids;
}

/** Aristas "depends_on" (referencias entre archivos) cuyos dos extremos
 * siguen visibles — evita dibujar líneas hacia nodos ocultos por un
 * filtro o un colapso manual. */
export function getVisibleReferenceEdges(graph: Graph, visibleIds: ReadonlySet<string>): RefEdge[] {
  const edges: RefEdge[] = [];
  for (const edge of graph.edges) {
    if (edge.edge_type !== "depends_on") continue;
    if (visibleIds.has(edge.source) && visibleIds.has(edge.target)) {
      edges.push({ source: edge.source, target: edge.target });
    }
  }
  return edges;
}

export function readEmbeddedJson<T>(elementId: string): T {
  const el = document.getElementById(elementId);
  if (!el || !el.textContent) {
    throw new Error(`no se encontró el bloque de datos embebido #${elementId}`);
  }
  return JSON.parse(el.textContent) as T;
}
