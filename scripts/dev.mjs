import { spawn } from "node:child_process";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { existsSync } from "node:fs";
import { resolve, join } from "node:path";

const home = resolve(process.env.PKB_HOME || ".local/dev");
await mkdir(home, { recursive: true, mode: 0o700 });
await mkdir("desktop-dist", { recursive: true });
await import("node:fs/promises").then((fs) =>
  fs.copyFile("desktop/parser.mjs", "desktop-dist/parser.mjs"),
);
const runtime = {
  parser_runtime: process.execPath,
  parser_script: resolve("desktop-dist/parser.mjs"),
  resources: resolve("resources"),
  web_root: resolve("dist"),
  mcp_executable: resolve(
    "engine/target/debug/knowledge-engine" +
      (process.platform === "win32" ? ".exe" : ""),
  ),
  electron_runtime: false,
};
await writeFile(join(home, "runtime.json"), JSON.stringify(runtime), {
  mode: 0o600,
});
function run(command, args, options = {}) {
  return spawn(command, args, { stdio: "inherit", ...options });
}
if (!existsSync(runtime.mcp_executable) || process.argv.includes("--rebuild")) {
  const build = run(
    "cargo",
    ["build", "--manifest-path", "engine/Cargo.toml"],
    { env: { ...process.env, RUSTUP_USE_CURL: "1" } },
  );
  const code = await new Promise((resolve) => build.on("exit", resolve));
  if (code) process.exit(Number(code));
}
let existing = false;
try {
  const info = JSON.parse(await readFile(join(home, "service.json"), "utf8"));
  existing = (
    await fetch("http://127.0.0.1:" + info.port + "/api/health", {
      headers: { Authorization: "Bearer " + info.token },
      signal: AbortSignal.timeout(1000),
    })
  ).ok;
} catch {}
const service = existing
  ? null
  : run(runtime.mcp_executable, ["serve", "--home", home]);
for (let i = 0; i < 100; i++) {
  try {
    const info = JSON.parse(await readFile(join(home, "service.json"), "utf8"));
    if (
      (
        await fetch("http://127.0.0.1:" + info.port + "/api/health", {
          headers: { Authorization: "Bearer " + info.token },
        })
      ).ok
    )
      break;
  } catch {}
  await new Promise((resolve) => setTimeout(resolve, 200));
}
const vite = run(
  process.execPath,
  [
    "node_modules/vite/bin/vite.js",
    ...process.argv.slice(2).filter((arg) => arg !== "--rebuild"),
  ],
  { env: { ...process.env, PKB_HOME: home } },
);
const stop = () => {
  vite.kill();
  service?.kill();
};
process.on("SIGINT", stop);
process.on("SIGTERM", stop);
vite.on("exit", (code) => {
  service?.kill();
  process.exit(code || 0);
});
