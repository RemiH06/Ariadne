import { buildFilteredTree, collectVisibleIds, getVisibleReferenceEdges, readEmbeddedJson, type FilterState } from "./data.js";
import { initFilterControls } from "./filters-ui.js";
import { LAYOUT_MODES, type LayoutMode } from "./layout.js";
import { DiagramRenderer } from "./render.js";
import type { Graph, RenderConfig } from "./types.js";

function main(): void {
  const graph = readEmbeddedJson<Graph>("ariadne-graph-data");
  const config = readEmbeddedJson<RenderConfig>("ariadne-render-config");

  const svgEl = document.getElementById("ariadne-svg") as SVGSVGElement | null;
  if (!svgEl) throw new Error("no se encontró #ariadne-svg");

  const collapsed = new Set<string>();
  let filterState: FilterState = {
    hideGenerated: config.filters.hide_generated,
    maxDepth: config.filters.max_depth ?? null,
    hideExtensions: new Set(config.filters.hide_extensions),
  };

  const renderer = new DiagramRenderer(svgEl, config, {
    onToggleCollapse: (nodeId) => {
      if (collapsed.has(nodeId)) collapsed.delete(nodeId);
      else collapsed.add(nodeId);
      rerender();
    },
  });

  let hasRenderedOnce = false;
  function rerender(): void {
    const filteredTree = buildFilteredTree(graph, filterState, collapsed);
    if (!filteredTree) return;
    const visibleIds = collectVisibleIds(filteredTree);
    const refEdges = getVisibleReferenceEdges(graph, visibleIds);
    renderer.render(filteredTree, refEdges, { refit: !hasRenderedOnce });
    hasRenderedOnce = true;
  }

  const hideGeneratedInput = document.getElementById("ariadne-filter-hide-generated") as HTMLInputElement | null;
  const maxDepthInput = document.getElementById("ariadne-filter-max-depth") as HTMLInputElement | null;
  const hideExtInput = document.getElementById("ariadne-filter-hide-ext") as HTMLInputElement | null;

  if (hideGeneratedInput && maxDepthInput && hideExtInput) {
    filterState = initFilterControls(
      { hideGeneratedInput, maxDepthInput, hideExtInput },
      config.filters,
      (state) => {
        filterState = state;
        rerender();
      }
    );
  }

  const totalDepth = graph.nodes.reduce((max, n) => Math.max(max, n.depth), 0);
  const depthIndicator = document.getElementById("ariadne-depth-indicator");
  if (depthIndicator) depthIndicator.textContent = String(totalDepth);

  const fitButton = document.getElementById("ariadne-fit-btn");
  fitButton?.addEventListener("click", () => renderer.fit());

  const showRefsInput = document.getElementById("ariadne-filter-show-refs") as HTMLInputElement | null;
  showRefsInput?.addEventListener("change", () => {
    renderer.setShowReferences(showRefsInput.checked);
  });

  const directionButtons = document.querySelectorAll<HTMLButtonElement>("[data-direction]");
  const setActiveDirectionButton = (direction: LayoutMode) => {
    directionButtons.forEach((btn) => {
      btn.classList.toggle("active", btn.dataset.direction === direction);
    });
  };
  directionButtons.forEach((btn) => {
    const direction = btn.dataset.direction as LayoutMode | undefined;
    if (!direction || !LAYOUT_MODES.includes(direction)) return;
    btn.addEventListener("click", () => {
      renderer.setDirection(direction);
      setActiveDirectionButton(direction);
    });
  });
  setActiveDirectionButton(renderer.getDirection());

  rerender();
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", main);
} else {
  main();
}
