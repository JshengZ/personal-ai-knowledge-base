import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { readFile, mkdir } from "node:fs/promises";
import { resolve } from "node:path";

// npm intentionally skips other operating systems' optional native packages.
// A cross-built installer still needs the target platform's precompiled module.
const windows = ["win", "win32"].includes(process.argv[2]);
const name = windows
  ? "@napi-rs/canvas-win32-x64-msvc"
  : "@napi-rs/canvas-darwin-" + process.arch;
const lock = JSON.parse(await readFile("package-lock.json", "utf8"));
const entry = lock.packages["node_modules/" + name];
if (!entry?.version || !entry.integrity)
  throw new Error(
    "Target canvas package is missing from the lockfile: " + name,
  );
const directory = resolve("node_modules", name);
if (existsSync(directory + "/package.json")) {
  const found = JSON.parse(await readFile(directory + "/package.json", "utf8"));
  if (found.version === entry.version) process.exit(0);
}
async function run(command, args) {
  const child = spawn(command, args, {
    stdio: "inherit",
    shell: process.platform === "win32" && command === "npm",
  });
  const code = await new Promise((accept, reject) => {
    child.on("error", reject);
    child.on("exit", accept);
  });
  if (code) throw new Error(command + " failed: " + code);
}
await mkdir(resolve(".local/downloads"), { recursive: true });
await run("npm", [
  "pack",
  name + "@" + entry.version,
  "--pack-destination",
  resolve(".local/downloads"),
  "--silent",
]);
const archive = resolve(
  ".local/downloads",
  name.replace("@", "").replace("/", "-") + "-" + entry.version + ".tgz",
);
const [algorithm, expected] = entry.integrity.split("-");
const actual = createHash(algorithm)
  .update(await readFile(archive))
  .digest("base64");
if (actual !== expected)
  throw new Error("Target canvas archive integrity mismatch");
await mkdir(directory, { recursive: true });
await run("tar", ["-xzf", archive, "-C", directory, "--strip-components=1"]);
