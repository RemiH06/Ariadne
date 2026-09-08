import { hierarchy, tree, type HierarchyNode, type HierarchyPointNode } from "d3-hierarchy";
import { select, type Selection } from "d3-selection";
import { linkRadial } from "d3-shape";
import { zoom, zoomIdentity, type D3ZoomEvent } from "d3-zoom";
import { DIRECTION_VECTORS, type LayoutMode, type Vec2 } from "./layout.js";
import type { GraphNode, RenderConfig, TreeNode } from "./types.js";

const LINE_COUNT_CAP = 2000; // debe coincidir con extractor::classify::LINE_COUNT_CAP
const LEVEL_GAP = 90; // separación extra entre niveles/anillos
const ICON_SIZE = 16;
const PADDING_X = 12;
const MIN_BOX_HEIGHT = 24;
const BASE_PADDING_Y = 6;
const MAX_EXTRA_PADDING_Y = 14; // padding vertical extra para archivos grandes (hasta LINE_COUNT_CAP)
const MAX_LABEL_WIDTH = 200; // un solo label larguísimo no debe inflar el espaciado de las 8 orientaciones
const RADIAL_SEPARATION_SCALE = 2.2; // ajuste empírico para que los anillos internos no se amontonen
const SUB_LANE_GAP = 16; // separación entre sub-filas dentro de un mismo nivel (menor que LEVEL_GAP)
const SIBLING_FLOW_GAP = 14; // separación entre cajas consecutivas dentro de una sub-fila
const BAND_USAGE_FRACTION = 0.97; // % del ancho/alto de pantalla que puede usar cada franja antes de saltar de fila

interface BoxGeom {
  width: number;
  height: number;
  /** Alto de la pestaña de carpeta (0 si la forma no es "folder") — el
   * texto/ícono se corren hacia abajo esta distancia/2 para quedar
   * centrados en el cuerpo, no en la pestaña. */
  tabHeight: number;
}

type ShapeKind = "rect" | "folder";

/** root y directory son conceptualmente "contenedores", así que comparten
 * la silueta de carpeta; el resto (incluyendo los tipos reservados para
 * cuando exista extracción de clases/atributos/etc.) usa un rectángulo
 * simple hasta que tengan uso real y sepamos qué forma les conviene. */
const SHAPE_BY_TYPE: Partial<Record<GraphNode["node_type"], ShapeKind>> = {
  root: "folder",
  directory: "folder",
};

function shapeFor(nodeType: GraphNode["node_type"]): ShapeKind {
  return SHAPE_BY_TYPE[nodeType] ?? "rect";
}

interface PositionedNode {
  hnode: HierarchyNode<TreeNode>;
  center: Vec2;
  geom: BoxGeom;
  /** Para el modo radial: ángulo (rad) y radio del nodo, usados por el
   * generador de links polares. Ausente en los modos lineales. */
  polar?: { angle: number; radius: number };
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
  private direction: LayoutMode = "left-right";
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

  setDirection(direction: LayoutMode): void {
    this.direction = direction;
    if (this.lastTree) this.render(this.lastTree, { refit: true });
  }

  getDirection(): LayoutMode {
    return this.direction;
  }

  /** Reajusta el zoom/pan para que todo el diagrama entre en pantalla, sin
   * tocar el árbol ni la orientación actual. */
  fit(): void {
    if (this.lastTree) this.render(this.lastTree, { refit: true });
  }

  render(rootTree: TreeNode, opts: { refit?: boolean } = {}): void {
    this.lastTree = rootTree;

    const rootHierarchy = hierarchy<TreeNode>(rootTree, (d) => d.children);
    // Estructura (sin x/y todavía) para la pasada de medición: el espaciado
    // real depende del tamaño de las cajas, que solo se conoce después de
    // medir el texto — el layout de d3 corre más abajo.
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
    // nombre largo infla el espaciado uniforme compartido por todo el árbol
    // y termina forzando un zoom-out extremo. Se agrega un <title> con el
    // nombre completo como tooltip al pasar el mouse.
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
      const bodyHeight = Math.max(MIN_BOX_HEIGHT, bbox.height + paddingY * 2);
      const tabHeight = shapeFor(node.node_type) === "folder" ? Math.min(bodyHeight * 0.4, 10) : 0;
      geomById.set(node.id, {
        width: PADDING_X * 2 + iconGap + bbox.width,
        height: bodyHeight + tabHeight,
        tabHeight,
      });
    });

    const { positioned, links, pathFor } =
      this.direction === "radial"
        ? this.layoutRadial(rootHierarchy, geomById)
        : this.layoutLinear(rootHierarchy, geomById, DIRECTION_VECTORS[this.direction]);

    const positionById = new Map<string, PositionedNode>();
    for (const p of positioned) positionById.set(this.graphNode(p.hnode).id, p);

    nodeGroups.attr("transform", (d) => {
      const p = positionById.get(this.graphNode(d).id)!;
      return `translate(${p.center.x},${p.center.y})`;
    });

    // Paso 2: la forma (rect o carpeta), insertada detrás del texto.
    nodeGroups.each((d, i, groups) => {
      const node = this.graphNode(d);
      const geom = geomById.get(node.id)!;
      const group = select(groups[i]);
      let shape: Selection<SVGGraphicsElement, unknown, null, undefined>;
      if (shapeFor(node.node_type) === "folder") {
        shape = group
          .insert("path", "text")
          .attr("d", folderShapePath(geom.width, geom.height, geom.tabHeight)) as unknown as Selection<SVGGraphicsElement, unknown, null, undefined>;
      } else {
        shape = group
          .insert("rect", "text")
          .attr("x", -geom.width / 2)
          .attr("y", -geom.height / 2)
          .attr("width", geom.width)
          .attr("height", geom.height)
          .attr("rx", 6) as unknown as Selection<SVGGraphicsElement, unknown, null, undefined>;
      }
      shape
        .attr("class", "ariadne-node-box")
        .attr("fill", this.colorFor(node))
        .attr("stroke", this.config.html.background)
        .attr("stroke-width", 1.5);
    });

    textSel
      .attr("x", (d) => {
        const node = this.graphNode(d);
        const geom = geomById.get(node.id)!;
        const iconGap = this.hasIcon(node) ? ICON_SIZE + 6 : 0;
        return -geom.width / 2 + PADDING_X + iconGap;
      })
      .attr("y", (d) => geomById.get(this.graphNode(d).id)!.tabHeight / 2)
      .attr("fill", (d) => contrastTextColor(this.colorFor(this.graphNode(d))));

    nodeGroups
      .filter((d) => this.hasIcon(this.graphNode(d)))
      .insert("use", "text")
      .attr("href", (d) => `#icon-${this.graphNode(d).metadata.icon_key}`)
      .attr("width", ICON_SIZE)
      .attr("height", ICON_SIZE)
      .attr("x", (d) => -geomById.get(this.graphNode(d).id)!.width / 2 + PADDING_X - 2)
      .attr("y", (d) => geomById.get(this.graphNode(d).id)!.tabHeight / 2 - ICON_SIZE / 2);

    linkLayer
      .selectAll("path")
      .data(links)
      .join("path")
      .attr("d", (link) => pathFor(link));

    if (opts.refit) {
      this.fitToViewport(positioned);
    }
  }

  /** Árbol lineal: un vector de "profundidad" y uno de "hermanos" fijos
   * para todo el árbol (las 8 direcciones del compás). */
  private layoutLinear(
    rootHierarchy: HierarchyNode<TreeNode>,
    geomById: Map<string, BoxGeom>,
    vectors: { depth: Vec2; sibling: Vec2 }
  ): { positioned: PositionedNode[]; links: Array<{ source: HierarchyNode<TreeNode>; target: HierarchyNode<TreeNode> }>; pathFor: (link: any) => string } {
    const { depth: depthVec, sibling: sibVec } = vectors;
    const allNodes = rootHierarchy.descendants(); // pre-order: mantiene hermanos/parientes agrupados

    // Grosor de una sub-fila: proyección de la caja sobre el eje de
    // profundidad (igual que antes), usado para separar los niveles Y las
    // sub-filas dentro de un mismo nivel.
    let maxDepthHalfExtent = MIN_BOX_HEIGHT / 2;
    for (const geom of geomById.values()) {
      const halfW = geom.width / 2;
      const halfH = geom.height / 2;
      maxDepthHalfExtent = Math.max(maxDepthHalfExtent, halfW * Math.abs(depthVec.x) + halfH * Math.abs(depthVec.y));
    }
    const subLaneThickness = maxDepthHalfExtent * 2 + SUB_LANE_GAP;

    const svgNode = this.svg.node();
    const viewportWidth = svgNode?.clientWidth || 800;
    const viewportHeight = svgNode?.clientHeight || 600;
    const siblingAxisViewportSize = Math.abs(sibVec.x) * viewportWidth + Math.abs(sibVec.y) * viewportHeight;
    const availableBreadth = Math.max(siblingAxisViewportSize * BAND_USAGE_FRACTION, 300);

    // Cada nivel es una franja 2D, no una sola línea: los hermanos de un
    // mismo nivel se acomodan uno tras otro a lo largo del eje "hermanos"
    // (usando el 100% del ancho/alto disponible) y saltan a una sub-fila
    // nueva cuando no caben más — como un texto que hace salto de línea.
    // Así varios nodos del mismo nivel terminan en "alturas" (posiciones de
    // profundidad) distintas sin dejar de pertenecer al mismo nivel, y un
    // nivel con muchos hermanos deja de estirarse como una sola línea larga.
    interface FlowPos {
      row: number;
      breadthCenter: number;
    }
    const byDepth = new Map<number, HierarchyNode<TreeNode>[]>();
    let maxDepth = 0;
    for (const n of allNodes) {
      if (!byDepth.has(n.depth)) byDepth.set(n.depth, []);
      byDepth.get(n.depth)!.push(n);
      if (n.depth > maxDepth) maxDepth = n.depth;
    }

    const flowById = new Map<string, FlowPos>();
    const rowsUsedAtDepth = new Map<number, number>();
    for (const [depth, group] of byDepth) {
      let row = 0;
      let cursor = 0;
      for (const n of group) {
        const geom = geomById.get(this.graphNode(n).id)!;
        const halfW = geom.width / 2;
        const halfH = geom.height / 2;
        const breadthExtent = halfW * Math.abs(sibVec.x) + halfH * Math.abs(sibVec.y);
        const size = breadthExtent * 2 + SIBLING_FLOW_GAP;
        if (cursor > 0 && cursor + size > availableBreadth) {
          row += 1;
          cursor = 0;
        }
        flowById.set(this.graphNode(n).id, { row, breadthCenter: cursor + breadthExtent });
        cursor += size;
      }
      rowsUsedAtDepth.set(depth, row + 1);
    }

    // Posición base (eje de profundidad) de cada nivel: acumula el grosor
    // total (todas sus sub-filas) del nivel anterior más el espacio normal
    // entre niveles, para que un nivel con varias sub-filas no se encime
    // con el siguiente.
    const levelBaseDepthPos = new Map<number, number>([[0, 0]]);
    for (let d = 1; d <= maxDepth; d++) {
      const prevBase = levelBaseDepthPos.get(d - 1)!;
      const prevRows = rowsUsedAtDepth.get(d - 1) ?? 1;
      levelBaseDepthPos.set(d, prevBase + prevRows * subLaneThickness + LEVEL_GAP);
    }

    const positioned: PositionedNode[] = allNodes.map((hnode) => {
      const geom = geomById.get(this.graphNode(hnode).id)!;
      const flow = flowById.get(this.graphNode(hnode).id)!;
      const depthPos = levelBaseDepthPos.get(hnode.depth)! + flow.row * subLaneThickness;
      const siblingPos = flow.breadthCenter;
      const center: Vec2 = {
        x: depthPos * depthVec.x + siblingPos * sibVec.x,
        y: depthPos * depthVec.y + siblingPos * sibVec.y,
      };
      return { hnode, center, geom };
    });

    const positionById = new Map<string, PositionedNode>();
    for (const p of positioned) positionById.set(this.graphNode(p.hnode).id, p);

    const pathFor = (link: { source: HierarchyNode<TreeNode>; target: HierarchyNode<TreeNode> }): string => {
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
    };

    return { positioned, links: rootHierarchy.links(), pathFor };
  }

  /** Árbol radial: la raíz al centro, un anillo más separado por nivel
   * (radio = profundidad * paso), y los hijos de cada nodo repartidos en
   * círculo alrededor de él — nada va "en una sola dirección", así que un
   * árbol ancho y poco profundo aprovecha el espacio en 2D en vez de
   * estirarse como una línea delgada. */
  private layoutRadial(
    rootHierarchy: HierarchyNode<TreeNode>,
    geomById: Map<string, BoxGeom>
  ): { positioned: PositionedNode[]; links: Array<{ source: HierarchyNode<TreeNode>; target: HierarchyNode<TreeNode> }>; pathFor: (link: any) => string } {
    let maxCircumRadius = MIN_BOX_HEIGHT / 2;
    for (const geom of geomById.values()) {
      maxCircumRadius = Math.max(maxCircumRadius, Math.hypot(geom.width, geom.height) / 2);
    }
    const minDepthStep = maxCircumRadius * 2 + LEVEL_GAP;

    const maxDepth = Math.max(...rootHierarchy.descendants().map((n) => n.depth), 1);
    const svgNode = this.svg.node();
    const viewportWidth = svgNode?.clientWidth || 800;
    const viewportHeight = svgNode?.clientHeight || 600;
    const availableRadius = (Math.min(viewportWidth, viewportHeight) / 2) * 0.9;
    const depthStep = Math.max(minDepthStep, availableRadius / maxDepth);

    const radialLayout = tree<TreeNode>()
      .size([2 * Math.PI, 1])
      .separation((a, b) => (RADIAL_SEPARATION_SCALE * (a.parent === b.parent ? 1 : 2)) / Math.max(a.depth, 1));
    const laidOut = radialLayout(rootHierarchy);
    const pointNodes = laidOut.descendants();

    const positioned: PositionedNode[] = pointNodes.map((hnode) => {
      const geom = geomById.get(this.graphNode(hnode).id)!;
      const angle = hnode.x - Math.PI / 2;
      const radius = hnode.depth * depthStep;
      const center: Vec2 = { x: radius * Math.cos(angle), y: radius * Math.sin(angle) };
      return { hnode, center, geom, polar: { angle, radius } };
    });

    const positionById = new Map<string, PositionedNode>();
    for (const p of positioned) positionById.set(this.graphNode(p.hnode).id, p);

    const radialLinkGen = linkRadial<unknown, { x: number; y: number }>()
      .angle((d) => d.x)
      .radius((d) => d.y);

    const pathFor = (link: { source: HierarchyPointNode<TreeNode>; target: HierarchyPointNode<TreeNode> }): string => {
      const source = positionById.get(this.graphNode(link.source).id)!;
      const target = positionById.get(this.graphNode(link.target).id)!;
      const s = { x: source.polar!.angle + Math.PI / 2, y: source.polar!.radius };
      const t = { x: target.polar!.angle + Math.PI / 2, y: target.polar!.radius };
      return radialLinkGen({ source: s, target: t } as never) ?? "";
    };

    return { positioned, links: laidOut.links(), pathFor };
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

  /** El ícono de un archivo ya identifica su lenguaje, así que el color
   * prioriza `category` (test/config/docs/styles/markup/script) — algo que
   * el ícono no dice — y solo cae al color por `node_type` cuando el
   * archivo no cae en ninguna categoría reconocida. */
  private colorFor(node: GraphNode): string {
    const colors = this.config.html.colors as unknown as Record<string, string>;
    const category = node.metadata.category;
    if (category && colors[category]) return colors[category];
    return colors[node.node_type] ?? colors.default;
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

/** Silueta de carpeta (rectángulo con una pestaña arriba a la izquierda),
 * centrada en el origen como el resto de las formas. `tabHeight` viene del
 * mismo cálculo que ya infló el `height` del BoxGeom. */
function folderShapePath(width: number, height: number, tabHeight: number): string {
  const tabWidth = Math.min(width * 0.4, 40);
  const left = -width / 2;
  const right = width / 2;
  const top = -height / 2;
  const bottom = height / 2;
  const bodyTop = top + tabHeight;
  return [
    `M ${left} ${bodyTop}`,
    `L ${left} ${bottom}`,
    `L ${right} ${bottom}`,
    `L ${right} ${bodyTop}`,
    `L ${left + tabWidth + tabHeight} ${bodyTop}`,
    `L ${left + tabWidth} ${top}`,
    `L ${left} ${top}`,
    "Z",
  ].join(" ");
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
