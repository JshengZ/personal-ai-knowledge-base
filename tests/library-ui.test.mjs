import test from "node:test";
import { interceptRequests } from "./ipc-requests.mjs";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { _electron as electron } from "playwright";

const executable =
  process.env.PKB_PACKAGED_APP || (await import("electron")).default;
async function until(fn, label, timeout = 25000) {
  const started = Date.now();
  let last;
  while (Date.now() - started < timeout) {
    try {
      const value = await fn();
      if (value) return value;
    } catch (error) {
      last = error;
    }
    await delay(100);
  }
  throw new Error(label + (last ? ": " + last.message : ""));
}
test(
  "library pagination survives refresh and transient errors; explicit sharing, quiet time and status are usable",
  { timeout: 240000 },
  async (context) => {
    const fixture = await mkdtemp(join(tmpdir(), "pkb-library-ui-")),
      home = join(fixture, "data"),
      sources = join(fixture, "Team notes");
    await mkdir(home);
    await mkdir(sources);
    const shots =
      process.env.PKB_RUNTIME_ARTIFACTS ||
      (await mkdtemp(join(tmpdir(), "pkb-runtime-ui-shots-")));
    await mkdir(shots, { recursive: true });
    await Promise.all(
      Array.from({ length: 205 }, (_, index) =>
        writeFile(
          join(sources, "note-" + String(index).padStart(3, "0") + ".md"),
          "# Team note " +
            index +
            "\nLIBRARY_NOTE_" +
            index +
            " synthetic acceptance evidence.",
        ),
      ),
    );
    const app = await electron.launch({
      executablePath: executable,
      args: process.env.PKB_PACKAGED_APP ? [] : [resolve(".")],
      env: { ...process.env, PKB_HOME: home, TOKIO_WORKER_THREADS: "1" },
      timeout: 30000,
    });
    const errors = [];
    try {
      const page = await app.firstWindow();
      page.on("pageerror", (error) => errors.push(error.message));
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
        file_quiet_seconds: 0,
      });
      await api("POST", "/roots", {
        path: sources,
        policy: "agents",
        name: "Team notes",
      });
      await until(
        async () => (await api("GET", "/state")).stats.ready === 205,
        "all 205 real files finish indexing",
        150000,
      );
      await page.getByRole("button", { name: "资料", exact: true }).click();
      const rows = page.locator(".file-table-row");
      await until(
        async () => (await rows.count()) === 100,
        "first page contains 100 files",
      );
      await app.evaluate(() => {
        globalThis.__delayLibraryRead = true;
        globalThis.__delayedLibraryReads = 0;
      });
      await interceptRequests(app, async ({ path }, next) => {
        if (globalThis.__delayLibraryRead && path.startsWith("/documents?")) {
          globalThis.__delayedLibraryReads++;
          await new Promise((resolve) => setTimeout(resolve, 3000));
        }
        return next();
      });
      await writeFile(
        join(sources, "note-001.md"),
        "LIBRARY_BACKGROUND_REFRESH before expanding",
      );
      await until(
        () => app.evaluate(() => globalThis.__delayedLibraryReads > 0),
        "background list read is deliberately slow",
      );
      assert.equal(
        await page
          .getByRole("button", { name: "加载更多", exact: true })
          .isEnabled(),
        true,
        "background refresh must not block user pagination",
      );
      await app.evaluate(() => {
        globalThis.__delayLibraryRead = false;
      });
      await page.getByRole("button", { name: "加载更多", exact: true }).click();
      await until(
        async () => (await rows.count()) === 200,
        "second page contains 200 unique files",
      );
      await page.getByRole("button", { name: "加载更多", exact: true }).click();
      await until(
        async () => (await rows.count()) === 205,
        "last page completes the list",
      );
      await writeFile(
        join(sources, "new-note.md"),
        "NEW_LIBRARY_NOTE added after expanding pages",
      );
      await until(
        async () => (await rows.count()) === 206,
        "background refresh preserves the expanded list",
        30000,
      );
      assert.equal(
        new Set(await rows.locator(".file-name strong").allTextContents()).size,
        206,
      );
      await app.evaluate(({ ipcMain }) => {
        const handler = ipcMain._invokeHandlers.get("knowledge:request");
        ipcMain.removeHandler("knowledge:request");
        globalThis.__failNextLibraryRead = true;
        ipcMain.handle("knowledge:request", (event, method, path, body) => {
          if (
            globalThis.__failNextLibraryRead &&
            method === "GET" &&
            path.startsWith("/documents?")
          ) {
            globalThis.__failNextLibraryRead = false;
            throw new Error("读取资料超过执行时限，请重试或查看运行状态。");
          }
          return handler(event, method, path, body);
        });
      });
      await writeFile(
        join(sources, "note-000.md"),
        "LIBRARY_NOTE_CHANGED force one ordinary background refresh",
      );
      const alert = page
        .getByRole("alert")
        .filter({ hasText: "已保留上次加载的资料" });
      await alert.waitFor({ timeout: 30000 });
      assert.equal(
        await rows.count(),
        206,
        "a transient read failure must preserve visible data",
      );
      assert.doesNotMatch(
        await alert.innerText(),
        /Error invoking remote method/,
      );
      await alert.getByRole("button", { name: "重试", exact: true }).click();
      await alert.waitFor({ state: "hidden" });
      assert.equal(await rows.count(), 206);
      await page.setViewportSize({ width: 1440, height: 1050 });
      await page.locator(".runtime-status summary").click();
      await page.getByText("预留全文通道", { exact: true }).waitFor();
      await page.evaluate(() => window.scrollTo(0, 0));
      await page.screenshot({ path: join(shots, "runtime-status.png") });
      await page.setViewportSize({ width: 640, height: 800 });
      await page.screenshot({ path: join(shots, "runtime-status-narrow.png") });
      assert.ok(
        await page.evaluate(
          () =>
            document.documentElement.scrollWidth <=
            document.documentElement.clientWidth,
        ),
        "status has no narrow-window horizontal overflow",
      );
      await page.setViewportSize({ width: 1440, height: 1050 });
      await page
        .getByRole("button", { name: "Agent 连接", exact: true })
        .click();
      await page.getByRole("button", { name: "添加连接", exact: true }).click();
      const dialog = page.getByRole("dialog", { name: "添加 Agent 连接" });
      await dialog
        .getByLabel("连接名称", { exact: true })
        .fill("Team research agent");
      assert.equal(
        await dialog.getByLabel("共享范围", { exact: true }).inputValue(),
        "",
      );
      assert.equal(
        await dialog
          .getByRole("button", { name: "生成连接", exact: true })
          .isDisabled(),
        true,
      );
      await page.screenshot({ path: join(shots, "sharing-choice.png") });
      await dialog.getByLabel("共享范围", { exact: true }).selectOption("all");
      await app.evaluate(() => {
        globalThis.__connectionPending = false;
      });
      await interceptRequests(app, async ({ method, path }, next) => {
        if (method === "POST" && path === "/connections") {
          globalThis.__connectionPending = true;
          await new Promise((resolve) => {
            globalThis.__releaseConnection = resolve;
          });
        }
        return next();
      });
      await dialog
        .getByRole("button", { name: "生成连接", exact: true })
        .click();
      await until(
        () => app.evaluate(() => globalThis.__connectionPending),
        "connection submission is in flight",
      );
      for (const label of [
        "连接名称",
        "客户端",
        "共享范围",
        "共享已确认的个人记忆",
        "允许 Agent 提交记忆建议，等待我确认",
      ])
        assert.equal(
          await dialog.getByLabel(label, { exact: true }).isDisabled(),
          true,
          "submitted permissions cannot be edited in flight",
        );
      await app.evaluate(() => globalThis.__releaseConnection());
      await page
        .getByRole("heading", { name: "接入 Agent", exact: true })
        .waitFor();
      assert.equal(
        (await api("GET", "/state")).connections[0].scope_mode,
        "all",
      );
      await page.keyboard.press("Escape");
      await page.getByRole("button", { name: "设置", exact: true }).click();
      await page.getByRole("tab", { name: "模型与检索", exact: true }).click();
      await page
        .getByLabel("连接方式", { exact: true })
        .first()
        .selectOption("direct");
      await page.getByRole("button", { name: "保存设置", exact: true }).click();
      await until(
        async () =>
          (await api("GET", "/state")).settings.chat.proxy_mode === "direct",
        "direct model connection is saved",
      );
      await page.screenshot({ path: join(shots, "model-network.png") });
      await page.getByRole("tab", { name: "索引与后台", exact: true }).click();
      await page
        .getByLabel("修改后的静默时间", { exact: true })
        .selectOption("60");
      await page.getByRole("button", { name: "保存设置", exact: true }).click();
      await until(
        async () =>
          (await api("GET", "/state")).settings.file_quiet_seconds === 60,
        "quiet-time preference is saved",
      );
      await page.screenshot({ path: join(shots, "indexing-policy.png") });
      assert.deepEqual(errors, []);
      context.diagnostic(
        "206 files, pagination, background refresh, recoverable read failure, shared-range choice, proxy preference, quiet window and status checked. Screenshots: " +
          shots,
      );
    } finally {
      await app
        .evaluate(() => globalThis.__releaseConnection?.())
        .catch(() => {});
      let pid;
      try {
        pid = JSON.parse(
          await readFile(join(home, "service.json"), "utf8"),
        ).pid;
      } catch {}
      await app.close();
      if (pid)
        await until(
          () => {
            try {
              process.kill(pid, 0);
              return false;
            } catch {
              return true;
            }
          },
          "UI exits and releases the engine",
          10000,
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
