import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import {
  readFile,
  writeFile,
  readdir,
  mkdir,
  copyFile,
  rm,
} from "node:fs/promises";
import { join, dirname, resolve } from "node:path";

const previous = JSON.parse(await readFile("licenses/components.json", "utf8"));
const roots = new Set(
  execFileSync("npm", ["ls", "--omit=dev", "--all", "--parseable"], {
    encoding: "utf8",
    shell: process.platform === "win32",
  })
    .trim()
    .split("\n")
    .filter((path) => path.includes("node_modules")),
);
const queue = [
  "@fontsource-variable/noto-serif-sc",
  "@phosphor-icons/react",
  "react",
  "react-dom",
  "react-markdown",
  "@napi-rs/canvas-win32-x64-msvc",
]
  .map((name) => resolve("node_modules", name))
  .filter((root) => existsSync(join(root, "package.json")));
const visited = new Set();
function dependency(from, name) {
  for (let current = from; ; current = dirname(current)) {
    const root = join(current, "node_modules", name);
    if (existsSync(join(root, "package.json"))) return root;
    if (current === dirname(current)) return null;
  }
}
while (queue.length) {
  const root = queue.pop();
  if (visited.has(root)) continue;
  visited.add(root);
  roots.add(root);
  const pkg = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
  for (const name of Object.keys({
    ...pkg.dependencies,
    ...pkg.optionalDependencies,
  })) {
    const found = dependency(root, name);
    if (found) queue.push(found);
  }
}
const npm = [];
for (const root of roots) {
  const pkg = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
  const id = (pkg.name + "-" + pkg.version).replaceAll("/", "-");
  const out = join("licenses/npm", id);
  await mkdir(out, { recursive: true });
  for (const file of await readdir(root)) {
    if (/^(licen[sc]e|copying|notice)([._-]|$)/i.test(file)) {
      try {
        await copyFile(join(root, file), join(out, file));
      } catch {}
    } else if (
      /^readme/i.test(file) &&
      /licen[sc]e|copyright/i.test(await readFile(join(root, file), "utf8"))
    ) {
      await copyFile(join(root, file), join(out, file));
    }
  }
  if (pkg.name.startsWith("@napi-rs/canvas-"))
    await copyFile(
      "node_modules/@napi-rs/canvas/LICENSE",
      join(out, "LICENSE"),
    );
  const license =
    typeof pkg.license === "string" ? pkg.license : pkg.license?.type;
  if (!license) throw new Error("No declared license for " + pkg.name);
  npm.push({
    name: pkg.name,
    version: pkg.version,
    license,
    repository:
      typeof pkg.repository === "string"
        ? pkg.repository
        : pkg.repository?.url || "",
    files: await readdir(out),
  });
}
const retained = new Set(
  npm.map((pkg) => (pkg.name + "-" + pkg.version).replaceAll("/", "-")),
);
for (const folder of await readdir("licenses/npm"))
  if (!retained.has(folder))
    await rm(join("licenses/npm", folder), { recursive: true, force: true });
npm.sort((a, b) => a.name.localeCompare(b.name));
for (const pkg of previous.rust)
  pkg.files = await readdir(
    join("licenses/rust", pkg.name + "-" + pkg.version),
  );
let notices = await readFile("THIRD-PARTY-NOTICES.md", "utf8");
const rows = npm
  .map((pkg) => `| ${pkg.name} | ${pkg.version} | ${pkg.license} |`)
  .join("\n");
notices = notices.replace(
  /(## JavaScript components\n\n\| Component \| Version \| License \|\n\| --- \| --- \| --- \|\n)[\s\S]*?(\n## Rust components)/,
  "$1" + rows + "\n$2",
);
await writeFile("THIRD-PARTY-NOTICES.md", notices);
await writeFile(
  "licenses/components.json",
  JSON.stringify({ npm, rust: previous.rust }, null, 2),
);
console.log(
  `Collected ${npm.length} JavaScript package licenses, including bundled UI dependencies.`,
);
