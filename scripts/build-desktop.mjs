import { build } from "esbuild";
import { mkdir, copyFile } from "node:fs/promises";

await mkdir("desktop-dist", { recursive: true });
for (const name of ["main", "preload"]) {
  await build({
    entryPoints: ["desktop/" + name + ".ts"],
    outfile: "desktop-dist/" + name + ".cjs",
    bundle: true,
    platform: "node",
    format: "cjs",
    target: "node22",
    external: ["electron"],
    sourcemap: false,
  });
}
await copyFile("desktop/parser.mjs", "desktop-dist/parser.mjs");
