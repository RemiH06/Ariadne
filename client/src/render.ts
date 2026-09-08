import { hierarchy, tree, type HierarchyPointNode } from "d3-hierarchy";
import { select, type Selection } from "d3-selection";
import { linkHorizontal } from "d3-shape";
import { zoom, zoomIdentity, type D3ZoomEvent } from "d3-zoom";
import type { GraphNode, RenderConfig, TreeNode } from "./types.js";

const DEFAULT_RADIUS = 5; // raíz / directorios
const MIN_FILE_RADIUS = 4;
const MAX_FILE_RADIUS = 14;
const LINE_COUNT_CAP = 2000; // debe coincidir con extractor::classify::LINE_COUNT_CAP
const SIBLING_GAP = 32;
const LEVEL_WIDTH = 160;

export interface RenderCallbacks {
  onToggleCollapse: (nodeId: string) => void;
}

export class DiagramRenderer {
  private svg: Selection<SVGSVGElement, unknown, null, undefined>;
  private viewport: Selection<SVGGElement, unknown, null, undefined>;
  private readonly config: RenderConfig;
  private readonly callbacks: RenderCallbacks;
  private readonly zoomBehavior = zoom<SVGSVGElement, unknown>().scaleExtent([0.05, 8]);
  private hasFitted = false;

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

  render(rootTree: TreeNode): void {
    const rootHierarchy = hierarchy<TreeNode>(rootTree, (d) => d.children);
    const layout = tree<TreeNode>().nodeSize([SIBLING_GAP, LEVEL_WIDTH]);
    const laidOut = layout(rootHierarchy);
    const nodes = laidOut.descendants();
    const links = laidOut.links();

    this.viewport.selectAll("*").remove();

    const linkGen = linkHorizontal<unknown, HierarchyPointNode<TreeNode>>()
      .x((d) => d.y)
      .y((d) => d.x);

    this.viewport
      .append("g")
      .attr("class", "ariadne-links")
      .attr("fill", "none")
      .attr("stroke", this.config.html.link_color)
      .attr("stroke-width", 1.5)
      .selectAll("path")
      .data(links)
      .join("path")
      .attr("d", (d) => linkGen(d as never));

    const nodeGroups = this.viewport
      .append("g")
      .attr("class", "ariadne-nodes")
      .selectAll<SVGGElement, HierarchyPointNode<TreeNode>>("g")
      .data(nodes)
      .join("g")
      .attr("class", "ariadne-node")
      .attr("transform", (d) => `translate(${d.y},${d.x})`)
      .style("cursor", (d) => (d.data.children || (d.data.children === undefined && this.graphNode(d).metadata.child_count) ? "pointer" : "default"))
      .on("click", (_event: MouseEvent, d: HierarchyPointNode<TreeNode>) => {
        this.callbacks.onToggleCollapse(this.graphNode(d).id);
      });

    nodeGroups
      .append("circle")
      .attr("r", (d) => this.radiusFor(this.graphNode(d)))
      .attr("fill", (d) => this.colorFor(this.graphNode(d).node_type))
      .attr("stroke", this.config.html.background)
      .attr("stroke-width", 1.5);

    nodeGroups
      .filter((d) => this.hasIcon(this.graphNode(d)))
      .append("use")
      .attr("href", (d) => `#icon-${this.graphNode(d).metadata.icon_key}`)
      .attr("width", 14)
      .attr("height", 14)
      .attr("x", (d) => this.radiusFor(this.graphNode(d)) + 3)
      .attr("y", -7);

    nodeGroups
      .append("text")
      .attr("class", "ariadne-node-label")
      .attr("x", (d) => {
        const r = this.radiusFor(this.graphNode(d));
        return this.hasIcon(this.graphNode(d)) ? r + 21 : r + 5;
      })
      .attr("dy", "0.32em")
      .attr("fill", this.config.html.text_color)
      .style("font", "12px system-ui, sans-serif")
      .text((d) => this.graphNode(d).label);

    if (!this.hasFitted) {
      this.fitToViewport(nodes);
      this.hasFitted = true;
    }
  }

  private graphNode(d: HierarchyPointNode<TreeNode>): GraphNode {
    return d.data.data;
  }

  /** Radio del nodo: fijo para raíz/directorios, proporcional al área
   * (raíz cuadrada del conteo de líneas) para archivos — así el tamaño
   * *percibido* crece con las líneas, no solo el radio crudo. */
  private radiusFor(node: GraphNode): number {
    if (node.node_type !== "file") return DEFAULT_RADIUS;
    const lines = node.metadata.line_count;
    if (!lines || lines <= 0) return MIN_FILE_RADIUS;
    const t = Math.sqrt(Math.min(lines, LINE_COUNT_CAP) / LINE_COUNT_CAP);
    return MIN_FILE_RADIUS + t * (MAX_FILE_RADIUS - MIN_FILE_RADIUS);
  }

  private hasIcon(node: GraphNode): boolean {
    return this.config.html.icons.enabled && !!node.metadata.icon_key;
  }

  private colorFor(nodeType: GraphNode["node_type"]): string {
    const colors = this.config.html.colors as unknown as Record<string, string>;
    return colors[nodeType] ?? colors.default;
  }

  private fitToViewport(nodes: HierarchyPointNode<TreeNode>[]): void {
    if (nodes.length === 0) return;
    const svgNode = this.svg.node();
    if (!svgNode) return;

    const width = svgNode.clientWidth || 800;
    const height = svgNode.clientHeight || 600;

    let minX = Infinity;
    let maxX = -Infinity;
    let minY = Infinity;
    let maxY = -Infinity;
    for (const n of nodes) {
      if (n.x < minX) minX = n.x;
      if (n.x > maxX) maxX = n.x;
      if (n.y < minY) minY = n.y;
      if (n.y > maxY) maxY = n.y;
    }

    const contentWidth = maxY - minY + 220;
    const contentHeight = maxX - minX + 80;
    const scale = Math.min(1, 0.9 * Math.min(width / contentWidth, height / contentHeight));
    const tx = width / 2 - ((minY + maxY) / 2) * scale;
    const ty = height / 2 - ((minX + maxX) / 2) * scale;

    this.svg.call(this.zoomBehavior.transform, zoomIdentity.translate(tx, ty).scale(scale));
  }
}
