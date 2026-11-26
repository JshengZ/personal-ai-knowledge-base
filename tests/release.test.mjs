import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  mkdtemp,
  mkdir,
  writeFile,
  readFile,
  rm,
  access,
} from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { prepareRelease } from "../scripts/prepare-release.mjs";

const commit = "a".repeat(40);
async function fixture(context, modify = () => {}) {
  const directory = await mkdtemp(join(tmpdir(), "pkb-release-"));
  context.after(() => rm(directory, { recursive: true, force: true }));
  for (const [target, platform, extension] of [
    ["mac", "mac-x64", "dmg"],
    ["win", "win-x64", "exe"],
  ]) {
    const root = join(directory, target);
    await mkdir(root);
    const bytes = Buffer.from("synthetic installer bytes: " + platform);
    const name = `Personal-Knowledge-1.0.1-${platform}.${extension}`;
    const info = {
      version: "1.0.1",
      source_commit: commit,
      source_state: "committed-source",
      knowledge_data_included: false,
      credentials_included: false,
      assets: [
        {
          platform,
          name,
          signed: false,
          bytes: bytes.length,
          sha256: createHash("sha256").update(bytes).digest("hex"),
        },
      ],
    };
    await writeFile(join(root, name), bytes);
    modify(info, target);
    await writeFile(join(root, "BUILD-INFO.json"), JSON.stringify(info));
  }
  const notesFile = join(directory, "notes.md");
  await writeFile(notesFile, "# Test release\n<!-- BUILD_VERIFICATION -->\n");
  return {
    directory,
    commit,
    sourceRun: "10",
    installerRun: "20",
    repository: "example/project",
    notesFile,
    publisherCommit: "b".repeat(40),
  };
}

test("assemble both verified platform artifacts with exact source and hashes", async (context) => {
  const options = await fixture(context);
  const { output, metadata } = await prepareRelease(options);
  assert.equal(metadata.source_commit, commit);
  assert.equal(metadata.publisher_commit, options.publisherCommit);
  assert.equal(metadata.assets.length, 2);
  const sums = await readFile(join(output, "SHA256SUMS.txt"), "utf8");
  for (const asset of metadata.assets)
    assert.ok(sums.includes(asset.sha256 + "  " + asset.name));
  const notes = await readFile(join(output, "RELEASE-NOTES.md"), "utf8");
  assert.ok(
    notes.includes("https://github.com/example/project/actions/runs/20"),
  );
  assert.ok(!notes.includes("BUILD_VERIFICATION"));
});

test("reject a mismatched source commit before creating release output", async (context) => {
  const options = await fixture(context, (info, target) => {
    if (target === "win") info.source_commit = "c".repeat(40);
  });
  await assert.rejects(prepareRelease(options), /source commit mismatch/);
  await assert.rejects(access(join(options.directory, "publish")));
});

test("reject modified installer bytes before publication", async (context) => {
  const options = await fixture(context);
  const file = join(
    options.directory,
    "win",
    "Personal-Knowledge-1.0.1-win-x64.exe",
  );
  const bytes = await readFile(file);
  bytes[0] ^= 1;
  await writeFile(file, bytes);
  await assert.rejects(prepareRelease(options), /checksum mismatch/);
  await assert.rejects(access(join(options.directory, "publish")));
});

test("reject differing platform versions and unsafe asset names", async (context) => {
  const options = await fixture(context, (info, target) => {
    if (target === "win") info.version = "1.0.2";
  });
  await assert.rejects(prepareRelease(options), /Platform versions differ/);
  const unsafe = await fixture(context, (info, target) => {
    if (target === "win") info.assets[0].name = "../../unexpected.exe";
  });
  await assert.rejects(prepareRelease(unsafe));
});
