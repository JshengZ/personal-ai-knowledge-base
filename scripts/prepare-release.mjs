import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import {
  readFile,
  writeFile,
  mkdir,
  readdir,
  copyFile,
  stat,
} from "node:fs/promises";
import { join, resolve } from "node:path";

export async function prepareRelease({
  directory,
  commit,
  sourceRun,
  installerRun,
  repository,
  notesFile,
  publisherCommit,
}) {
  assert.match(commit, /^[0-9a-f]{40}$/);
  assert.match(String(sourceRun), /^\d+$/);
  assert.match(String(installerRun), /^\d+$/);
  assert.match(repository, /^[\w.-]+\/[\w.-]+$/);
  const project = "https://github.com/" + repository;
  const output = resolve(directory, "publish");
  async function find(root, name) {
    const found = [];
    for (const item of await readdir(root, { withFileTypes: true })) {
      const file = join(root, item.name);
      if (item.isDirectory()) found.push(...(await find(file, name)));
      else if (item.isFile() && item.name === name) found.push(file);
    }
    return found;
  }
  async function digest(file) {
    const hash = createHash("sha256");
    for await (const bytes of createReadStream(file)) hash.update(bytes);
    return hash.digest("hex");
  }
  const builds = {},
    assets = [],
    copies = [];
  let version;
  for (const [target, platform] of [
    ["mac", "mac-x64"],
    ["win", "win-x64"],
  ]) {
    const root = resolve(directory, target);
    const manifests = await find(root, "BUILD-INFO.json");
    assert.equal(manifests.length, 1, target + " needs one build manifest");
    const info = JSON.parse(await readFile(manifests[0], "utf8"));
    assert.equal(
      info.source_commit,
      commit,
      "Installer source commit mismatch",
    );
    assert.match(info.version, /^\d+\.\d+\.\d+(?:-[\w.-]+)?$/);
    version ??= info.version;
    assert.equal(info.version, version, "Platform versions differ");
    assert.ok(
      ["committed-source", "committed-source-with-generated-assets"].includes(
        info.source_state,
      ),
    );
    assert.equal(info.knowledge_data_included, false);
    assert.equal(info.credentials_included, false);
    assert.equal(info.assets.length, 1);
    const asset = info.assets[0];
    assert.equal(asset.platform, platform);
    assert.equal(asset.signed, false);
    assert.equal(
      asset.name,
      `Personal-Knowledge-${version}-${platform}.${target === "mac" ? "dmg" : "exe"}`,
    );
    assert.match(asset.sha256, /^[0-9a-f]{64}$/);
    const installers = await find(root, asset.name);
    assert.equal(installers.length, 1);
    assert.equal((await stat(installers[0])).size, asset.bytes);
    assert.equal(
      await digest(installers[0]),
      asset.sha256,
      "Installer checksum mismatch",
    );
    copies.push([installers[0], asset.name]);
    builds[platform] = info;
    assets.push(asset);
  }
  const metadata = {
    project,
    version,
    source_commit: commit,
    publisher_commit: publisherCommit,
    generated_at: new Date().toISOString(),
    ci: {
      source: project + "/actions/runs/" + sourceRun,
      installers: project + "/actions/runs/" + installerRun,
    },
    assets,
    builds,
    application_language: "zh-CN",
    knowledge_data_included: false,
    credentials_included: false,
    verification:
      "Native source CI and packaged-app checks passed. See linked runs and docs/verification.md for scope and target-environment acceptance boundaries.",
  };
  const template = await readFile(
    notesFile || resolve("docs/releases", `v${version}.md`),
    "utf8",
  );
  assert.ok(template.includes("<!-- BUILD_VERIFICATION -->"));
  const notes = template.replace(
    "<!-- BUILD_VERIFICATION -->",
    `Source commit: [\`${commit.slice(0, 12)}\`](${project}/commit/${commit}). Native Mac and Windows [source checks](${metadata.ci.source}) and [packaged-app checks](${metadata.ci.installers}) passed, including the guide, appearance and migration. / 对应源码已通过 Mac 与 Windows 原生源码及打包版检查，包含导览、外观与迁移。`,
  );
  await mkdir(output, { recursive: true });
  for (const [file, name] of copies) await copyFile(file, join(output, name));
  await writeFile(
    join(output, "BUILD-INFO.json"),
    JSON.stringify(metadata, null, 2) + "\n",
  );
  await writeFile(
    join(output, "SHA256SUMS.txt"),
    assets.map((asset) => asset.sha256 + "  " + asset.name).join("\n") + "\n",
  );
  await writeFile(join(output, "RELEASE-NOTES.md"), notes);
  return { output, metadata };
}
