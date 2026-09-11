import { buildFilteredTree, collectVisibleIds, getVisibleReferenceEdges, readEmbeddedJson, type FilterState } from "./data.js";
import { initFilterControls } from "./filters-ui.js";
import { LAYOUT_MODES, type LayoutMode } from "./layout.js";
import { DiagramRenderer, formatRelativeDate } from "./render.js";
import type { Graph, GraphNode, RenderConfig } from "./types.js";

const NO_SELECTION_LABEL = "Haz clic en un nodo para ver su información.";

/** Actualiza el contenido del panel de "Nodo seleccionado" (su propio panel
 * a la derecha, con pestaña independiente, no se muestra ni se oculta según
 * el focus, eso lo controla el usuario con la pestaña) y arma la lista de
 * "ver historial" (vacía hasta el primer clic, después queda cacheada en
 * el DOM). */
function updateSelectionPanel(node: GraphNode | null): void {
  const label = document.getElementById("ariadne-selection-label");
  const gotoDocsLink = document.getElementById("ariadne-goto-docs-btn") as HTMLAnchorElement | null;
  const historyBtn = document.getElementById("ariadne-history-btn") as HTMLButtonElement | null;
  const historyList = document.getElementById("ariadne-history-list");
  if (!label || !gotoDocsLink || !historyBtn || !historyList) return;

  historyList.hidden = true;
  historyList.replaceChildren();

  if (!node) {
    label.textContent = NO_SELECTION_LABEL;
    gotoDocsLink.hidden = true;
    historyBtn.hidden = true;
    return;
  }

  label.textContent = node.label;

  // "Ir a documentación" es un link normal — Ariadne no renderiza ni
  // embebe nada de esa página, solo redirige (se abre en una pestaña
  // nueva para no perder el estado del diagrama).
  const docUrl = node.metadata.doc_url;
  gotoDocsLink.hidden = !docUrl;
  if (docUrl) gotoDocsLink.href = docUrl;

  const commits = node.metadata.recent_commits ?? [];
  historyBtn.hidden = commits.length === 0;
  historyBtn.onclick = () => {
    historyList.hidden = !historyList.hidden;
    if (!historyList.hidden && historyList.childElementCount === 0) {
      for (const commit of commits) {
        const row = document.createElement("div");
        row.className = "ariadne-history-row";
        const meta = document.createElement("div");
        meta.className = "ariadne-history-meta";
        meta.textContent = `${commit.author} · ${formatRelativeDate(commit.timestamp)} · ${commit.short_hash}`;
        const subject = document.createElement("div");
        subject.className = "ariadne-history-subject";
        subject.textContent = commit.subject;
        row.append(meta, subject);
        historyList.appendChild(row);
      }
    }
  };
}

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
    onlyExtensions: new Set(),
    hideMembers: false,
  };

  const renderer = new DiagramRenderer(svgEl, config, {
    onToggleCollapse: (nodeId) => {
      if (collapsed.has(nodeId)) collapsed.delete(nodeId);
      else collapsed.add(nodeId);
      rerender();
    },
    onFocusChange: updateSelectionPanel,
  });
  renderer.setGraph(graph);

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
  const onlyExtInput = document.getElementById("ariadne-filter-only-ext") as HTMLInputElement | null;
  const hideMembersInput = document.getElementById("ariadne-filter-hide-members") as HTMLInputElement | null;

  if (hideGeneratedInput && maxDepthInput && hideExtInput && onlyExtInput && hideMembersInput) {
    filterState = initFilterControls(
      { hideGeneratedInput, maxDepthInput, hideExtInput, onlyExtInput, hideMembersInput },
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

  const controlsPanel = document.getElementById("ariadne-controls");
  const controlsToggle = document.getElementById("ariadne-controls-toggle");
  controlsToggle?.addEventListener("click", () => {
    const collapsed = controlsPanel?.classList.toggle("ariadne-collapsed");
    controlsToggle.textContent = collapsed ? "▸" : "◂";
  });

  const selectionPanel = document.getElementById("ariadne-selection-panel");
  const selectionToggle = document.getElementById("ariadne-selection-toggle");
  selectionToggle?.addEventListener("click", () => {
    const collapsed = selectionPanel?.classList.toggle("ariadne-collapsed");
    selectionToggle.textContent = collapsed ? "◂" : "▸";
  });

  const showRefsInput = document.getElementById("ariadne-filter-show-refs") as HTMLInputElement | null;
  showRefsInput?.addEventListener("change", () => {
    renderer.setShowReferences(showRefsInput.checked);
  });

  const colorByAgeInput = document.getElementById("ariadne-filter-color-by-age") as HTMLInputElement | null;
  colorByAgeInput?.addEventListener("change", () => {
    renderer.setColorByAge(colorByAgeInput.checked);
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
