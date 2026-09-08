import type { Graph, GraphNode, TreeNode } from "./types.js";

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

export function readEmbeddedJson<T>(elementId: string): T {
  const el = document.getElementById(elementId);
  if (!el || !el.textContent) {
    throw new Error(`no se encontró el bloque de datos embebido #${elementId}`);
  }
  return JSON.parse(el.textContent) as T;
}
