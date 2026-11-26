import { spawn } from "node:child_process";
import {
  mkdir,
  copyFile,
  readFile,
  stat,
  writeFile,
  readdir,
  rm,
} from "node:fs/promises";
import { existsSync } from "node:fs";
import { resolve, join } from "node:path";
import { homedir } from "node:os";
import { build, Platform, Arch } from "electron-builder";

const platform = process.argv[2] || process.platform;
const windows = ["win", "win32"].includes(platform);
const target =
  windows && process.platform !== "win32" ? "x86_64-pc-windows-gnu" : null;
const runtime = windows
  ? "win-x64"
  : process.arch === "arm64"
    ? "mac-arm64"
    : "mac-x64";
async function run(command, args) {
  const separator = "\u001f";
  const existing =
    process.env.CARGO_ENCODED_RUSTFLAGS?.split(separator) ||
    process.env.RUSTFLAGS?.trim().split(/\s+/).filter(Boolean) ||
    [];
  const remapped = [
    ...existing,
    "--remap-path-prefix=" + homedir() + "=/build-home",
    "--remap-path-prefix=" +
      resolve(".") +
      "=/workspace/personal-ai-knowledge-base",
  ];
  const child = spawn(command, args, {
    stdio: "inherit",
    env: {
      ...process.env,
      RUSTUP_USE_CURL: "1",
      CARGO_ENCODED_RUSTFLAGS: remapped.join(separator),
    },
  });
  const code = await new Promise((resolve, reject) => {
    child.on("error", reject);
    child.on("exit", resolve);
  });
  if (code) throw new Error(command + " exited " + code);
}
const required = [
  "resources/runtime/models/Qwen3-Embedding-0.6B-Q8_0.gguf",
  "resources/runtime/" +
    runtime +
    "/" +
    (windows ? "llama-server.exe" : "llama-server"),
  "resources/ocr/eng.traineddata",
  "resources/ocr/chi_sim.traineddata",
  "resources/tray.png",
];
for (const path of required)
  if (!existsSync(path) || (await stat(path)).size < 100)
    throw new Error("完整安装包缺少资源 " + path + "，请先运行资源准备流程。");
if (windows && !existsSync("resources/runtime/win-x64/build-manifest.json"))
  throw new Error(
    "Windows 完整包需先构建静态检索运行时，运行 npm run assets:win-runtime。",
  );
await run(process.execPath, [
  "scripts/prepare-platform.mjs",
  windows ? "win" : "mac",
]);
await run(process.execPath, ["node_modules/typescript/bin/tsc", "--noEmit"]);
await run(process.execPath, ["node_modules/vite/bin/vite.js", "build"]);
await run(process.execPath, ["scripts/build-desktop.mjs"]);
const command = process.env.PKB_CARGO_COMMAND || "cargo";
const args = [
  process.env.PKB_CARGO_SUBCOMMAND || (target ? "zigbuild" : "build"),
  "--manifest-path",
  "engine/Cargo.toml",
  "--release",
  ...(target ? ["--target", target] : []),
];
await run(command, args);
const directory = resolve("resources/engine");
await mkdir(directory, { recursive: true });
const binary = windows ? "knowledge-engine.exe" : "knowledge-engine";
await copyFile(
  join("engine/target", ...(target ? [target] : []), "release", binary),
  join(directory, binary),
);
const template = JSON.parse(
  await readFile(
    process.env.PKB_COMPANY_CONFIG || "resources/company-config.json",
    "utf8",
  ),
);
const company = {
  schema_version: 1,
  organization:
    typeof template.organization === "string" ? template.organization : "",
};
for (const kind of ["chat", "embedding", "rerank"]) {
  const provider = template[kind] || {};
  company[kind] = Object.fromEntries(
    ["base_url", "model", "protocol", "endpoint"]
      .filter((key) => typeof provider[key] === "string")
      .map((key) => [key, provider[key]]),
  );
}
company.embedding.mode = ["auto", "local", "disabled"].includes(
  template.embedding?.mode,
)
  ? template.embedding.mode
  : "auto";
company.rerank.enabled = template.rerank?.enabled === true;
const companyPath = resolve(
  ".local/package-" + runtime + "/company-config.json",
);
await mkdir(resolve(".local/package-" + runtime), { recursive: true });
await writeFile(companyPath, JSON.stringify(company, null, 2));
const extraResources = [
  { from: "resources/engine/" + binary, to: "engine/" + binary },
  { from: "resources/runtime/" + runtime, to: "runtime/" + runtime },
  { from: "resources/runtime/models", to: "runtime/models" },
  { from: "resources/ocr", to: "ocr" },
  {
    from: companyPath,
    to: "company-config.json",
  },
  { from: "resources/tray.png", to: "tray.png" },
  { from: "resources/assets-manifest.json", to: "assets-manifest.json" },
];
await rm(resolve("release", runtime, windows ? "win-unpacked" : "mac"), {
  recursive: true,
  force: true,
});
await build({
  targets: (windows ? Platform.WINDOWS : Platform.MAC).createTarget(
    windows ? "nsis" : "dmg",
    windows || process.arch !== "arm64" ? Arch.x64 : Arch.arm64,
  ),
  config: {
    directories: { output: "release/" + runtime },
    artifactName: "Personal-Knowledge-${version}-${os}-${arch}.${ext}",
    npmRebuild: false,
    files: [
      ...JSON.parse(await readFile("package.json", "utf8")).build.files,
      ...(windows
        ? ["!node_modules/@napi-rs/canvas-darwin-*/**"]
        : ["!node_modules/@napi-rs/canvas-win32-*/**"]),
      "!node_modules/@napi-rs/canvas-linux-*/**",
      "!node_modules/@napi-rs/canvas-android-*/**",
    ],
    extraResources,
    mac: { identity: process.env.CSC_LINK ? undefined : null },
    win: { signExecutable: !!process.env.CSC_LINK },
    ...(process.env.PKB_ELECTRON_DIST
      ? { electronDist: resolve(process.env.PKB_ELECTRON_DIST) }
      : {}),
    ...(process.env.ELECTRON_MIRROR
      ? { electronDownload: { mirror: process.env.ELECTRON_MIRROR } }
      : {}),
  },
});
console.log("Installer written to release/.");
