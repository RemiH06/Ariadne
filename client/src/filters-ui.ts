import type { FilterState } from "./data.js";
import type { FilterDefaults } from "./types.js";

export interface FilterControls {
  hideGeneratedInput: HTMLInputElement;
  maxDepthInput: HTMLInputElement;
  hideExtInput: HTMLInputElement;
  onlyExtInput: HTMLInputElement;
  hideMembersInput: HTMLInputElement;
}

function parseExtList(raw: string): Set<string> {
  return new Set(
    raw
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0)
  );
}

/** Inicializa los controles de filtro con los defaults del RenderConfig y
 * conecta sus eventos `change` a `onChange`. Devuelve el FilterState inicial. */
export function initFilterControls(
  controls: FilterControls,
  defaults: FilterDefaults,
  onChange: (state: FilterState) => void
): FilterState {
  controls.hideGeneratedInput.checked = defaults.hide_generated;
  controls.maxDepthInput.value = defaults.max_depth != null ? String(defaults.max_depth) : "";
  controls.hideExtInput.value = defaults.hide_extensions.join(",");
  controls.onlyExtInput.value = "";
  controls.hideMembersInput.checked = false;

  const readState = (): FilterState => {
    const maxDepthRaw = controls.maxDepthInput.value.trim();
    return {
      hideGenerated: controls.hideGeneratedInput.checked,
      maxDepth: maxDepthRaw === "" ? null : Number(maxDepthRaw),
      hideExtensions: parseExtList(controls.hideExtInput.value.trim()),
      onlyExtensions: parseExtList(controls.onlyExtInput.value.trim()),
      hideMembers: controls.hideMembersInput.checked,
    };
  };

  const emit = () => onChange(readState());
  controls.hideGeneratedInput.addEventListener("change", emit);
  controls.maxDepthInput.addEventListener("change", emit);
  controls.hideExtInput.addEventListener("change", emit);
  controls.onlyExtInput.addEventListener("change", emit);
  controls.hideMembersInput.addEventListener("change", emit);

  return readState();
}
