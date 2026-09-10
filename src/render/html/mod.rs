mod template;

use crate::config::{FilterConfig, HtmlConfig};
use crate::render::docs::DocPage;
use crate::schema::Graph;
use anyhow::Result;
use serde::Serialize;

const CLIENT_BUNDLE: &str = include_str!("../../assets/client-bundle.js");
const ICON_SPRITE: &str = include_str!("../../assets/icons-sprite.svg");
const LOGO_SVG: &str = include_str!("../../assets/logo.svg");

/// Espejo de `RenderConfig` en `client/src/types.ts`. `HtmlConfig` y
/// `FilterConfig` ya coinciden campo a campo con lo que el cliente espera,
/// así que se serializan directamente sin duplicar la definición.
#[derive(Serialize)]
struct ClientRenderConfig<'a> {
    title: &'a str,
    html: &'a HtmlConfig,
    filters: &'a FilterConfig,
}

pub fn render_html(
    graph: &Graph,
    title: &str,
    html_cfg: &HtmlConfig,
    filters: &FilterConfig,
    doc_pages: &[DocPage],
) -> Result<String> {
    let graph_json = serde_json::to_string(graph)?;
    let render_config = ClientRenderConfig {
        title,
        html: html_cfg,
        filters,
    };
    let config_json = serde_json::to_string(&render_config)?;

    let icon_defs = if html_cfg.icons.enabled { ICON_SPRITE } else { "" };
    // `#` es el único carácter del SVG que un data URI no tolera sin
    // codificar (se confunde con un fragmento de URL) — todo lo demás
    // (comillas, espacios, `<`/`>`) los navegadores lo aceptan literal.
    let favicon_data_uri = LOGO_SVG.replace('#', "%23");

    Ok(template::render(template::TemplateInput {
        title,
        background: &html_cfg.background,
        text_color: &html_cfg.text_color,
        link_color: &html_cfg.link_color,
        accent: &html_cfg.colors.default,
        colors: &html_cfg.colors,
        favicon_data_uri: &favicon_data_uri,
        icon_defs,
        graph_json: &graph_json,
        config_json: &config_json,
        client_js: CLIENT_BUNDLE,
        doc_pages,
    }))
}
