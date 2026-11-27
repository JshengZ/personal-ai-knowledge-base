import test from "node:test";
import assert from "node:assert/strict";
import {
  mkdtemp,
  mkdir,
  writeFile,
  readFile,
  rm,
  access,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { _electron as electron } from "playwright";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";

const electronBinary = process.env.PKB_PACKAGED_APP
  ? undefined
  : (await import("electron")).default;
async function until(fn, label, timeout = 20000) {
  const start = Date.now();
  let last;
  while (Date.now() - start < timeout) {
    try {
      const result = await fn();
      if (result) return result;
    } catch (error) {
      last = error;
    }
    await delay(100);
  }
  throw new Error(label + (last ? ": " + last.message : ""));
}

test(
  "named collections, native multiselect, adaptive exclusions and dynamic MCP form one complete flow",
  { timeout: 180000 },
  async (context) => {
    const fixture = await mkdtemp(join(tmpdir(), "pkb-collections-"));
    const home = join(fixture, "data"),
      desktop = join(fixture, "Desktop"),
      notes = join(fixture, "Notes");
    for (const path of [home, desktop, notes]) await mkdir(path);
    const first = join(desktop, "FirstProject"),
      second = join(desktop, "SecondProject");
    for (const path of [first, second]) await mkdir(path);
    await writeFile(join(first, "package.json"), '{"name":"first-project"}');
    await writeFile(
      join(first, "source.py"),
      "# COLLECTION_FIRST_SOURCE\nprint('first')",
    );
    await writeFile(
      join(second, "pyproject.toml"),
      '[project]\nname="second-project"',
    );
    await writeFile(
      join(second, "design.md"),
      "COLLECTION_SECOND_SOURCE original design",
    );
    await writeFile(
      join(notes, "note.md"),
      "COLLECTION_NOTES_SOURCE original note",
    );
    for (const name of ["tool.build", "tool.dist", "tool.onefile-build"]) {
      const output = join(first, name);
      await mkdir(output);
      for (const extension of ["c", "h", "json", "txt"])
        await writeFile(
          join(output, "generated." + extension),
          "GENERATED_NOISE_MUST_NOT_RECALL",
        );
    }
    const unknown = join(first, "renamed-output");
    await mkdir(unknown);
    await writeFile(join(unknown, "__constants.c"), "generated");
    await writeFile(join(unknown, "__helpers.c"), "generated");
    await writeFile(join(unknown, "copy.md"), "RENAMED_GENERATED_NOISE");
    for (const name of ["setup.exe", "archive.zip", "video.mp4"])
      await writeFile(join(desktop, name), "BINARY_MEDIA_NOISE");
    const app = await electron.launch({
      executablePath: process.env.PKB_PACKAGED_APP || electronBinary,
      args: process.env.PKB_PACKAGED_APP ? [] : [resolve(".")],
      env: { ...process.env, PKB_HOME: home },
      timeout: 30000,
    });
    const errors = [];
    let client;
    const shots =
      process.env.PKB_COLLECTION_ARTIFACTS ||
      (await mkdtemp(join(tmpdir(), "pkb-collections-shots-")));
    await mkdir(shots, { recursive: true });
    try {
      const page = await app.firstWindow();
      page.on("pageerror", (error) => errors.push(error.message));
      await page.waitForFunction(() => !!window.knowledge);
      await page.getByRole("button", { name: "跳过导览", exact: true }).click();
      const api = (method, path, body) =>
        page.evaluate(
          ({ method, path, body }) =>
            window.knowledge.request(method, path, body),
          { method, path, body },
        );
      await api("PATCH", "/settings", {
        embedding_mode: "disabled",
        auto_summarize: false,
      });
      // This connection predates every collection and project below.
      const config = await api("POST", "/connections", {
        name: "Dynamic scope agent",
        kind: "other",
      });
      assert.equal(config.connection.scope_mode, "all");
      client = new Client({ name: "collections-acceptance", version: "1.0.2" });
      await client.connect(
        new StreamableHTTPClientTransport(new URL(config.http.url), {
          requestInit: { headers: config.http.headers },
        }),
      );
      const tool = async (name, args = {}) => {
        const result = await client.callTool({ name, arguments: args });
        return {
          error: result.isError,
          value: result.isError
            ? { message: result.content[0].text }
            : JSON.parse(result.content[0].text),
        };
      };
      await app.evaluate(
        ({ dialog }, paths) => {
          globalThis.__directoryProperties = [];
          dialog.showOpenDialog = async (...args) => {
            globalThis.__directoryProperties.push(args.at(-1).properties);
            return { canceled: false, filePaths: paths };
          };
        },
        [desktop, notes],
      );
      await page
        .getByRole("button", { name: "添加资料目录", exact: true })
        .click();
      let dialog = page.getByRole("dialog", { name: "添加资料目录" });
      assert.equal(
        await dialog
          .getByRole("button", { name: "取消", exact: true })
          .isEnabled(),
        true,
        "empty selection can be cancelled",
      );
      await dialog
        .getByRole("button", { name: "选择目录", exact: true })
        .click();
      await until(
        () =>
          dialog
            .locator(".directory-batch-row")
            .count()
            .then((count) => count === 2),
        "both chosen directories appear",
      );
      assert.ok(
        (
          await app.evaluate(() => globalThis.__directoryProperties[0])
        ).includes("multiSelections"),
      );
      await dialog.getByLabel("名称", { exact: true }).nth(0).fill("工作资料");
      await dialog.getByLabel("名称", { exact: true }).nth(1).fill("技术笔记");
      await page.screenshot({ path: join(shots, "batch-add.png") });
      const box = await dialog
        .getByRole("button", { name: "加入知识库", exact: true })
        .boundingBox();
      assert.ok(
        box &&
          box.y >= 0 &&
          box.y + box.height <= (await page.evaluate(() => innerHeight)),
        "long form retains a visible action footer",
      );
      await app.evaluate(() => {
        const original = globalThis.fetch;
        globalThis.__collectionBatchPending = false;
        globalThis.fetch = async (...args) => {
          if (String(args[0]).endsWith("/api/roots/batch")) {
            globalThis.__collectionBatchPending = true;
            await new Promise((resolve) => {
              globalThis.__releaseCollectionBatch = resolve;
            });
            globalThis.fetch = original;
          }
          return original(...args);
        };
      });
      await dialog
        .getByRole("button", { name: "加入知识库", exact: true })
        .click();
      await until(
        () => app.evaluate(() => globalThis.__collectionBatchPending),
        "batch request is held for pending-form verification",
      );
      assert.equal(
        await dialog.getByLabel("名称", { exact: true }).first().isDisabled(),
        true,
      );
      assert.equal(
        await dialog
          .getByLabel("资料目录", { exact: true })
          .first()
          .isDisabled(),
        true,
      );
      assert.equal(
        await dialog.getByLabel("资料用途", { exact: true }).isDisabled(),
        true,
      );
      assert.equal(
        await dialog.getByLabel("文档与笔记", { exact: true }).isDisabled(),
        true,
      );
      await app.evaluate(() => globalThis.__releaseCollectionBatch());
      await dialog.waitFor({ state: "hidden" });
      let state = await until(async () => {
        const state = await api("GET", "/state");
        return state.documents.length === 5 &&
          state.documents.every((document) => document.status === "ready")
          ? state
          : false;
      }, "only five original sources are indexed");
      const collection = state.roots.find((root) => root.name === "工作资料");
      assert.ok(collection);
      assert.ok(state.roots.some((root) => root.name === "技术笔记"));
      assert.ok(
        !state.documents.some((document) =>
          /\.build|\.dist|onefile|renamed-output/.test(document.path),
        ),
      );
      await page
        .locator("#file-project")
        .getByRole("option", { name: "工作资料", exact: true })
        .waitFor({ state: "attached" });
      await page
        .locator("#file-project")
        .selectOption("collection:" + collection.id);
      await until(
        () =>
          page
            .locator(".file-table-row .file-name strong")
            .count()
            .then((count) => count === 4),
        "collection scope includes both projects",
      );
      await page.locator("#file-project").click();
      await page.screenshot({ path: join(shots, "collection-scopes.png") });
      await page.keyboard.press("Escape");
      let projects = (await tool("list_projects")).value.projects;
      assert.equal(
        projects.length,
        3,
        "new projects are shared without editing the existing connection",
      );
      assert.ok(
        projects.some((project) => project.collection_name === "工作资料"),
      );
      const filtered = await tool("search", {
        query: "COLLECTION",
        collection_id: collection.id,
      });
      assert.ok(filtered.value.hits.length);
      assert.ok(
        filtered.value.hits.every((hit) =>
          state.documents.some(
            (document) =>
              document.id === hit.document_id &&
              document.root_id === collection.id,
          ),
        ),
      );
      // New directories and files inherit the already-established dynamic connection.
      const third = join(desktop, "ThirdProject");
      await mkdir(third);
      await writeFile(join(third, "new.md"), "AUTO_NEW_PROJECT_VISIBLE");
      await api("POST", "/scan", {});
      await until(async () => {
        const result = await tool("search", {
          query: "AUTO_NEW_PROJECT_VISIBLE",
        });
        return result.value.hits.length;
      }, "later project is automatically shared");
      // Turning the whole collection local immediately revokes every project in it.
      await api("PATCH", "/roots/" + collection.id, { policy: "local" });
      assert.equal(
        (await tool("search", { query: "AUTO_NEW_PROJECT_VISIBLE" })).value.hits
          .length,
        0,
      );
      await api("PATCH", "/roots/" + collection.id, {
        policy: "agents",
        name: "重命名资料",
      });
      await page
        .locator("#file-project")
        .getByRole("option", { name: "重命名资料", exact: true })
        .waitFor({ state: "attached" });
      state = await api("GET", "/state");
      const original = state.documents.find(
        (document) => document.title === "source.py",
      );
      await api("PATCH", "/settings", { paused: true });
      await api("POST", "/sources/" + original.id + "/exclude", {
        scope: "directory",
      });
      assert.equal(
        (await tool("read_source", { document_id: original.id })).error,
        true,
      );
      await until(
        async () =>
          !(
            await api("GET", "/documents?collection_id=" + collection.id)
          ).documents.some((document) => document.id === original.id),
        "excluded source disappears while paused",
      );
      await until(
        async () =>
          !(await api("GET", "/state")).documents.some(
            (document) => document.id === original.id,
          ),
        "old chunks reclaimed during pause",
      );
      await access(join(first, "source.py"));
      const rules = (await api("GET", "/state")).roots.find(
        (root) => root.id === collection.id,
      );
      rules.filters.excluded_paths = [];
      await api("PATCH", "/roots/" + collection.id, { filters: rules.filters });
      await api("PATCH", "/settings", { paused: false });
      await api("POST", "/scan", {});
      const restored = await until(async () => {
        const current = await api("GET", "/state");
        return current.documents.find(
          (document) =>
            document.title === "source.py" && document.status === "ready",
        );
      }, "restored inclusion reindexes correctly");
      // A selected connection stays restricted when another project appears.
      const selected = await api("POST", "/connections", {
        name: "Manual scope",
        kind: "other",
        project_ids: [restored.project_id],
      });
      assert.equal(selected.connection.scope_mode, "selected");
      // A generated directory selected directly is explained before submission.
      const direct = await api("POST", "/roots/inspect", {
        items: [{ path: join(first, "tool.dist") }],
      });
      assert.match(direct.items[0].error, /重叠/);
      const directOutput = join(fixture, "external-output.dist");
      await mkdir(directOutput);
      await writeFile(join(directOutput, "generated.md"), "direct output");
      const directResult = await api("POST", "/roots/inspect", {
        items: [{ path: directOutput }],
      });
      assert.match(directResult.items[0].warning, /编译产物/);
      // Partial batch failure returns per-item results without duplicating successes.
      const extra = join(fixture, "Extra");
      await mkdir(extra);
      await writeFile(join(extra, "extra.md"), "EXTRA_COLLECTION");
      const partial = await api("POST", "/roots/batch", {
        items: [{ path: notes }, { path: extra, name: "补充资料" }],
      });
      assert.equal(partial.added.length, 1);
      assert.equal(partial.errors.length, 1);
      await until(
        async () =>
          (await api("GET", "/state")).documents.some(
            (document) => document.title === "extra.md",
          ),
        "successful partial-batch entry indexes",
      );
      state = await api("GET", "/state");
      const project = state.projects.find(
        (project) => project.root_id === collection.id,
      );
      await api("PATCH", "/projects/" + project.id, {
        goal: "保留的手写项目背景",
      });
      // Keep the deleted collection selected and verify the fallback immediately.
      await page
        .locator("#file-project")
        .selectOption("collection:" + collection.id);
      await api("DELETE", "/roots/" + collection.id);
      await until(
        () =>
          page
            .locator("#file-project")
            .inputValue()
            .then((value) => value === ""),
        "deleted selected collection falls back to all sources",
      );
      assert.equal(
        await page
          .locator("#file-project")
          .getByRole("option", { name: "重命名资料", exact: true })
          .count(),
        0,
      );
      const afterDelete = (
        await tool("search", { query: "COLLECTION_FIRST_SOURCE" })
      ).value.hits;
      assert.ok(
        afterDelete.every(
          (hit) =>
            !state.documents.some(
              (document) =>
                document.id === hit.document_id &&
                document.root_id === collection.id,
            ),
        ),
        "no removed source may be recalled even if other documents match shared query terms",
      );
      assert.equal(
        (await tool("read_source", { document_id: original.id })).error,
        true,
      );
      state = await api("GET", "/state");
      assert.ok(
        state.archived_projects.some(
          (project) => project.goal === "保留的手写项目背景",
        ),
      );
      assert.ok(
        !state.projects.some((project) => project.root_id === collection.id),
      );
      await page.getByRole("button", { name: "资料目录", exact: true }).click();
      await until(
        () =>
          page
            .getByText("技术笔记", { exact: true })
            .count()
            .then((count) => count > 0),
        "remaining collection is shown",
      );
      await page.screenshot({ path: join(shots, "collections.png") });
      await page
        .getByRole("button", { name: "Agent 连接", exact: true })
        .click();
      await page
        .getByRole("button", { name: "编辑权限", exact: true })
        .first()
        .click();
      dialog = page.getByRole("dialog", { name: "编辑连接权限" });
      assert.equal(
        await dialog.getByLabel("共享范围", { exact: true }).inputValue(),
        "all",
      );
      await page.screenshot({ path: join(shots, "automatic-sharing.png") });
      await page.keyboard.press("Escape");
      await page.getByRole("button", { name: "添加连接", exact: true }).click();
      dialog = page.getByRole("dialog", { name: "添加 Agent 连接" });
      await page.setViewportSize({ width: 640, height: 520 });
      await dialog
        .getByLabel("共享范围", { exact: true })
        .selectOption("selected");
      await page.screenshot({ path: join(shots, "scope-small.png") });
      await page.keyboard.press("Escape");
      await access(join(first, "source.py"));
      assert.deepEqual(errors, []);
      context.diagnostic(
        "Native folder multiselect, independent names, dynamic MCP policy, exclusions, partial success, archival and real source preservation passed. Screenshots: " +
          shots,
      );
    } finally {
      if (client) await client.close();
      await app.close();
      await until(
        async () => {
          try {
            await readFile(join(home, "service.json"));
            return false;
          } catch (error) {
            return error.code === "ENOENT";
          }
        },
        "engine releases collection fixture files",
        15000,
      );
      await rm(fixture, {
        recursive: true,
        force: true,
        maxRetries: 20,
        retryDelay: 100,
      });
    }
  },
);
