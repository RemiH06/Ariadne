import { fileURLToPath } from "node:url";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));
const iconsRoot = path.join(here, "node_modules", "devicon", "icons");
const outFile = path.join(here, "..", "src", "assets", "icons-sprite.svg");

// icon_key (debe coincidir con classify.rs) -> nombre de carpeta + variante
// preferida dentro de devicon/icons/. Se prueban variantes en orden hasta
// encontrar un archivo existente.
const ICONS = {
  rust: ["rust-original"],
  elixir: ["elixir-original"],
  typescript: ["typescript-original"],
  javascript: ["javascript-original"],
  python: ["python-original"],
  go: ["go-original"],
  ruby: ["ruby-original"],
  java: ["java-original"],
  kotlin: ["kotlin-original"],
  c: ["c-original"],
  cplusplus: ["cplusplus-original"],
  csharp: ["csharp-original"],
  php: ["php-original"],
  swift: ["swift-original"],
  html5: ["html5-original"],
  css3: ["css3-original"],
  sass: ["sass-original"],
  markdown: ["markdown-original"],
  yaml: ["yaml-original"],
  json: ["json-original"],
  bash: ["bash-original"],
  powershell: ["powershell-original"],
  postgresql: ["postgresql-original"],
  lua: ["lua-original"],
  haskell: ["haskell-original"],
  perl: ["perl-original"],
};

async function readIconSource(key, variants) {
  for (const variant of variants) {
    const file = path.join(iconsRoot, key, `${variant}.svg`);
    try {
      return await readFile(file, "utf8");
    } catch {
      // probar la siguiente variante
    }
  }
  return null;
}

function namespaceIds(svgInner, key) {
  const prefixed = svgInner.replace(/id="([^"]+)"/g, (_m, id) => `id="${key}-${id}"`);
  return prefixed.replace(
    /(url\(#|href="#|xlink:href="#)([^")]+)/g,
    (_m, prefix, id) => `${prefix}${key}-${id}`
  );
}

function toSymbol(key, svgSource) {
  const match = svgSource.match(/<svg[^>]*viewBox="([^"]*)"[^>]*>([\s\S]*)<\/svg>/i);
  const viewBox = match ? match[1] : "0 0 128 128";
  const inner = match ? match[2] : svgSource;
  return `<symbol id="icon-${key}" viewBox="${viewBox}">${namespaceIds(inner, key)}</symbol>`;
}

async function main() {
  const symbols = [];
  const missing = [];

  for (const [key, variants] of Object.entries(ICONS)) {
    const source = await readIconSource(key, variants);
    if (!source) {
      missing.push(key);
      continue;
    }
    symbols.push(toSymbol(key, source));
  }

  if (missing.length > 0) {
    console.warn(`aviso: sin ícono disponible para: ${missing.join(", ")}`);
  }

  const sprite = symbols.join("\n");
  await writeFile(outFile, sprite, "utf8");
  console.log(`sprite escrito en ${outFile} (${symbols.length} íconos)`);
}

main().catch((err) => {
  console.error(err);
  process.exitCode = 1;
});
