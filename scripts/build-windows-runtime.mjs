import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream, existsSync } from "node:fs";
import {
  mkdir,
  readFile,
  writeFile,
  chmod,
  copyFile,
  rm,
} from "node:fs/promises";
import { join, resolve } from "node:path";
import { homedir } from "node:os";

const version = "b12711";
const sourceHash =
  "af27c3fd7b02c3bc492481513cd864892075ae4221dc45f3e0ac59ab27331723";
const archive = resolve(".local/downloads/llama-b12711-source.tar.gz");
const source = resolve(".local/llama-source");
const output = resolve(".local/llama-windows");
const runtime = resolve("resources/runtime/win-x64");
const cmake = process.env.PKB_CMAKE || "cmake";
async function hash(path) {
  const digest = createHash("sha256");
  for await (const bytes of createReadStream(path)) digest.update(bytes);
  return digest.digest("hex");
}
async function run(command, args) {
  const child = spawn(command, args, { stdio: "inherit" });
  const code = await new Promise((accept, reject) => {
    child.on("error", reject);
    child.on("exit", accept);
  });
  if (code) throw new Error(command + " failed: " + code);
}
const existing = join(runtime, "build-manifest.json");
if (!process.argv.includes("--force") && existsSync(existing)) {
  const info = JSON.parse(await readFile(existing, "utf8"));
  if (
    info.source_sha256 === sourceHash &&
    info.path_remapped === true &&
    (await hash(join(runtime, "llama-server.exe"))) === info.binary_sha256
  ) {
    console.log("Verified self-contained Windows embedding runtime.");
    process.exit(0);
  }
}
await mkdir(resolve(".local/downloads"), { recursive: true });
if (!existsSync(archive) || (await hash(archive)) !== sourceHash) {
  await run("curl", [
    "--fail",
    "--location",
    "--retry",
    "3",
    "--connect-timeout",
    "15",
    "--max-time",
    "180",
    "--output",
    archive,
    "https://codeload.github.com/ggml-org/llama.cpp/tar.gz/refs/tags/" +
      version,
  ]);
}
if ((await hash(archive)) !== sourceHash)
  throw new Error("llama.cpp source checksum mismatch");
await mkdir(source, { recursive: true });
await run("tar", ["-xzf", archive, "-C", source, "--strip-components=1"]);
const args = [
  "-S",
  source,
  "-B",
  output,
  "-DCMAKE_BUILD_TYPE=Release",
  "-DBUILD_SHARED_LIBS=OFF",
  "-DGGML_OPENMP=OFF",
  "-DGGML_NATIVE=OFF",
  "-DGGML_AVX=OFF",
  "-DGGML_AVX2=OFF",
  "-DGGML_FMA=OFF",
  "-DGGML_F16C=OFF",
  "-DLLAMA_OPENSSL=OFF",
  "-DLLAMA_BUILD_TESTS=OFF",
  "-DLLAMA_BUILD_EXAMPLES=OFF",
  "-DLLAMA_BUILD_TOOLS=ON",
  "-DLLAMA_BUILD_SERVER=ON",
  "-DLLAMA_TOOLS_INSTALL=OFF",
  "-DLLAMA_BUILD_UI=OFF",
  "-DLLAMA_USE_PREBUILT_UI=OFF",
  "-DLLAMA_BUILD_NUMBER=12711",
  "-DLLAMA_BUILD_COMMIT=f20c5907c",
];
let compiler = "MSVC /MT";
if (process.platform === "win32") {
  args.push("-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded");
  const mapping = [
    homedir() + "=/build-home",
    resolve(".") + "=/workspace/personal-ai-knowledge-base",
  ]
    .map((value) => '"/pathmap:' + value.replaceAll('"', '\\"') + '"')
    .join(" ");
  args.push("-DCMAKE_C_FLAGS=" + mapping, "-DCMAKE_CXX_FLAGS=" + mapping);
} else {
  const mapping = [
    homedir() + "=/build-home",
    resolve(".") + "=/workspace/personal-ai-knowledge-base",
  ]
    .map((value) => '"-ffile-prefix-map=' + value.replaceAll('"', '\\"') + '"')
    .join(" ");
  args.push("-DCMAKE_C_FLAGS=" + mapping, "-DCMAKE_CXX_FLAGS=" + mapping);
  const zig = resolve(process.env.PKB_ZIG || ".local/build-tools/ziglang/zig");
  if (!existsSync(zig)) throw new Error("Set PKB_ZIG to a Zig executable.");
  const quote = (text) => "'" + text.replaceAll("'", "'\\''") + "'";
  for (const [name, command] of [
    ["cc", "cc"],
    ["cxx", "c++"],
    ["ar", "ar"],
    ["ranlib", "ranlib"],
  ]) {
    const file = resolve(".local/pkb-zig-" + name);
    await writeFile(
      file,
      "#!/bin/sh\nexec " +
        quote(zig) +
        " " +
        command +
        (name === "cc" || name === "cxx" ? " -target x86_64-windows-gnu" : "") +
        ' "$@"\n',
    );
    await chmod(file, 0o755);
  }
  args.push(
    "-DCMAKE_SYSTEM_NAME=Windows",
    "-DCMAKE_C_COMPILER=" + resolve(".local/pkb-zig-cc"),
    "-DCMAKE_CXX_COMPILER=" + resolve(".local/pkb-zig-cxx"),
    "-DCMAKE_AR=" + resolve(".local/pkb-zig-ar"),
    "-DCMAKE_RANLIB=" + resolve(".local/pkb-zig-ranlib"),
    "-DPKB_EMBEDDING_ONLY=ON",
  );
  // This hidden, loopback-only service does not need the upstream GUI manifest.
  // Avoid requiring a separate Windows resource compiler on a Mac build host.
  const manifest = join(source, "cmake/win-app.cmake");
  const text = await readFile(manifest, "utf8");
  await writeFile(
    manifest,
    text.replace(
      "function(llama_apply_app_manifests)\n",
      "function(llama_apply_app_manifests)\n    if (PKB_EMBEDDING_ONLY)\n        return()\n    endif()\n",
    ),
  );
  compiler = "Zig GNU, static C++ runtime";
}
await run(cmake, args);
await run(cmake, [
  "--build",
  output,
  "--config",
  "Release",
  "--target",
  "llama-server",
  "--parallel",
  process.env.PKB_BUILD_JOBS || "4",
]);
const binary = [
  join(output, "bin/llama-server.exe"),
  join(output, "bin/Release/llama-server.exe"),
].find(existsSync);
if (!binary) throw new Error("Embedding server was not produced.");
await rm(runtime, { recursive: true, force: true });
await mkdir(runtime, { recursive: true });
await copyFile(binary, join(runtime, "llama-server.exe"));
await copyFile(join(source, "LICENSE"), join(runtime, "LICENSE"));
await writeFile(
  join(runtime, "build-manifest.json"),
  JSON.stringify(
    {
      version,
      source_url: "https://github.com/ggml-org/llama.cpp/tree/" + version,
      source_sha256: sourceHash,
      binary_sha256: await hash(binary),
      compiler,
      path_remapped: true,
      cpu: "x64 baseline",
      shared_libraries: false,
      openmp: false,
      openssl: false,
      local_changes:
        "Skip GUI application manifest for cross-built hidden embedding service.",
    },
    null,
    2,
  ),
);
console.log("Self-contained Windows runtime prepared.");
