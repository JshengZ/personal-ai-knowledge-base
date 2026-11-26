import { createHash } from "node:crypto";
import { createReadStream, existsSync } from "node:fs";
import { readFile, writeFile, mkdir, stat } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { join } from "node:path";

const pkg = JSON.parse(await readFile("package.json", "utf8"));
const files = [
  ["mac-x64", `Personal-Knowledge-${pkg.version}-mac-x64.dmg`],
  ["win-x64", `Personal-Knowledge-${pkg.version}-win-x64.exe`],
];
const assets = [];
for (const [platform, name] of files) {
  const path = join("release", platform, name);
  if (!existsSync(path)) continue;
  const hash = createHash("sha256");
  for await (const bytes of createReadStream(path)) hash.update(bytes);
  const digest = hash.digest("hex");
  assets.push({
    platform,
    name,
    bytes: (await stat(path)).size,
    sha256: digest,
    signed: false,
  });
  await writeFile(
    join("release", platform, "SHA256SUMS.txt"),
    digest + "  " + name + "\n",
  );
}
if (!assets.length)
  throw new Error("No installers found for the package version.");
let commit = null;
let sourceDirty = null;
let localChanges = null;
let generatedOnly = false;
const generatedAssets = [];
try {
  commit = execFileSync("git", ["rev-parse", "HEAD"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  }).trim();
  sourceDirty = Boolean(
    execFileSync("git", ["status", "--porcelain", "--untracked-files=normal"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim(),
  );
  if (sourceDirty) {
    const trackedChanges = execFileSync(
      "git",
      ["diff", "--name-only", "-z", "HEAD"],
      { encoding: "utf8" },
    )
      .split("\0")
      .filter(Boolean);
    const untrackedPaths = execFileSync(
      "git",
      ["ls-files", "--others", "--exclude-standard", "-z"],
      { encoding: "utf8" },
    )
      .split("\0")
      .filter(Boolean);
    const changedPaths = [...new Set([...trackedChanges, ...untrackedPaths])];
    const generated = new Set([
      "resources/assets-manifest.json",
      "resources/icon.png",
      "resources/tray.png",
      "build/icon.ico",
      "build/icon.icns",
    ]);
    generatedOnly =
      process.env.GITHUB_ACTIONS === "true" &&
      process.env.GITHUB_SHA === commit &&
      changedPaths.length > 0 &&
      changedPaths.every(
        (path) =>
          generated.has(path) ||
          (path.startsWith("build/icon.iconset/") && path.endsWith(".png")),
      );
    if (generatedOnly) {
      for (const path of changedPaths.sort())
        generatedAssets.push({
          path,
          sha256: createHash("sha256")
            .update(await readFile(path))
            .digest("hex"),
        });
    }
    const digest = createHash("sha256");
    digest.update(
      execFileSync("git", ["diff", "--binary", "HEAD"], {
        maxBuffer: 32 * 1024 * 1024,
      }),
    );
    const untracked = execFileSync(
      "git",
      ["ls-files", "--others", "--exclude-standard", "-z"],
      { encoding: "utf8" },
    )
      .split("\0")
      .filter(Boolean)
      .sort();
    for (const path of untracked) {
      const contents = await readFile(path);
      digest.update(JSON.stringify([path, contents.length]) + "\n");
      digest.update(contents);
    }
    localChanges = digest.digest("hex");
  }
} catch {
  generatedOnly = false;
  generatedAssets.length = 0;
}
const info = {
  project: "https://github.com/JshengZ/personal-ai-knowledge-base",
  version: pkg.version,
  source_commit: sourceDirty === false || generatedOnly ? commit : null,
  source_base_commit: commit,
  source_state:
    sourceDirty === null
      ? "unknown"
      : sourceDirty
        ? generatedOnly
          ? "committed-source-with-generated-assets"
          : "unreleased-working-tree"
        : "committed-source",
  generated_assets: generatedAssets,
  local_changes_sha256: localChanges,
  generated_at: new Date().toISOString(),
  assets,
  embedding_model: {
    name: "Qwen3-Embedding-0.6B-Q8_0",
    sha256: "06507c7b42688469c4e7298b0a1e16deff06caf291cf0a5b278c308249c3e439",
    license: "Apache-2.0",
  },
  verification:
    sourceDirty === false || generatedOnly
      ? "See docs/verification.md and the GitHub Actions run for the source commit."
      : "Unreleased local build. Consult docs/verification.md for local checks; prior GitHub Actions runs do not verify these working-tree changes.",
  application_language: "zh-CN",
  knowledge_data_included: false,
  credentials_included: false,
};
await mkdir("release", { recursive: true });
await writeFile(
  "release/SHA256SUMS.txt",
  assets.map((asset) => asset.sha256 + "  " + asset.name).join("\n") + "\n",
);
await writeFile(
  "release/BUILD-INFO.json",
  JSON.stringify(info, null, 2) + "\n",
);
console.log(
  JSON.stringify({
    version: pkg.version,
    assets: assets.map((asset) => ({
      name: asset.name,
      bytes: asset.bytes,
      sha256: asset.sha256,
    })),
  }),
);
