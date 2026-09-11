---
name: ariadne
description: Genera un diagrama HTML interactivo y documentación (PDF/LaTeX/RTF/XML) de la estructura de un proyecto con la herramienta Ariadne. Usar cuando el usuario pida explorar, documentar o entender la estructura de un proyecto, ver dependencias entre archivos, o generar un mapa visual del código. El HTML generado también sirve como fuente de datos estructurados para el propio agente (ver más abajo).
---

# Ariadne

Ariadne analiza un proyecto de código (heurística de texto, 15 lenguajes:
JS/TS, Python, Rust, Go, Java, Kotlin, PHP, Ruby, C/C++, Elixir, Haskell,
Julia, R) y genera un diagrama HTML interactivo (árbol de archivos,
dependencias `depends_on`, clases/métodos/atributos, autor/fecha de git)
más documentación exportable a PDF/LaTeX/RTF/XML.

## Invocación

```bash
ariadne generate <ruta-al-proyecto> --out <dir-salida> --title "Nombre"
```

Flags útiles: `--formats html,pdf,xml,rtf,latex` (default: lo que diga
`conf.ariadne`, o `html`), `--config <archivo>` (default: busca
`conf.ariadne` en el directorio actual), `--max-depth N`,
`--hide-generated`, `--theme light|dark`.

Sin ningún flag ni `conf.ariadne`, `ariadne generate .` ya genera algo
razonable con defaults.

## `conf.ariadne` mínimo

```toml
[project]
name = "MiProyecto"

[output]
formats = ["html"]
dir = "./docs"
```

Referencia completa de secciones (colores, filtros, `[[docs.pages]]` para
que un nodo redirija a una URL de documentación propia): generar un
`conf.ariadne.example` no es necesario, cualquier campo no declarado usa
un default sensato.

## Cómo leer la salida como agente, no solo como imagen

El HTML generado embebe el grafo completo como JSON, sin necesidad de
renderizar nada:

```html
<script type="application/json" id="ariadne-graph-data">{ "nodes": [...], "edges": [...] }</script>
```

Extraé ese bloque (por ejemplo con un regex o un DOM parser) y parseálo
como JSON para obtener:

- **`nodes`**: cada uno con `id` (ruta relativa, o id sintético para
  clases/métodos/librerías), `node_type` (`root`/`directory`/`file`/
  `class`/`method`/`attribute`/`library`), `label`, `parent_id`, `depth`,
  y `metadata`, que incluye `language`, `category`, `line_count`,
  `last_author`/`last_modified`/`recent_commits` (si el proyecto es un
  repo git), y `doc_url` (si el nodo tiene una página de documentación
  propia vinculada vía `[[docs.pages]]`, un link, Ariadne no la renderiza).
- **`edges`**: `edge_type` `"contains"` (jerarquía archivo/carpeta/clase) o
  `"depends_on"` (referencia real entre archivos, imports resueltos).

Esto es más confiable y más barato que interpretar la imagen renderizada:
para responder "¿qué archivos dependen de X?" o "¿qué clases tiene este
archivo?", filtrar `edges`/`nodes` directamente es exacto; mirar el dibujo
no lo es.

## Limitaciones a tener en cuenta

- Heurística de texto, no un parser AST real. Puede no capturar imports
  dinámicos, macros, o metaprogramación poco convencional.
- El grafo no persigue rutas de imports absolutos de convenciones que
  Ariadne no reconoce todavía; un `depends_on` faltante no siempre implica
  que el archivo esté aislado.
- El historial de git (`last_author`, `recent_commits`) requiere que el
  proyecto analizado sea un repo git real con `git` disponible.
