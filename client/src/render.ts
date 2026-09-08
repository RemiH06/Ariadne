import { hierarchy, tree, type HierarchyNode, type HierarchyPointNode } from "d3-hierarchy";
import { select, type Selection } from "d3-selection";
import { zoom, zoomIdentity, type D3ZoomEvent } from "d3-zoom";
import { DIRECTION_VECTORS, type LayoutDirection, type Vec2 } from "./layout.js";
import type { GraphNode, RenderConfig, TreeNode } from "./types.js";

const LINE_COUNT_CAP = 2000; // debe coincidir con extractor::classify::LINE_COUNT_CAP
const SIBLING_GAP = 44;
const LEVEL_GAP = 64;
const ICON_SIZE = 16;
const PADDING_X = 12;
const MIN_BOX_HEIGHT = 24;
const BASE_PADDING_Y = 6;
const MAX_EXTRA_PADDING_Y = 14; // padding vertical extra para archivos grandes (hasta LINE_COUNT_CAP)
const MAX_LABEL_WIDTH = 200; // un solo label larguísimo no debe inflar el espaciado de las 8 orientaciones

interface BoxGeom {
  width: number;
  height: number;
}

interface PositionedNode {
  hnode: HierarchyPointNode<TreeNode>;
  center: Vec2;
  geom: BoxGeom;
}

export interface RenderCallbacks {
  onToggleCollapse: (nodeId: string) => void;
}

export class DiagramRenderer {
  private svg: Selection<SVGSVGElement, unknown, null, undefined>;
  private viewport: Selection<SVGGElement, unknown, null, undefined>;
  private readonly config: RenderConfig;
  private readonly callbacks: RenderCallbacks;
  private readonly zoomBehavior = zoom<SVGSVGElement, unknown>().scaleExtent([0.05, 8]);
  private direction: LayoutDirection = "left-right";
  private lastTree: TreeNode | null = null;

  constructor(svgEl: SVGSVGElement, config: RenderConfig, callbacks: RenderCallbacks) {
    this.svg = select(svgEl);
    this.config = config;
    this.callbacks = callbacks;

    this.viewport = this.svg.append("g").attr("class", "ariadne-viewport");

    this.zoomBehavior.on("zoom", (event: D3ZoomEvent<SVGSVGElement, unknown>) => {
      this.viewport.attr("transform", event.transform.toString());
    });
    this.svg.call(this.zoomBehavior);
  }

  setDirection(direction: LayoutDirection): void {
    this.direction = direction;
    if (this.lastTree) this.render(this.lastTree, { refit: true });
  }

  getDirection(): LayoutDirection {
    return this.direction;
  }

  /** Reajusta el zoom/pan para que todo el diagrama entre en pantalla, sin
   * tocar el árbol ni la orientación actual. */
  fit(): void {
    if (this.lastTree) this.render(this.lastTree, { refit: true });
  }

  render(rootTree: TreeNode, opts: { refit?: boolean } = {}): void {
    this.lastTree = rootTree;
    const { depth: depthVec, sibling: sibVec } = DIRECTION_VECTORS[this.direction];

    const rootHierarchy = hierarchy<TreeNode>(rootTree, (d) => d.children);
    // Estructura (sin x/y todavía) para la pasada de medición: el espaciado
    // real entre hermanos depende del tamaño de las cajas, que solo se
    // conoce después de medir el texto — el layout de d3 corre más abajo,
    // una vez calculado `siblingStep`.
    const nodes = rootHierarchy.descendants();

    this.viewport.selectAll("*").remove();

    const linkLayer = this.viewport
      .append("g")
      .attr("class", "ariadne-links")
      .attr("fill", "none")
      .attr("stroke", this.config.html.link_color)
      .attr("stroke-width", 1.5);
    const nodeLayer = this.viewport.append("g").attr("class", "ariadne-nodes");

    const nodeGroups = nodeLayer
      .selectAll<SVGGElement, HierarchyNode<TreeNode>>("g")
      .data(nodes)
      .join("g")
      .attr("class", "ariadne-node")
      .style("cursor", (d) => (d.data.children || (d.data.children === undefined && this.graphNode(d).metadata.child_count) ? "pointer" : "default"))
      .on("click", (_event: MouseEvent, d: HierarchyNode<TreeNode>) => {
        this.callbacks.onToggleCollapse(this.graphNode(d).id);
      });

    // Paso 1: texto primero (sin caja aún) para poder medirlo con getBBox().
    const textSel = nodeGroups
      .append("text")
      .attr("class", "ariadne-node-label")
      .attr("dy", "0.32em")
      .style("font", "13px system-ui, sans-serif")
      .text((d) => this.graphNode(d).label);

    // Truncar con elipsis las etiquetas larguísimas: sin esto, un solo
    // nombre largo infla el espaciado uniforme compartido por las 8
    // orientaciones y termina forzando un zoom-out extremo de todo el árbol.
    // Se agrega un <title> con el nombre completo como tooltip al pasar el mouse.
    textSel.each((d, i, groups) => {
      const textEl = groups[i] as SVGTextElement;
      const fullLabel = this.graphNode(d).label;
      const truncated = truncateToWidth(textEl, fullLabel, MAX_LABEL_WIDTH);
      if (truncated) {
        select(textEl).append("title").text(fullLabel);
      }
    });

    const geomById = new Map<string, BoxGeom>();
    textSel.each((d, i, groups) => {
      const node = this.graphNode(d);
      const textEl = groups[i] as SVGTextElement;
      const bbox = textEl.getBBox();
      const iconGap = this.hasIcon(node) ? ICON_SIZE + 6 : 0;
      const paddingY = BASE_PADDING_Y + this.extraPaddingFor(node);
      geomById.set(node.id, {
        width: PADDING_X * 2 + iconGap + bbox.width,
        height: Math.max(MIN_BOX_HEIGHT, bbox.height + paddingY * 2),
      });
    });

    // Paso de espaciado: se proyecta cada caja sobre el eje real de
    // profundidad/hermanos de la orientación elegida (no el máximo entre
    // ancho y alto a ciegas) — así left-right sigue usando solo el alto de
    // la caja para separar hermanos, top-bottom solo el ancho, y las
    // diagonales combinan ambos proporcionalmente, sin desperdiciar espacio
    // ni producir superposiciones en ningún caso.
    let maxDepthHalfExtent = MIN_BOX_HEIGHT / 2;
    let maxSiblingHalfExtent = MIN_BOX_HEIGHT / 2;
    for (const geom of geomById.values()) {
      const halfW = geom.width / 2;
      const halfH = geom.height / 2;
      maxDepthHalfExtent = Math.max(maxDepthHalfExtent, halfW * Math.abs(depthVec.x) + halfH * Math.abs(depthVec.y));
      maxSiblingHalfExtent = Math.max(maxSiblingHalfExtent, halfW * Math.abs(sibVec.x) + halfH * Math.abs(sibVec.y));
    }
    const depthStep = maxDepthHalfExtent * 2 + LEVEL_GAP;
    const siblingStep = maxSiblingHalfExtent * 2 + (SIBLING_GAP - MIN_BOX_HEIGHT);

    // Recién ahora corre el layout de d3, con el paso de espaciado final —
    // así `hnode.x` ya viene correctamente escalado y no hace falta
    // reescalarlo después (eso era lo que componía el error en todo el árbol).
    const laidOut = tree<TreeNode>().nodeSize([siblingStep, 1])(rootHierarchy);
    const pointNodes = laidOut.descendants();

    const positioned: PositionedNode[] = pointNodes.map((hnode) => {
      const geom = geomById.get(this.graphNode(hnode).id)!;
      const depthPos = hnode.depth * depthStep;
      const siblingPos = hnode.x;
      const center: Vec2 = {
        x: depthPos * depthVec.x + siblingPos * sibVec.x,
        y: depthPos * depthVec.y + siblingPos * sibVec.y,
      };
      return { hnode, center, geom };
    });
    const positionById = new Map<string, PositionedNode>();
    for (const p of positioned) positionById.set(this.graphNode(p.hnode).id, p);

    nodeGroups.attr("transform", (d) => {
      const p = positionById.get(this.graphNode(d).id)!;
      return `translate(${p.center.x},${p.center.y})`;
    });

    // Paso 2: la caja, insertada detrás del texto.
    nodeGroups
      .insert("rect", "text")
      .attr("class", "ariadne-node-box")
      .attr("x", (d) => -geomById.get(this.graphNode(d).id)!.width / 2)
      .attr("y", (d) => -geomById.get(this.graphNode(d).id)!.height / 2)
      .attr("width", (d) => geomById.get(this.graphNode(d).id)!.width)
      .attr("height", (d) => geomById.get(this.graphNode(d).id)!.height)
      .attr("rx", 6)
      .attr("fill", (d) => this.colorFor(this.graphNode(d).node_type))
      .attr("stroke", this.config.html.background)
      .attr("stroke-width", 1.5);

    textSel
      .attr("x", (d) => {
        const node = this.graphNode(d);
        const geom = geomById.get(node.id)!;
        const iconGap = this.hasIcon(node) ? ICON_SIZE + 6 : 0;
        return -geom.width / 2 + PADDING_X + iconGap;
      })
      .attr("fill", (d) => contrastTextColor(this.colorFor(this.graphNode(d).node_type)));

    nodeGroups
      .filter((d) => this.hasIcon(this.graphNode(d)))
      .insert("use", "text")
      .attr("href", (d) => `#icon-${this.graphNode(d).metadata.icon_key}`)
      .attr("width", ICON_SIZE)
      .attr("height", ICON_SIZE)
      .attr("x", (d) => -geomById.get(this.graphNode(d).id)!.width / 2 + PADDING_X - 2)
      .attr("y", -ICON_SIZE / 2);

    // Links: salen de la caja del padre en la dirección de "profundidad" y
    // entran a la del hijo desde la dirección opuesta, sin importar la
    // orientación elegida.
    linkLayer
      .selectAll("path")
      .data(laidOut.links())
      .join("path")
      .attr("d", (link) => {
        const source = positionById.get(this.graphNode(link.source).id)!;
        const target = positionById.get(this.graphNode(link.target).id)!;
        const exitDist = boxExitDistance(source.geom, depthVec);
        const entryDist = boxExitDistance(target.geom, depthVec);
        const exit: Vec2 = { x: source.center.x + depthVec.x * exitDist, y: source.center.y + depthVec.y * exitDist };
        const entry: Vec2 = { x: target.center.x - depthVec.x * entryDist, y: target.center.y - depthVec.y * entryDist };
        const dist = Math.hypot(entry.x - exit.x, entry.y - exit.y) / 2;
        const c1: Vec2 = { x: exit.x + depthVec.x * dist, y: exit.y + depthVec.y * dist };
        const c2: Vec2 = { x: entry.x - depthVec.x * dist, y: entry.y - depthVec.y * dist };
        return `M${exit.x},${exit.y} C${c1.x},${c1.y} ${c2.x},${c2.y} ${entry.x},${entry.y}`;
      });

    if (opts.refit) {
      this.fitToViewport(positioned);
    }
  }

  private graphNode(d: HierarchyNode<TreeNode>): GraphNode {
    return d.data.data;
  }

  /** Padding vertical extra para archivos, proporcional al área (raíz
   * cuadrada del conteo de líneas) — así la caja "se siente" más grande
   * cuanto más código tiene el archivo, sin dejar de contener el texto. */
  private extraPaddingFor(node: GraphNode): number {
    if (node.node_type !== "file") return 0;
    const lines = node.metadata.line_count;
    if (!lines || lines <= 0) return 0;
    const t = Math.sqrt(Math.min(lines, LINE_COUNT_CAP) / LINE_COUNT_CAP);
    return t * MAX_EXTRA_PADDING_Y;
  }

  private hasIcon(node: GraphNode): boolean {
    return this.config.html.icons.enabled && !!node.metadata.icon_key;
  }

  private colorFor(nodeType: GraphNode["node_type"]): string {
    const colors = this.config.html.colors as unknown as Record<string, string>;
    return colors[nodeType] ?? colors.default;
  }

  private fitToViewport(positioned: PositionedNode[]): void {
    if (positioned.length === 0) return;
    const svgNode = this.svg.node();
    if (!svgNode) return;

    const width = svgNode.clientWidth || 800;
    const height = svgNode.clientHeight || 600;

    let minX = Infinity;
    let maxX = -Infinity;
    let minY = Infinity;
    let maxY = -Infinity;
    for (const p of positioned) {
      minX = Math.min(minX, p.center.x - p.geom.width / 2);
      maxX = Math.max(maxX, p.center.x + p.geom.width / 2);
      minY = Math.min(minY, p.center.y - p.geom.height / 2);
      maxY = Math.max(maxY, p.center.y + p.geom.height / 2);
    }

    const contentWidth = maxX - minX + 60;
    const contentHeight = maxY - minY + 60;
    const scale = Math.min(1, 0.9 * Math.min(width / contentWidth, height / contentHeight));
    const tx = width / 2 - ((minX + maxX) / 2) * scale;
    const ty = height / 2 - ((minY + maxY) / 2) * scale;

    this.svg.call(this.zoomBehavior.transform, zoomIdentity.translate(tx, ty).scale(scale));
  }
}

/** Recorta `label` con "…" hasta que quepa en `maxWidth`, midiendo con
 * getBBox() en el propio elemento. Devuelve true si tuvo que truncar. */
function truncateToWidth(textEl: SVGTextElement, label: string, maxWidth: number): boolean {
  textEl.textContent = label;
  if (textEl.getBBox().width <= maxWidth || label.length <= 1) return false;

  let lo = 0;
  let hi = label.length - 1;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    textEl.textContent = label.slice(0, mid) + "…";
    if (textEl.getBBox().width <= maxWidth) {
      lo = mid;
    } else {
      hi = mid - 1;
    }
  }
  textEl.textContent = label.slice(0, lo) + "…";
  return true;
}

/** Distancia desde el centro de una caja axis-aligned hasta el punto donde
 * un rayo en dirección `dir` (unitaria) cruza su borde — exacta para
 * direcciones cardinales y diagonales de 45°. */
function boxExitDistance(geom: BoxGeom, dir: Vec2): number {
  const halfW = geom.width / 2;
  const halfH = geom.height / 2;
  const candidates: number[] = [];
  if (dir.x !== 0) candidates.push(halfW / Math.abs(dir.x));
  if (dir.y !== 0) candidates.push(halfH / Math.abs(dir.y));
  return Math.min(...candidates);
}

/** Luminancia relativa aproximada para elegir texto negro o blanco legible
 * sobre el color de fondo de la caja, sin importar la paleta configurada. */
function contrastTextColor(hex: string): string {
  const clean = hex.replace("#", "");
  const r = parseInt(clean.substring(0, 2), 16) / 255;
  const g = parseInt(clean.substring(2, 4), 16) / 255;
  const b = parseInt(clean.substring(4, 6), 16) / 255;
  const luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return luminance > 0.55 ? "#111111" : "#f5f5f5";
}
