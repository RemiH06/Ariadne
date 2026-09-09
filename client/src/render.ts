import { hierarchy, tree, type HierarchyNode, type HierarchyPointNode } from "d3-hierarchy";
import { select, type Selection } from "d3-selection";
import { linkRadial } from "d3-shape";
import { zoom, zoomIdentity, type D3ZoomEvent } from "d3-zoom";
import type { RefEdge } from "./data.js";
import { DIRECTION_VECTORS, type LayoutMode, type Vec2 } from "./layout.js";
import type { GraphNode, RenderConfig, TreeNode } from "./types.js";

const LINE_COUNT_CAP = 2000; // debe coincidir con extractor::classify::LINE_COUNT_CAP
const NODE_SCALE = 2; // factor de tamaño de todos los nodos (íconos, texto, padding, figuras) — no toca la separación entre ellos
const LEVEL_GAP = 90; // separación extra entre niveles/anillos
const ICON_SIZE = 16 * NODE_SCALE;
const PADDING_X = 12 * NODE_SCALE;
const MIN_BOX_HEIGHT = 24 * NODE_SCALE;
const BASE_PADDING_Y = 6 * NODE_SCALE;
const MAX_EXTRA_PADDING_Y = 14 * NODE_SCALE; // padding vertical extra para archivos grandes (hasta LINE_COUNT_CAP)
const MAX_LABEL_WIDTH = 200 * NODE_SCALE; // un solo label larguísimo no debe inflar el espaciado de las 8 orientaciones
const FONT_SIZE = 13 * NODE_SCALE;
const RADIAL_SEPARATION_SCALE = 2.2; // ajuste empírico para que los anillos internos no se amontonen
const SUB_LANE_GAP = 16; // separación entre sub-filas dentro de un mismo nivel (menor que LEVEL_GAP)
const SIBLING_FLOW_GAP = 14; // separación entre cajas consecutivas dentro de una sub-fila
const BAND_USAGE_FRACTION = 0.97; // % del ancho/alto de pantalla que puede usar cada franja antes de saltar de fila
const CENTERED_INNER_PAD_Y = 8 * NODE_SCALE;
const ICON_TEXT_GAP = 4 * NODE_SCALE; // separación ícono/texto en el layout centrado (vertical, no el de fila)
const SHAPE_SIZE_GROWTH = 0.7; // crecimiento máximo (fracción) por tamaño de archivo, en figuras centradas
const REGULAR_SHAPE_MARGIN = 1.15; // margen del radio sobre el contenido — el texto puede salirse, esto es solo para que el ícono no quede pegado al borde
const OVERFLOW_RESERVE = 16 * NODE_SCALE; // margen extra que el layout reserva alrededor de una etiqueta que se sale de su figura, para que no toque al vecino
const TEXT_HALO_WIDTH = 3 * NODE_SCALE;

interface BoxGeom {
  width: number;
  height: number;
  /** Alto de la pestaña de carpeta (0 si la forma no es "folder") — el
   * texto/ícono se corren hacia abajo esta distancia/2 para quedar
   * centrados en el cuerpo, no en la pestaña. */
  tabHeight: number;
  /** Alto del bloque ícono+texto (0 para "folder"/"book", que no lo
   * usan) — sirve para centrar ese bloque dentro de las figuras "centradas"
   * (círculo, hexágono, rombo, etc.), donde el texto puede salirse de la
   * figura sin problema. */
  contentHeight: number;
  /** Radio (circunradio) de la figura, solo para formas centradas — hace
   * falta guardarlo aparte de width/height porque un polígono regular no
   * tiene por qué medir exactamente 2*radio de alto (p. ej. un triángulo
   * mide 1.5*radio), y hay que volver a generar los mismos puntos al dibujar. */
  radius: number;
}

type ShapeKind = "book" | "folder" | "circle" | "hexagon" | "diamond" | "triangle" | "pentagon" | "octagon";

/** root y directory son conceptualmente "contenedores", así que comparten
 * la silueta de carpeta; library (dependencia externa, no viene del disco)
 * se dibuja como un "libro" (rectángulo con lomo) para distinguirla de los
 * archivos; los tipos reservados sin uso real todavía (object/attribute/
 * method) caen al círculo genérico; class (reservado) ya tiene forma
 * asignada (hexágono) aunque todavía no se emitan nodos de ese tipo. */
const NODE_TYPE_SHAPE: Partial<Record<GraphNode["node_type"], ShapeKind>> = {
  root: "folder",
  directory: "folder",
  library: "book",
  class: "hexagon",
};

/** Para archivos: la familia visual por formato (`metadata.shape`, ver
 * extractor::classify::file_shape en Rust) decide la forma; sin ella, un
 * archivo es un círculo por defecto — distinto de la carpeta/libro, para
 * diferenciar visualmente archivos del mismo color. */
const FILE_SHAPE_BY_METADATA: Record<string, ShapeKind> = {
  data: "diamond",
  image: "triangle",
  text: "pentagon",
  markup: "octagon",
};

function shapeFor(node: GraphNode): ShapeKind {
  const byType = NODE_TYPE_SHAPE[node.node_type];
  if (byType) return byType;
  if (node.node_type === "file") {
    const metaShape = node.metadata.shape;
    return (metaShape && FILE_SHAPE_BY_METADATA[metaShape]) || "circle";
  }
  return "circle";
}

/** Cantidad de lados y rotación (grados) de cada figura regular — todas
 * inscritas en el mismo circunradio, así que el "peso" visual es
 * consistente entre ellas. La rotación decide la orientación (vértice vs.
 * arista arriba): hexágono y octágono quedan con arista plana arriba/abajo,
 * rombo/triángulo/pentágono quedan apuntando hacia arriba. */
const REGULAR_SHAPE_SPEC: Partial<Record<ShapeKind, { sides: number; rotationDeg: number }>> = {
  diamond: { sides: 4, rotationDeg: -90 },
  triangle: { sides: 3, rotationDeg: -90 },
  pentagon: { sides: 5, rotationDeg: -90 },
  hexagon: { sides: 6, rotationDeg: 0 },
  octagon: { sides: 8, rotationDeg: 22.5 },
};

/** "folder"/"book" usan el layout de fila (ícono a la izquierda, texto a la
 * derecha, todo alineado a la izquierda de la caja) — el resto usa un
 * layout centrado (ícono arriba, texto abajo, todo centrado) porque una
 * fila angosta no se ve bien inscrita en un círculo/hexágono/rombo/etc. */
function isRowShape(kind: ShapeKind): boolean {
  return kind === "book" || kind === "folder";
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
  private lastRefEdges: RefEdge[] = [];
  private showReferences = false;
  private focusedId: string | null = null;

  constructor(svgEl: SVGSVGElement, config: RenderConfig, callbacks: RenderCallbacks) {
    this.svg = select(svgEl);
    this.config = config;
    this.callbacks = callbacks;

    this.viewport = this.svg.append("g").attr("class", "ariadne-viewport");

    this.zoomBehavior.on("zoom", (event: D3ZoomEvent<SVGSVGElement, unknown>) => {
      this.viewport.attr("transform", event.transform.toString());
    });
    this.svg.call(this.zoomBehavior);

    // Click en el fondo (no en un nodo): quita el focus actual.
    this.svg.on("click", (event: MouseEvent) => {
      if (event.target === svgEl) this.setFocusedNode(null);
    });
  }

  /** Resalta las referencias directas (entrantes y salientes) de un nodo,
   * atenuando el resto del diagrama. `null` quita el focus. Pasar el mismo
   * id ya enfocado lo quita (toggle). */
  setFocusedNode(id: string | null): void {
    this.focusedId = this.focusedId === id ? null : id;
    if (this.lastTree) this.render(this.lastTree, this.lastRefEdges);
  }

  setDirection(direction: LayoutMode): void {
    this.direction = direction;
    if (this.lastTree) this.render(this.lastTree, this.lastRefEdges, { refit: true });
  }

  getDirection(): LayoutMode {
    return this.direction;
  }

  /** Reajusta el zoom/pan para que todo el diagrama entre en pantalla, sin
   * tocar el árbol ni la orientación actual. */
  fit(): void {
    if (this.lastTree) this.render(this.lastTree, this.lastRefEdges, { refit: true });
  }

  setShowReferences(show: boolean): void {
    this.showReferences = show;
    if (this.lastTree) this.render(this.lastTree, this.lastRefEdges);
  }

  render(rootTree: TreeNode, refEdges: RefEdge[] = [], opts: { refit?: boolean } = {}): void {
    this.lastTree = rootTree;
    this.lastRefEdges = refEdges;

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
      .style("cursor", "pointer")
      .on("click", (_event: MouseEvent, d: HierarchyNode<TreeNode>) => {
        const node = this.graphNode(d);
        if (this.hasChildren(d)) {
          this.callbacks.onToggleCollapse(node.id);
        } else {
          // las hojas no tienen nada que colapsar — el click enfoca sus
          // referencias directas (imports entrantes/salientes) en su lugar.
          this.setFocusedNode(node.id);
        }
      });

    // Paso 1: texto primero (sin caja aún) para poder medirlo con getBBox().
    const textSel = nodeGroups
      .append("text")
      .attr("class", "ariadne-node-label")
      .attr("dy", "0.32em")
      .style("font", `${FONT_SIZE}px system-ui, sans-serif`)
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
      const kind = shapeFor(node);
      const hasIcon = this.hasIcon(node);

      if (isRowShape(kind)) {
        const iconGap = hasIcon ? ICON_SIZE + 6 : 0;
        const paddingY = BASE_PADDING_Y + this.extraPaddingFor(node);
        const bodyHeight = Math.max(MIN_BOX_HEIGHT, bbox.height + paddingY * 2);
        const tabHeight = kind === "folder" ? Math.min(bodyHeight * 0.4, 10 * NODE_SCALE) : 0;
        geomById.set(node.id, {
          width: PADDING_X * 2 + iconGap + bbox.width,
          height: bodyHeight + tabHeight,
          tabHeight,
          contentHeight: 0,
          radius: 0,
        });
        return;
      }

      // Figuras centradas y regulares: ícono arriba, texto abajo, todo
      // centrado. El tamaño (circunradio) sale del alto del contenido —
      // nunca de su ancho, que puede salirse de la figura sin problema — y
      // crece con el conteo de líneas del archivo (hasta el tope) para que
      // la diferencia de tamaño entre archivos sea visualmente obvia.
      const contentHeight = bbox.height + (hasIcon ? ICON_SIZE + ICON_TEXT_GAP : 0);
      const baseDiameter = Math.max(contentHeight + CENTERED_INNER_PAD_Y * 2, MIN_BOX_HEIGHT);
      const growth = 1 + this.sizeScaleT(node) * SHAPE_SIZE_GROWTH;
      const radius = (baseDiameter / 2) * growth * REGULAR_SHAPE_MARGIN;

      let shapeWidth: number;
      let shapeHeight: number;
      if (kind === "circle") {
        shapeWidth = shapeHeight = radius * 2;
      } else {
        const spec = REGULAR_SHAPE_SPEC[kind]!;
        const poly = regularPolygonExtent(spec.sides, radius, spec.rotationDeg);
        shapeWidth = poly.width;
        shapeHeight = poly.height;
      }

      // El texto puede salirse de la figura (a propósito), pero el espacio
      // que el layout reserva para el nodo (width/height, usado para
      // separar hermanos y niveles) sí tiene que ser al menos tan ancho
      // como la etiqueta — si no, un nodo vecino queda lo bastante cerca
      // como para taparle la parte que sobresale. La figura que se dibuja
      // (radius) no cambia, solo el espacio reservado alrededor.
      const width = Math.max(shapeWidth, bbox.width + OVERFLOW_RESERVE);
      const height = Math.max(shapeHeight, contentHeight);
      geomById.set(node.id, { width, height, tabHeight: 0, contentHeight, radius });
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

    // Paso 2: la forma (carpeta, libro, círculo, o un polígono regular),
    // insertada detrás del texto.
    nodeGroups.each((d, i, groups) => {
      const node = this.graphNode(d);
      const geom = geomById.get(node.id)!;
      const group = select(groups[i]);
      const kind = shapeFor(node);
      let shape: Selection<SVGGraphicsElement, unknown, null, undefined>;
      if (kind === "folder") {
        shape = group
          .insert("path", "text")
          .attr("d", folderShapePath(geom.width, geom.height, geom.tabHeight)) as unknown as Selection<SVGGraphicsElement, unknown, null, undefined>;
      } else if (kind === "book") {
        shape = group
          .insert("rect", "text")
          .attr("x", -geom.width / 2)
          .attr("y", -geom.height / 2)
          .attr("width", geom.width)
          .attr("height", geom.height)
          .attr("rx", 4 * NODE_SCALE) as unknown as Selection<SVGGraphicsElement, unknown, null, undefined>;
      } else if (kind === "circle") {
        shape = group
          .insert("ellipse", "text")
          .attr("rx", geom.radius)
          .attr("ry", geom.radius) as unknown as Selection<SVGGraphicsElement, unknown, null, undefined>;
      } else {
        const spec = REGULAR_SHAPE_SPEC[kind]!;
        shape = group
          .insert("polygon", "text")
          .attr("points", regularPolygonPoints(spec.sides, geom.radius, spec.rotationDeg)) as unknown as Selection<SVGGraphicsElement, unknown, null, undefined>;
      }
      shape
        .attr("class", "ariadne-node-box")
        .attr("fill", this.colorFor(node))
        .attr("stroke", this.config.html.background)
        .attr("stroke-width", 1.5 * NODE_SCALE);

      // El "lomo" del libro: un par de líneas verticales cerca del borde
      // izquierdo, para distinguir de un archivo/rectángulo cualquiera.
      if (kind === "book") {
        const spineStroke = contrastTextColor(this.colorFor(node));
        for (const frac of [0.16, 0.24]) {
          group
            .insert("line", "text")
            .attr("x1", -geom.width / 2 + geom.width * frac)
            .attr("x2", -geom.width / 2 + geom.width * frac)
            .attr("y1", -geom.height / 2 + 3 * NODE_SCALE)
            .attr("y2", geom.height / 2 - 3 * NODE_SCALE)
            .attr("stroke", spineStroke)
            .attr("stroke-width", 1.2 * NODE_SCALE)
            .attr("opacity", 0.45);
        }
      }
    });

    textSel
      .attr("x", (d) => {
        const node = this.graphNode(d);
        const geom = geomById.get(node.id)!;
        const kind = shapeFor(node);
        if (!isRowShape(kind)) return 0;
        const iconGap = this.hasIcon(node) ? ICON_SIZE + 6 : 0;
        return -geom.width / 2 + PADDING_X + iconGap;
      })
      .attr("text-anchor", (d) => (isRowShape(shapeFor(this.graphNode(d))) ? "start" : "middle"))
      .attr("y", (d) => {
        const node = this.graphNode(d);
        const geom = geomById.get(node.id)!;
        if (isRowShape(shapeFor(node))) return geom.tabHeight / 2;
        // Centrada: el bloque ícono+texto (contentHeight) va centrado en el
        // origen; el texto ocupa la parte de abajo del bloque, debajo del
        // ícono si lo hay.
        const top = -geom.contentHeight / 2;
        const iconBlock = this.hasIcon(node) ? ICON_SIZE + ICON_TEXT_GAP : 0;
        const textTop = top + iconBlock;
        const textHeight = geom.contentHeight - iconBlock;
        return textTop + textHeight / 2;
      })
      // El texto puede salirse de las figuras centradas (círculo, hexágono,
      // rombo, etc. — a propósito, para no forzar el tamaño de la figura a
      // la longitud del label) y terminar sobre el fondo de la página en
      // vez de sobre el color del nodo. `mix-blend-mode: difference` se
      // probó primero para invertir el color automáticamente, pero en SVG
      // cada `<g>` transformado arma su propio grupo de blending aislado:
      // el texto solo invierte contra su propia figura/ícono, y en cuanto
      // sale de ese grupo (fondo de página) deja de haber nada contra qué
      // invertir — se vuelve blanco liso, invisible sobre un tema claro.
      // En su lugar: color de texto fijo (el del tema, ya pensado para
      // contrastar con el fondo) más un halo — un stroke grueso del color
      // de fondo pintado detrás del fill — así el texto se lee igual de
      // bien sobre la figura que sobre la página, sin depender de blending.
      .attr("fill", this.config.html.text_color)
      .attr("stroke", this.config.html.background)
      .attr("stroke-width", TEXT_HALO_WIDTH)
      .style("paint-order", "stroke")
      .style("stroke-linejoin", "round");

    nodeGroups
      .filter((d) => this.hasIcon(this.graphNode(d)))
      .insert("use", "text")
      .attr("href", (d) => `#icon-${this.graphNode(d).metadata.icon_key}`)
      .attr("width", ICON_SIZE)
      .attr("height", ICON_SIZE)
      .attr("x", (d) => {
        const node = this.graphNode(d);
        const kind = shapeFor(node);
        if (isRowShape(kind)) return -geomById.get(node.id)!.width / 2 + PADDING_X - 2;
        return -ICON_SIZE / 2;
      })
      .attr("y", (d) => {
        const node = this.graphNode(d);
        const geom = geomById.get(node.id)!;
        if (isRowShape(shapeFor(node))) return geom.tabHeight / 2 - ICON_SIZE / 2;
        return -geom.contentHeight / 2;
      });

    linkLayer
      .selectAll("path")
      .data(links)
      .join("path")
      .attr("d", (link) => pathFor(link));

    if (this.showReferences && refEdges.length > 0) {
      const refLayer = this.viewport
        .insert("g", ".ariadne-nodes")
        .attr("class", "ariadne-ref-links")
        .attr("fill", "none")
        .attr("stroke", this.config.html.colors.default)
        .attr("stroke-width", 1.3)
        .attr("stroke-dasharray", "5 3")
        .attr("opacity", 0.65);

      refLayer
        .selectAll("path")
        .data(refEdges)
        .join("path")
        .attr("d", (edge) => this.refEdgePath(edge, positionById));
    }

    // Focus de nodo: resalta sus referencias directas (entrantes y
    // salientes) con flechas dirigidas, atenuando el resto del diagrama.
    if (this.focusedId) {
      const related = refEdges.filter((e) => e.source === this.focusedId || e.target === this.focusedId);
      const relatedIds = new Set<string>([this.focusedId]);
      for (const e of related) {
        relatedIds.add(e.source);
        relatedIds.add(e.target);
      }

      nodeGroups
        .classed("ariadne-dimmed", (d) => !relatedIds.has(this.graphNode(d).id))
        .classed("ariadne-focused", (d) => this.graphNode(d).id === this.focusedId);
      linkLayer.attr("opacity", 0.15);
      this.viewport.select(".ariadne-ref-links").attr("opacity", 0.15);

      const focusLayer = this.viewport
        .append("g")
        .attr("class", "ariadne-focus-links")
        .attr("fill", "none")
        .attr("stroke", this.config.html.colors.default)
        .attr("stroke-width", 2)
        .attr("marker-end", "url(#arrow-focus)");

      focusLayer
        .selectAll("path")
        .data(related)
        .join("path")
        .attr("d", (edge) => this.refEdgePath(edge, positionById));
    }

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

  private hasChildren(d: HierarchyNode<TreeNode>): boolean {
    return Boolean(d.data.children) || (d.data.children === undefined && Boolean(this.graphNode(d).metadata.child_count));
  }

  /** Qué tan "grande" es un archivo, en [0, 1] — raíz cuadrada del conteo de
   * líneas (con tope), para que el crecimiento visual sea proporcional al
   * ÁREA del archivo y no a su conteo de líneas directamente. Usado tanto
   * para el padding extra de cajas rectangulares como el crecimiento de
   * tamaño de las figuras centradas. */
  private sizeScaleT(node: GraphNode): number {
    if (node.node_type !== "file") return 0;
    const lines = node.metadata.line_count;
    if (!lines || lines <= 0) return 0;
    return Math.sqrt(Math.min(lines, LINE_COUNT_CAP) / LINE_COUNT_CAP);
  }

  /** Padding vertical extra para cajas rectangulares — así la caja "se
   * siente" más grande cuanto más código tiene el archivo, sin dejar de
   * contener el texto. */
  private extraPaddingFor(node: GraphNode): number {
    return this.sizeScaleT(node) * MAX_EXTRA_PADDING_Y;
  }

  private hasIcon(node: GraphNode): boolean {
    return this.config.html.icons.enabled && !!node.metadata.icon_key;
  }

  /** Camino recto entre los bordes de dos cajas (no las de árbol, esas usan
   * `pathFor` con la tangente compartida) — compartido entre la capa de
   * "todas las referencias" y la de focus, que solo cambia el estilo. */
  private refEdgePath(edge: RefEdge, positionById: Map<string, PositionedNode>): string {
    const source = positionById.get(edge.source);
    const target = positionById.get(edge.target);
    if (!source || !target) return "";
    const dir = normalizeVec({ x: target.center.x - source.center.x, y: target.center.y - source.center.y });
    const exitDist = boxExitDistance(source.geom, dir);
    const entryDist = boxExitDistance(target.geom, dir);
    const exit: Vec2 = { x: source.center.x + dir.x * exitDist, y: source.center.y + dir.y * exitDist };
    const entry: Vec2 = { x: target.center.x - dir.x * entryDist, y: target.center.y - dir.y * entryDist };
    return `M${exit.x},${exit.y} L${entry.x},${entry.y}`;
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
  const tabWidth = Math.min(width * 0.4, 40 * NODE_SCALE);
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

/** Puntos (para un `<polygon>`) de una figura centrada en el origen que
 * ocupa el `width`x`height` dado — hexágono (achatado arriba/abajo, puntas
 * a los lados), rombo, triángulo (apunta arriba), pentágono ("home plate",
 * punta arriba) y octágono (esquinas cortadas, el más parecido a un rect).
 * No son polígonos regulares: están ajustados para dejar espacio razonable
 * al contenido centrado (ícono+texto) que llevan encima, no para precisión
 * geométrica. */
/** Vértices de un polígono regular de `sides` lados, circunradio `radius`,
 * centrado en el origen — `rotationDeg` decide qué vértice queda arriba
 * (-90 = un vértice apunta arriba; 0 = arista horizontal arriba/abajo). */
function regularPolygonVertices(sides: number, radius: number, rotationDeg: number): Array<[number, number]> {
  const rotation = (rotationDeg * Math.PI) / 180;
  const step = (2 * Math.PI) / sides;
  const verts: Array<[number, number]> = [];
  for (let k = 0; k < sides; k++) {
    const angle = rotation + k * step;
    verts.push([radius * Math.cos(angle), radius * Math.sin(angle)]);
  }
  return verts;
}

function regularPolygonPoints(sides: number, radius: number, rotationDeg: number): string {
  return regularPolygonVertices(sides, radius, rotationDeg)
    .map(([x, y]) => `${x},${y}`)
    .join(" ");
}

/** Caja delimitadora real de un polígono regular — no siempre mide
 * 2*radius de alto/ancho (un triángulo apuntando arriba, por ejemplo, mide
 * 1.5*radio de alto), y el layout necesita el tamaño real para el
 * espaciado entre nodos. */
function regularPolygonExtent(sides: number, radius: number, rotationDeg: number): { width: number; height: number } {
  const verts = regularPolygonVertices(sides, radius, rotationDeg);
  let minX = Infinity;
  let maxX = -Infinity;
  let minY = Infinity;
  let maxY = -Infinity;
  for (const [x, y] of verts) {
    minX = Math.min(minX, x);
    maxX = Math.max(maxX, x);
    minY = Math.min(minY, y);
    maxY = Math.max(maxY, y);
  }
  return { width: maxX - minX, height: maxY - minY };
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

/** Vector unitario en la dirección de `v`; si `v` es (0,0) (dos nodos con el
 * mismo centro, caso degenerado raro) cae a apuntar a la derecha. */
function normalizeVec(v: Vec2): Vec2 {
  const len = Math.hypot(v.x, v.y);
  if (len === 0) return { x: 1, y: 0 };
  return { x: v.x / len, y: v.y / len };
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
