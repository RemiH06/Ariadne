pub struct TemplateInput<'a> {
    pub title: &'a str,
    pub background: &'a str,
    pub text_color: &'a str,
    pub link_color: &'a str,
    pub accent: &'a str,
    pub icon_defs: &'a str,
    pub graph_json: &'a str,
    pub config_json: &'a str,
    pub client_js: &'a str,
}

pub fn render(input: TemplateInput) -> String {
    let title = escape_html(input.title);
    let background = input.background;
    let text_color = input.text_color;
    let link_color = input.link_color;
    let accent = input.accent;
    let icon_defs = input.icon_defs;
    let graph_json = script_safe_json(input.graph_json);
    let config_json = script_safe_json(input.config_json);
    let client_js = input.client_js;

    // El ícono es una única flecha (apunta a la derecha) rotada por botón —
    // así las 8 orientaciones comparten un solo símbolo SVG y se ven
    // idénticas en cualquier navegador, sin depender de qué glifos de flecha
    // Unicode traiga instalada la fuente del sistema.
    let dir_btn_topleft = dir_button("from-topleft", 45, "Diagonal desde arriba-izquierda", false);
    let dir_btn_topbottom = dir_button("top-bottom", 90, "De arriba hacia abajo", false);
    let dir_btn_topright = dir_button("from-topright", 135, "Diagonal desde arriba-derecha", false);
    let dir_btn_leftright = dir_button("left-right", 0, "De izquierda a derecha", true);
    let dir_btn_rightleft = dir_button("right-left", 180, "De derecha a izquierda", false);
    let dir_btn_bottomleft = dir_button("from-bottomleft", 315, "Diagonal desde abajo-izquierda", false);
    let dir_btn_bottomtop = dir_button("bottom-top", 270, "De abajo hacia arriba", false);
    let dir_btn_bottomright = dir_button("from-bottomright", 225, "Diagonal desde abajo-derecha", false);
    let dir_btn_radial = radial_button();

    format!(
        r##"<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>{title}</title>
<style>
  :root {{
    --ariadne-bg: {background};
    --ariadne-text: {text_color};
    --ariadne-link: {link_color};
    --ariadne-accent: {accent};
  }}
  html, body {{
    margin: 0;
    height: 100%;
    background: var(--ariadne-bg);
    color: var(--ariadne-text);
    font-family: system-ui, sans-serif;
  }}
  #ariadne-app {{
    position: relative;
    width: 100%;
    height: 100vh;
    overflow: hidden;
  }}
  #ariadne-svg {{
    width: 100%;
    height: 100%;
    display: block;
  }}
  #ariadne-controls {{
    position: absolute;
    top: 12px;
    left: 12px;
    z-index: 10;
    background: var(--ariadne-bg);
    border: 1px solid var(--ariadne-link);
    border-radius: 10px;
    padding: 12px 14px;
    font-size: 13px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.18);
  }}
  #ariadne-controls .ariadne-title {{
    font-size: 14px;
    font-weight: 700;
  }}
  .ariadne-section {{
    display: flex;
    flex-direction: column;
    gap: 6px;
  }}
  .ariadne-section-title {{
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    opacity: 0.6;
  }}
  .ariadne-row {{
    display: flex;
    align-items: center;
    gap: 8px;
  }}
  .ariadne-btn {{
    background: transparent;
    color: var(--ariadne-text);
    border: 1px solid var(--ariadne-link);
    border-radius: 6px;
    padding: 4px 9px;
    font: inherit;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }}
  .ariadne-btn:hover {{
    border-color: var(--ariadne-accent);
  }}
  .ariadne-btn.active {{
    background: var(--ariadne-accent);
    border-color: var(--ariadne-accent);
    color: var(--ariadne-bg);
    font-weight: 600;
  }}
  .ariadne-compass {{
    display: grid;
    grid-template-columns: repeat(3, 28px);
    grid-auto-rows: 28px;
    gap: 4px;
  }}
  .ariadne-compass button {{
    width: 28px;
    height: 28px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }}
  .ariadne-dir-icon {{
    width: 15px;
    height: 15px;
    color: inherit;
  }}
  #ariadne-controls label {{
    display: flex;
    align-items: center;
    gap: 6px;
  }}
  #ariadne-controls input[type="number"],
  #ariadne-controls input[type="text"] {{
    width: 90px;
    background: transparent;
    color: var(--ariadne-text);
    border: 1px solid var(--ariadne-link);
    border-radius: 4px;
    padding: 2px 6px;
    font: inherit;
  }}
  .ariadne-node-label {{
    pointer-events: none;
    user-select: none;
  }}
</style>
</head>
<body>
<div id="ariadne-app">
  <svg id="ariadne-svg">
    <defs>
{icon_defs}
    </defs>
  </svg>
  <svg width="0" height="0" style="position:absolute" aria-hidden="true">
    <symbol id="icon-dir-arrow" viewBox="0 0 24 24">
      <path d="M3 12h15m0 0l-5-5m5 5l-5 5" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>
    </symbol>
    <symbol id="icon-radial" viewBox="0 0 24 24">
      <circle cx="12" cy="12" r="2.6" fill="currentColor"/>
      <circle cx="12" cy="12" r="7" fill="none" stroke="currentColor" stroke-width="1.6"/>
      <circle cx="12" cy="12" r="11" fill="none" stroke="currentColor" stroke-width="1.3" opacity="0.55"/>
    </symbol>
    <symbol id="icon-fit-screen" viewBox="0 0 24 24">
      <path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
    </symbol>
  </svg>
  <div id="ariadne-controls">
    <span class="ariadne-title">{title}</span>

    <div class="ariadne-section">
      <span class="ariadne-section-title">Orientación</span>
      <div class="ariadne-row">
        <div class="ariadne-compass" role="group" aria-label="Orientación del árbol">
          {dir_btn_topleft}
          {dir_btn_topbottom}
          {dir_btn_topright}
          {dir_btn_leftright}
          {dir_btn_radial}
          {dir_btn_rightleft}
          {dir_btn_bottomleft}
          {dir_btn_bottomtop}
          {dir_btn_bottomright}
        </div>
        <button id="ariadne-fit-btn" class="ariadne-btn" title="Ajustar todo el diagrama a la pantalla">
          <svg class="ariadne-dir-icon" viewBox="0 0 24 24"><use href="#icon-fit-screen"/></svg>
          Ajustar
        </button>
      </div>
    </div>

    <div class="ariadne-section">
      <span class="ariadne-section-title">Filtros</span>
      <label><input type="checkbox" id="ariadne-filter-hide-generated" /> Ocultar generados</label>
      <label>Profundidad máx. <input type="number" id="ariadne-filter-max-depth" min="0" /> <span style="opacity:.65">(total: <strong id="ariadne-depth-indicator">–</strong>)</span></label>
      <label>Ocultar extensiones <input type="text" id="ariadne-filter-hide-ext" placeholder=".lock,.min.js" /></label>
    </div>
  </div>
</div>
<script type="application/json" id="ariadne-graph-data">{graph_json}</script>
<script type="application/json" id="ariadne-render-config">{config_json}</script>
<script>{client_js}</script>
</body>
</html>
"##
    )
}

fn dir_button(direction: &str, angle: u16, title: &str, active: bool) -> String {
    let active_class = if active { " active" } else { "" };
    format!(
        r##"<button data-direction="{direction}" title="{title}" class="ariadne-btn{active_class}"><svg class="ariadne-dir-icon" viewBox="0 0 24 24"><use href="#icon-dir-arrow" transform="rotate({angle} 12 12)"/></svg></button>"##
    )
}

fn radial_button() -> String {
    r##"<button data-direction="radial" title="Radial: la raíz al centro, un anillo por nivel" class="ariadne-btn"><svg class="ariadne-dir-icon" viewBox="0 0 24 24"><use href="#icon-radial"/></svg></button>"##.to_string()
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Evita que un `</script` embebido dentro del JSON (ej. en un path de
/// archivo) cierre prematuramente el bloque `<script>` que lo contiene.
fn script_safe_json(json: &str) -> String {
    json.replace("</", "<\\/")
}
