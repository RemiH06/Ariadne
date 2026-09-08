import * as esbuild from "esbuild";
import { fileURLToPath } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));
const outfile = path.join(here, "..", "src", "assets", "client-bundle.js");

await esbuild.build({
  entryPoints: [path.join(here, "src", "main.ts")],
  bundle: true,
  format: "iife",
  target: "es2020",
  minify: true,
  outfile,
});

console.log(`bundle escrito en ${outfile}`);
