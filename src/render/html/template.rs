use crate::config::ColorsConfig;

pub struct TemplateInput<'a> {
    pub title: &'a str,
    pub background: &'a str,
    pub text_color: &'a str,
    pub link_color: &'a str,
    pub accent: &'a str,
    pub colors: &'a ColorsConfig,
    pub favicon_data_uri: &'a str,
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
    let favicon_data_uri = input.favicon_data_uri;
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
    let legend = legend_html(input.colors);

    format!(
        r##"<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>{title}</title>
<link rel="icon" type="image/svg+xml" href='data:image/svg+xml,{favicon_data_uri}' />
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
  #ariadne-controls-toggle {{
    position: absolute;
    top: 12px;
    left: 12px;
    z-index: 11;
    width: 30px;
    height: 30px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--ariadne-bg);
    color: var(--ariadne-text);
    border: 1px solid var(--ariadne-link);
    border-radius: 8px;
    font-size: 14px;
    line-height: 1;
    cursor: pointer;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.18);
    transition: border-color 0.15s ease;
  }}
  #ariadne-controls-toggle:hover {{
    border-color: var(--ariadne-accent);
  }}
  #ariadne-controls {{
    position: absolute;
    top: 12px;
    left: 54px;
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
    max-height: calc(100vh - 24px);
    overflow-y: auto;
    transition: opacity 0.15s ease, transform 0.15s ease;
  }}
  #ariadne-controls.ariadne-collapsed {{
    opacity: 0;
    transform: translateX(-8px);
    pointer-events: none;
  }}
  .ariadne-legend {{
    display: flex;
    gap: 18px;
  }}
  .ariadne-legend-group {{
    display: flex;
    flex-direction: column;
    gap: 4px;
  }}
  .ariadne-legend-subtitle {{
    font-size: 10px;
    opacity: 0.55;
  }}
  .ariadne-legend-row {{
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
  }}
  .ariadne-legend-shape, .ariadne-legend-swatch {{
    width: 13px;
    height: 13px;
    flex: 0 0 auto;
    background: var(--ariadne-link);
  }}
  .ariadne-legend-swatch {{
    border-radius: 3px;
  }}
  .ariadne-legend-circle {{ border-radius: 50%; }}
  .ariadne-legend-diamond {{ clip-path: polygon(50% 0%, 100% 50%, 50% 100%, 0% 50%); }}
  .ariadne-legend-triangle {{ clip-path: polygon(50% 0%, 100% 100%, 0% 100%); }}
  .ariadne-legend-pentagon {{ clip-path: polygon(50% 0%, 100% 38%, 82% 100%, 18% 100%, 0% 38%); }}
  .ariadne-legend-octagon {{ clip-path: polygon(30% 0%, 70% 0%, 100% 30%, 100% 70%, 70% 100%, 30% 100%, 0% 70%, 0% 30%); }}
  .ariadne-legend-hexagon {{ clip-path: polygon(25% 0%, 75% 0%, 100% 50%, 75% 100%, 25% 100%, 0% 50%); }}
  .ariadne-legend-folder {{ clip-path: polygon(0% 20%, 42% 20%, 52% 2%, 100% 2%, 100% 100%, 0% 100%); }}
  .ariadne-legend-book {{ position: relative; border-radius: 2px; }}
  .ariadne-legend-book::after {{
    content: "";
    position: absolute;
    left: 32%;
    top: 15%;
    bottom: 15%;
    width: 1.5px;
    background: rgba(0, 0, 0, 0.4);
    box-shadow: 3px 0 0 rgba(0, 0, 0, 0.4);
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
  .ariadne-links path, .ariadne-ref-links path {{
    transition: opacity 0.15s ease;
  }}
  .ariadne-node.ariadne-focused .ariadne-node-box {{
    filter: url(#focus-glow);
  }}
  [hidden] {{
    display: none !important;
  }}
  .ariadne-selection-label {{
    font-weight: 600;
    word-break: break-word;
  }}
  .ariadne-history-list {{
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 220px;
    overflow-y: auto;
  }}
  .ariadne-history-row {{
    display: flex;
    flex-direction: column;
    gap: 1px;
    border-left: 2px solid var(--ariadne-link);
    padding-left: 7px;
  }}
  .ariadne-history-meta {{
    font-size: 10px;
    opacity: 0.6;
  }}
  .ariadne-history-subject {{
    font-size: 12px;
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
    <symbol id="icon-folder-test" viewBox="0 0 24 24">
      <path d="M5 13l4 4L19 7" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>
    </symbol>
    <symbol id="icon-folder-config" viewBox="0 0 24 24">
      <path d="M4 7h6M16 7h4M4 12h1M7 12h13M4 17h6M16 17h4" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
      <circle cx="11" cy="7" r="2.3" fill="currentColor"/>
      <circle cx="4.3" cy="12" r="2.3" fill="currentColor"/>
      <circle cx="11" cy="17" r="2.3" fill="currentColor"/>
    </symbol>
    <symbol id="icon-folder-src" viewBox="0 0 24 24">
      <path d="M9 8l-5 4 5 4M15 8l5 4-5 4" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
    </symbol>
    <symbol id="icon-folder-docs" viewBox="0 0 24 24">
      <path d="M7 3h7l4 4v14H7z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"/>
      <path d="M14 3v4h4M9 12h6M9 16h6" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>
    </symbol>
    <marker id="arrow-focus" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
      <path d="M0,0 L10,5 L0,10 z" fill="{accent}"/>
    </marker>
    <filter id="focus-glow" x="-120%" y="-120%" width="340%" height="340%">
      <feGaussianBlur in="SourceGraphic" stdDeviation="7" result="blur"/>
      <feColorMatrix in="blur" type="matrix" values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 6 0" result="halo"/>
      <feMerge>
        <feMergeNode in="halo"/>
        <feMergeNode in="halo"/>
        <feMergeNode in="SourceGraphic"/>
      </feMerge>
    </filter>
  </svg>
  <button id="ariadne-controls-toggle" title="Mostrar/ocultar panel">◂</button>
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
      <label>Mostrar solo extensiones <input type="text" id="ariadne-filter-only-ext" placeholder=".ts,.rs" /></label>
      <label><input type="checkbox" id="ariadne-filter-hide-members" /> Ocultar clases/métodos/atributos</label>
      <label><input type="checkbox" id="ariadne-filter-show-refs" /> Mostrar referencias entre archivos</label>
      <label><input type="checkbox" id="ariadne-filter-color-by-age" /> Colorear por antigüedad (más claro = más viejo)</label>
    </div>

    <div class="ariadne-section" id="ariadne-selection-section" hidden>
      <span class="ariadne-section-title">Nodo seleccionado</span>
      <div id="ariadne-selection-label" class="ariadne-selection-label"></div>
      <button id="ariadne-history-btn" class="ariadne-btn" hidden>Ver historial</button>
      <div id="ariadne-history-list" class="ariadne-history-list" hidden></div>
    </div>

{legend}
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

/// Leyenda de formas (qué tipo/formato de nodo es cada silueta) y colores
/// (qué categoría de archivo representa cada uno) — las dos son ejes
/// independientes (ver `client/src/render.ts`: `shapeFor`/`colorFor`), así
/// que se muestran como dos grupos separados. Se arma en el servidor
/// (no en el cliente) porque los colores configurados ya se conocen acá,
/// sin duplicar la lógica de dibujo de figuras del cliente.
fn legend_html(colors: &ColorsConfig) -> String {
    format!(
        r##"    <div class="ariadne-section">
      <span class="ariadne-section-title">Leyenda</span>
      <div class="ariadne-legend">
        <div class="ariadne-legend-group">
          <span class="ariadne-legend-subtitle">Forma</span>
          <div class="ariadne-legend-row"><span class="ariadne-legend-shape ariadne-legend-folder" style="background:{directory}"></span> Carpeta</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-shape ariadne-legend-book" style="background:{library}"></span> Librería</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-shape ariadne-legend-circle"></span> Archivo</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-shape ariadne-legend-diamond"></span> Datos</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-shape ariadne-legend-triangle"></span> Imagen</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-shape ariadne-legend-pentagon"></span> Texto plano</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-shape ariadne-legend-octagon"></span> Markup/docs</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-shape ariadne-legend-hexagon" style="background:{class}"></span> Clase</div>
        </div>
        <div class="ariadne-legend-group">
          <span class="ariadne-legend-subtitle">Color</span>
          <div class="ariadne-legend-row"><span class="ariadne-legend-swatch" style="background:{file}"></span> Código fuente</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-swatch" style="background:{directory}"></span> Carpeta</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-swatch" style="background:{library}"></span> Librería</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-swatch" style="background:{test}"></span> Test</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-swatch" style="background:{config}"></span> Config</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-swatch" style="background:{docs}"></span> Docs</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-swatch" style="background:{styles}"></span> Estilos</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-swatch" style="background:{markup}"></span> Markup</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-swatch" style="background:{script}"></span> Script</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-swatch" style="background:{class}"></span> Clase</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-swatch" style="background:{method}"></span> Método</div>
          <div class="ariadne-legend-row"><span class="ariadne-legend-swatch" style="background:{attribute}"></span> Atributo</div>
        </div>
      </div>
    </div>"##,
        file = colors.file,
        directory = colors.directory,
        library = colors.library,
        test = colors.test,
        config = colors.config,
        docs = colors.docs,
        styles = colors.styles,
        markup = colors.markup,
        script = colors.script,
        class = colors.class,
        method = colors.method,
        attribute = colors.attribute,
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
