![Built with Love](https://forthebadge.com/images/badges/built-with-love.svg)
![Uses Git](https://forthebadge.com/images/badges/uses-git.svg)
![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)

```
 █████╗ ██████╗ ██╗ █████╗ ██████╗ ███╗   ██╗███████╗
██╔══██╗██╔══██╗██║██╔══██╗██╔══██╗████╗  ██║██╔════╝
███████║██████╔╝██║███████║██║  ██║██╔██╗ ██║█████╗  
██╔══██║██╔══██╗██║██╔══██║██║  ██║██║╚██╗██║██╔══╝  
██║  ██║██║  ██║██║██║  ██║██████╔╝██║ ╚████║███████╗
╚═╝  ╚═╝╚═╝  ╚═╝╚═╝╚═╝  ╚═╝╚═════╝ ╚═╝  ╚═══╝╚══════╝

        Diagrama y documentación de proyectos          by Hex (@RemiH06)          version 0.1.0
```

![License](https://img.shields.io/badge/License-AGPL%20v3-blue.svg?style=for-the-badge)

---

## 🧵 Descripción general

**Ariadne** analiza un proyecto de código y genera un diagrama HTML
interactivo (árbol de archivos, dependencias entre ellos, clases, métodos y
atributos, historial de git) además de documentación exportable a PDF,
LaTeX, RTF y XML. El nombre viene del hilo de Ariadna del mito griego, la
guía que deja seguir el camino de vuelta a través de un laberinto. En este
caso, el laberinto de un codebase que no es tuyo, o que ya no recuerdas.

Usa heurística de texto en vez de un parser AST real, así que soporta 15
lenguajes sin depender de una gramática distinta por cada uno, a costa de
romperse con macros o metaprogramación poco convencional. Consulta
[`docs/architecture.html`](docs/architecture.html) para el porqué completo
de esa decisión. Ese mismo sitio (`docs/`) es la documentación completa de
Ariadne, con su propio diagrama incrustado como forma de navegar.

```diff
+ Diagrama HTML interactivo y autocontenido, funciona con file:// o subido tal cual a GitHub Pages
+ 15 lenguajes: referencias entre archivos, clases/métodos/atributos, dependencias declaradas
+ Autor y fecha de git por archivo, colorear por antigüedad, "ver historial", link a documentación propia
+ Exporta a PDF/LaTeX/RTF/XML vía Pandoc, desde el mismo grafo
- No es un parser AST real: es heurística de texto, se rompe con macros o metaprogramación poco convencional
- No hace blame línea por línea todavía: usa "ver historial" o abre el archivo directo
```

---

## ⚙️ Arquitectura

```
46.Ariadne/
├── src/
│   ├── cli/            # subcomandos (generate, ...)
│   ├── config/          # parseo de conf.ariadne
│   ├── extractor/        # walk → classify → imports/classes/git_blame → build_graph
│   ├── render/
│   │   ├── html/         # diagrama interactivo (embebe el bundle de client/)
│   │   └── docs/          # Markdown canónico a Pandoc, y valida [[docs.pages]] (redirect, no embebe nada)
│   ├── schema/           # el Graph (nodes/edges) que viaja entre extractor y render
│   └── assets/           # bundle de JS, sprite de íconos, logo, skill de agente, todo embebido en el binario
├── client/               # TypeScript + D3, compilado a un solo bundle con esbuild
├── docs/                 # sitio de documentación propio (GitHub Pages), index.html incrusta el diagrama como navegación
├── conf.ariadne          # config de Ariadne documentándose a sí misma
└── conf.ariadne.example  # referencia comentada de cada sección
```

Pipeline completo, con el porqué de cada decisión: [`docs/architecture.html`](docs/architecture.html).
Cómo agregar un lenguaje nuevo: [`docs/extending-languages.html`](docs/extending-languages.html).

---

## 🚀 Instalación y quickstart

```bash
git clone <este-repo>
cd 46.Ariadne
cargo install --path .

# genera el diagrama de cualquier proyecto
ariadne generate ruta/al/proyecto --out salida --title "Mi Proyecto"

# o de este mismo repo, usando su propio conf.ariadne
ariadne generate .
```

Sin `--config`, Ariadne busca `conf.ariadne` en el directorio actual. Si no
existe, usa defaults razonables. Los flags de la CLI pisan `conf.ariadne`,
que a su vez pisa los defaults.

El `conf.ariadne` de este repo genera el diagrama directo en `docs/` (no en
`output/`, que está en `.gitignore`), así queda versionado junto al resto
del sitio, listo para servirse con GitHub Pages apuntando a `docs/`.

---

## 🔧 Configuración

Todo vive en `conf.ariadne`. Consulta
[`conf.ariadne.example`](conf.ariadne.example) para la referencia completa
y comentada de cada sección (`[project]`, `[output]`,
`[output.html.colors]`, `[filters]`, `[ignore]`, `[[docs.pages]]`). Lo más
relevante:

- **`[filters]`**: qué se oculta por defecto (generados, extensiones,
  rutas), también ajustable en vivo desde el panel del diagrama.
- **`[[docs.pages]]`**: conecta un nodo del grafo a una URL propia. El nodo
  se marca con una estrella y gana un link "Ir a documentación". Ariadne
  no renderiza nada de esa página, solo redirige. Ver
  [`docs/docs-linking.html`](docs/docs-linking.html).

---

## 🤖 Para agentes de IA

Ariadne está pensada para que un agente también la use, no solo un humano
mirando el dibujo. El HTML generado embebe el grafo completo como JSON
(`<script type="application/json" id="ariadne-graph-data">`), así que un
agente puede leer `nodes`/`edges` estructurados en vez de interpretar una
imagen.

Dos skills de Claude Code, para dos audiencias distintas:

- **Trabajar en este repo**: `.claude/skills/ariadne-dev/` (ya incluida
  aquí, no se distribuye).
- **Usar Ariadne desde cualquier otro proyecto**: corre
  `ariadne init-agent-skill` en la raíz de ese proyecto para copiar la guía
  a `.claude/skills/ariadne/SKILL.md`, de modo que cualquier sesión de
  Claude Code ahí sepa cómo invocar Ariadne e interpretar su salida.

---

## 🏗️ Stack tecnológico

| Capa | Herramienta |
|---|---|
| Núcleo | Rust (`clap`, `rayon`, `serde`, `toml`, `chrono`) |
| Export PDF/LaTeX/RTF/XML | Pandoc (opcional, solo si pides esos formatos) |
| Diagrama interactivo | TypeScript + D3 (`d3-hierarchy`, `d3-selection`, `d3-shape`, `d3-zoom`, `d3-transition`), compilado con esbuild |
| Íconos | Devicon (sprite SVG embebido) |
| Sitio de documentación (`docs/`) | HTML/CSS sin librerías de estilo, `motion@11` vía CDN para las animaciones. Degrada a contenido estático si el import falla. |

---

## 🧪 Tests

```bash
cargo test
```

Heurística por lenguaje con 1 a 3 tests unitarios cada uno, más tests de
integración contra `fixtures/sample-project`.

---

## 📄 Licencia

Distribuido bajo AGPL-3.0.

---

## 👤 Autoría

by Hex ([@RemiH06](https://github.com/RemiH06))
