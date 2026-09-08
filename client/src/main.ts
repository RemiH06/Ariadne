import { buildFilteredTree, readEmbeddedJson, type FilterState } from "./data.js";
import { initFilterControls } from "./filters-ui.js";
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

  function rerender(): void {
    const tree = buildFilteredTree(graph, filterState, collapsed);
    if (tree) renderer.render(tree);
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

  rerender();
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", main);
} else {
  main();
}
