import { readFile, writeFile } from "node:fs/promises";
const { version } = JSON.parse(await readFile("package.json", "utf8"));
if (!/^\d+\.\d+\.\d+$/.test(version))
  throw Error("Use a stable three-component application version");
for (const [path, pattern, replacement] of [
  ["engine/Cargo.toml", /^(version\s*=\s*)"[^"]+"/m, `$1"${version}"`],
  [
    "engine/Cargo.lock",
    /(name = "knowledge-engine"\r?\nversion = )"[^"]+"/,
    `$1"${version}"`,
  ],
]) {
  const text = await readFile(path, "utf8");
  if (!pattern.test(text)) throw Error("Missing version in " + path);
  await writeFile(path, text.replace(pattern, replacement));
}
console.log("Application version synchronized: " + version);
