import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { appendFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { prepareRelease } from "./prepare-release.mjs";

const repository = process.env.GITHUB_REPOSITORY;
const run = process.env.PKB_INSTALLER_RUN;
assert.match(repository || "", /^[\w.-]+\/[\w.-]+$/);
assert.match(run || "", /^\d+$/);
const gh = (...args) =>
  execFileSync("gh", args, { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
const api = (path) => JSON.parse(gh("api", "repos/" + repository + "/" + path));
const build = api("actions/runs/" + run);
assert.equal(build.path, ".github/workflows/build-installers.yml");
assert.equal(build.event, "workflow_dispatch");
assert.equal(build.head_repository.full_name, repository);
assert.equal(build.head_branch, "main");
assert.equal(build.status, "completed");
assert.equal(
  build.conclusion,
  "success",
  "Both native package jobs must pass before publication",
);
const commit = build.head_sha;
assert.match(commit, /^[0-9a-f]{40}$/);
assert.ok(
  ["ahead", "identical"].includes(api("compare/" + commit + "...main").status),
);
const runs = api(
  "actions/runs?head_sha=" + commit + "&event=push&per_page=100",
).workflow_runs;
const checked = (path) => {
  const value = runs.find(
    (item) =>
      item.path === path &&
      item.head_sha === commit &&
      item.event === "push" &&
      item.head_repository.full_name === repository &&
      item.status === "completed" &&
      item.conclusion === "success",
  );
  assert.ok(value, "Missing successful source check: " + path);
  return value.id;
};
const sourceRun = checked(".github/workflows/ci.yml");
checked(".github/workflows/secret-scan.yml");
const directory = resolve(".local/release-promotion", run);
for (const target of ["mac", "win"])
  gh(
    "run",
    "download",
    run,
    "--repo",
    repository,
    "--name",
    "installer-" + target,
    "--dir",
    join(directory, target),
  );
const { output, metadata } = await prepareRelease({
  directory,
  repository,
  commit,
  sourceRun,
  installerRun: run,
  publisherCommit: process.env.GITHUB_SHA,
});
const tag = "v" + metadata.version;
let existing;
try {
  existing = api("releases/tags/" + tag);
} catch (error) {
  if (!String(error.stderr).includes("404")) throw error;
}
if (existing) {
  assert.equal(
    existing.draft,
    true,
    "Refusing to change an already published release",
  );
  assert.equal(
    existing.target_commitish,
    commit,
    "Draft belongs to another source commit",
  );
} else {
  gh(
    "release",
    "create",
    tag,
    "--repo",
    repository,
    "--target",
    commit,
    "--draft",
    "--title",
    tag + " — Personal Knowledge / 个人知识库",
    "--notes-file",
    join(output, "RELEASE-NOTES.md"),
  );
}
gh(
  "release",
  "edit",
  tag,
  "--repo",
  repository,
  "--notes-file",
  join(output, "RELEASE-NOTES.md"),
);
gh(
  "release",
  "upload",
  tag,
  "--repo",
  repository,
  "--clobber",
  ...metadata.assets.map((asset) => join(output, asset.name)),
  join(output, "SHA256SUMS.txt"),
  join(output, "BUILD-INFO.json"),
);
const draft = api("releases/tags/" + tag);
for (const expected of metadata.assets) {
  const actual = draft.assets.find((asset) => asset.name === expected.name);
  assert.ok(actual, "Missing uploaded installer");
  assert.equal(actual.state, "uploaded");
  assert.equal(actual.size, expected.bytes);
  if (actual.digest) assert.equal(actual.digest, "sha256:" + expected.sha256);
}
for (const name of ["SHA256SUMS.txt", "BUILD-INFO.json"])
  assert.equal(
    draft.assets.find((asset) => asset.name === name)?.state,
    "uploaded",
  );
gh("release", "edit", tag, "--repo", repository, "--draft=false", "--latest");
const published = api("releases/tags/" + tag);
assert.equal(published.draft, false);
assert.equal(api("git/ref/tags/" + tag).object.sha, commit);
if (process.env.GITHUB_STEP_SUMMARY)
  await appendFile(
    process.env.GITHUB_STEP_SUMMARY,
    `Published [${tag}](${published.html_url}) from \`${commit}\`.\n`,
  );
console.log(
  JSON.stringify({
    published: true,
    url: published.html_url,
    source_commit: commit,
    assets: published.assets.map(({ name, size, digest }) => ({
      name,
      size,
      digest,
    })),
  }),
);
