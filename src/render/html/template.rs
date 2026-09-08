pub struct TemplateInput<'a> {
    pub title: &'a str,
    pub background: &'a str,
    pub text_color: &'a str,
    pub link_color: &'a str,
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
    let icon_defs = input.icon_defs;
    let graph_json = script_safe_json(input.graph_json);
    let config_json = script_safe_json(input.config_json);
    let client_js = input.client_js;

    format!(
        r#"<!doctype html>
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
    border-radius: 8px;
    padding: 10px 12px;
    font-size: 13px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    opacity: 0.95;
  }}
  #ariadne-controls label {{
    display: flex;
    align-items: center;
    gap: 6px;
  }}
  #ariadne-controls input[type="number"],
  #ariadne-controls input[type="text"] {{
    width: 90px;
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
  <div id="ariadne-controls">
    <strong>{title}</strong>
    <label><input type="checkbox" id="ariadne-filter-hide-generated" /> Ocultar generados</label>
    <label>Profundidad máx. <input type="number" id="ariadne-filter-max-depth" min="0" /></label>
    <label>Ocultar extensiones <input type="text" id="ariadne-filter-hide-ext" placeholder=".lock,.min.js" /></label>
  </div>
</div>
<script type="application/json" id="ariadne-graph-data">{graph_json}</script>
<script type="application/json" id="ariadne-render-config">{config_json}</script>
<script>{client_js}</script>
</body>
</html>
"#
    )
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
