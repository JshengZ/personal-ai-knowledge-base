import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createReadStream, existsSync } from "node:fs";
import { readFile, readdir } from "node:fs/promises";
import { resolve, join } from "node:path";
import { listPackage } from "@electron/asar";
import { execFileSync } from "node:child_process";

const windows = ["win", "win32"].includes(process.argv[2]);
const platform = windows ? "win-x64" : "mac-" + process.arch;
const resources = resolve(
  windows
    ? "release/win-x64/win-unpacked/resources"
    : "release/" + platform + "/mac/个人知识库.app/Contents/Resources",
);
async function hash(path) {
  const digest = createHash("sha256");
  for await (const bytes of createReadStream(path)) digest.update(bytes);
  return digest.digest("hex");
}
// ASAR lists use the host OS separator; audit one canonical form on both targets.
const files = listPackage(join(resources, "app.asar")).map((file) =>
  file.replaceAll("\\", "/"),
);
assert.ok(files.some((file) => file.endsWith("/desktop-dist/main.cjs")));
assert.ok(files.some((file) => file.endsWith("/THIRD-PARTY-NOTICES.md")));
assert.equal(
  files.filter((file) =>
    /(?:^|\/)\.local(?:\/|$)|knowledge\.sqlite|agent-tokens\.json|service\.json|\.pkb$/.test(
      file,
    ),
  ).length,
  0,
);
const unpacked = join(resources, "app.asar.unpacked");
for (const file of [
  "dist/index.html",
  "dist/favicon.png",
  "desktop-dist/parser.mjs",
  "node_modules/pdfjs-dist/package.json",
  "node_modules/tesseract.js/package.json",
])
  assert.ok(existsSync(join(unpacked, file)), file);
const native = join(unpacked, "node_modules/@napi-rs");
const modules = (await readdir(native))
  .filter((name) => name.startsWith("canvas-"))
  .sort();
assert.deepEqual(modules, [
  windows ? "canvas-win32-x64-msvc" : "canvas-darwin-" + process.arch,
]);
assert.deepEqual(
  (await readdir(join(resources, "runtime"))).sort(),
  [platform, "models"].sort(),
);
const modelHash = await hash(
  join(resources, "runtime/models/Qwen3-Embedding-0.6B-Q8_0.gguf"),
);
assert.equal(
  modelHash,
  "06507c7b42688469c4e7298b0a1e16deff06caf291cf0a5b278c308249c3e439",
);
const executable = windows ? "knowledge-engine.exe" : "knowledge-engine";
const expectedVersion = JSON.parse(
  await readFile("package.json", "utf8"),
).version;
assert.equal(
  execFileSync(join(resources, "engine", executable), ["--version"], {
    encoding: "utf8",
  }).trim(),
  "knowledge-engine " + expectedVersion,
);
if (windows && process.platform === "win32") {
  const metadata = JSON.parse(
    execFileSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "(Get-Item -LiteralPath $env:PKB_VERSION_ENGINE).VersionInfo | Select-Object ProductVersion,FileVersion,ProductName,FileDescription | ConvertTo-Json -Compress",
      ],
      {
        encoding: "utf8",
        env: {
          ...process.env,
          PKB_VERSION_ENGINE: join(resources, "engine", executable),
        },
      },
    ),
  );
  assert.equal(metadata.ProductVersion, expectedVersion);
  assert.equal(metadata.FileVersion, expectedVersion);
  assert.equal(metadata.ProductName, "Personal Knowledge");
  assert.match(metadata.FileDescription, /Knowledge Engine/);
}
assert.deepEqual(await readdir(join(resources, "engine")), [executable]);
const candidates = (
  windows
    ? ["engine/target/x86_64-pc-windows-gnu/release/", "engine/target/release/"]
    : ["engine/target/release/"]
)
  .map((directory) => resolve(directory, executable))
  .filter(existsSync);
assert.ok(
  (await Promise.all(candidates.map(hash))).includes(
    await hash(join(resources, "engine", executable)),
  ),
  "bundled engine must match a built target",
);
const organization = JSON.parse(
  await readFile(join(resources, "company-config.json"), "utf8"),
);
assert.deepEqual(
  Object.keys(organization).sort(),
  ["schema_version", "organization", "chat", "embedding", "rerank"].sort(),
);
for (const kind of ["chat", "embedding", "rerank"]) {
  for (const key of Object.keys(organization[kind]))
    assert.ok(
      ["base_url", "model", "protocol", "endpoint", "mode", "enabled"].includes(
        key,
      ),
    );
  assert.equal(organization[kind].base_url, "");
}
for (const lang of ["eng", "chi_sim"])
  assert.ok(existsSync(join(resources, "ocr", lang + ".traineddata")));
if (windows) {
  const manifest = JSON.parse(
    await readFile(
      join(resources, "runtime/win-x64/build-manifest.json"),
      "utf8",
    ),
  );
  assert.equal(
    await hash(join(resources, "runtime/win-x64/llama-server.exe")),
    manifest.binary_sha256,
  );
}
console.log(
  JSON.stringify({
    platform,
    verified: true,
    nativeModules: modules,
    modelSha256: modelHash,
    privateDataFiles: 0,
    credentials: "not bundled",
  }),
);
