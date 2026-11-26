import test from "node:test";
import assert from "node:assert/strict";
import {
  mkdtemp,
  mkdir,
  writeFile,
  readFile,
  access,
  rm,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import electronBinary from "electron";
import { _electron as electron } from "playwright";

async function until(fn, label, timeout = 25000) {
  const start = Date.now();
  let error;
  while (Date.now() - start < timeout) {
    try {
      const value = await fn();
      if (value) return value;
    } catch (caught) {
      error = caught;
    }
    await delay(150);
  }
  throw new Error(label + (error ? ": " + error.message : ""));
}
const screenshots =
  process.env.PKB_UI_ARTIFACTS ||
  (await mkdtemp(join(tmpdir(), "pkb-storage-ui-")));

test(
  "desktop storage controls migrate, restart, release old data and preserve filesystem indexing",
  { timeout: 120000 },
  async (context) => {
    const directory = await mkdtemp(join(tmpdir(), "pkb-desktop-storage-"));
    const home = join(directory, "old-data");
    const sources = join(directory, "source-documents");
    const destination = join(directory, "chosen-volume");
    await Promise.all([
      mkdir(home),
      mkdir(sources),
      mkdir(destination),
      mkdir(screenshots, { recursive: true }),
    ]);
    await writeFile(
      join(sources, "reference.md"),
      "# STORAGE_REFERENCE\nA synthetic source document for storage migration.",
    );
    const executablePath = process.env.PKB_PACKAGED_APP || electronBinary;
    const launch = () =>
      electron.launch({
        executablePath,
        args: process.env.PKB_PACKAGED_APP ? [] : [resolve(".")],
        env: { ...process.env, PKB_HOME: home },
        timeout: 30000,
      });
    let app = await launch();
    let info;
    try {
      let page = await app.firstWindow();
      await page.waitForFunction(() => !!window.knowledge);
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      page.on("console", (message) => {
        if (message.type() === "error") errors.push(message.text());
      });
      let api = (method, path, body) =>
        page.evaluate(
          ({ method, path, body }) =>
            window.knowledge.request(method, path, body),
          { method, path, body },
        );
      await api("PATCH", "/settings", {
        onboarding_complete: true,
        embedding_mode: "disabled",
      });
      await api("POST", "/roots", { path: sources, policy: "agents" });
      await until(async () => {
        const value = await api("GET", "/state");
        return (
          value.documents.length === 1 && value.documents[0].status === "ready"
        );
      }, "source is indexed");
      await page.setViewportSize({ width: 1536, height: 1024 });
      await page.getByRole("button", { name: "设置", exact: true }).click();
      await page.getByRole("tab", { name: "个性化", exact: true }).click();
      await page.getByRole("radio", { name: "深色", exact: true }).check();
      await until(
        () =>
          page.evaluate(
            () => document.documentElement.dataset.theme === "dark",
          ),
        "dark appearance applies",
      );
      assert.equal(
        await app.evaluate(
          ({ nativeTheme }) => nativeTheme.shouldUseDarkColors,
        ),
        true,
      );
      await page
        .getByLabel("自定义主题颜色", { exact: true })
        .evaluate((input) => {
          Object.getOwnPropertyDescriptor(
            HTMLInputElement.prototype,
            "value",
          ).set.call(input, "#f4dacc");
          input.dispatchEvent(new Event("input", { bubbles: true }));
          input.dispatchEvent(new Event("change", { bubbles: true }));
        });
      await until(
        async () => (await api("GET", "/appearance")).accent === "#f4dacc",
        "custom color picker saves through the UI",
      );
      await page.getByRole("radio", { name: "海蓝", exact: true }).check();
      await page.getByRole("radio", { name: "大", exact: true }).check();
      await page.getByLabel("阅读字体", { exact: true }).selectOption("serif");
      await page.getByRole("radio", { name: "紧凑", exact: true }).check();
      await page.getByRole("switch", { name: "收起侧栏", exact: true }).check();
      await until(async () => {
        const value = await api("GET", "/appearance");
        return (
          value.theme === "dark" &&
          value.accent === "blue" &&
          value.font_size === 16 &&
          value.reading_font === "serif" &&
          value.density === "compact" &&
          value.sidebar_collapsed
        );
      }, "all appearance selections are saved");
      assert.equal(
        await page.evaluate(
          () => getComputedStyle(document.documentElement).fontSize,
        ),
        "16px",
      );
      await page.screenshot({ path: join(screenshots, "appearance-dark.png") });
      await page.getByRole("radio", { name: "跟随系统", exact: true }).check();
      await page.emulateMedia({ colorScheme: "dark" });
      await until(
        () =>
          page.evaluate(
            () => document.documentElement.dataset.theme === "dark",
          ),
        "system dark appearance",
      );
      await page.emulateMedia({ colorScheme: "light" });
      await until(
        () =>
          page.evaluate(
            () => document.documentElement.dataset.theme === "light",
          ),
        "system light appearance",
      );
      await page
        .getByRole("button", { name: "恢复默认外观", exact: true })
        .click();
      await until(async () => {
        const value = await api("GET", "/appearance");
        return (
          value.theme === "light" &&
          value.accent === "graphite" &&
          value.font_size === 14 &&
          value.reading_font === "sans" &&
          value.density === "comfortable" &&
          !value.sidebar_collapsed
        );
      }, "reset restores all defaults");
      await page.getByRole("tab", { name: "模型与检索", exact: true }).click();
      const modelName = page.getByLabel("模型名称", { exact: true }).first();
      await modelName.fill("unsaved-company-model");
      await page.getByRole("tab", { name: "个性化", exact: true }).click();
      await page.getByRole("radio", { name: "松绿", exact: true }).check();
      await page.getByRole("tab", { name: "模型与检索", exact: true }).click();
      assert.equal(
        await page.getByLabel("模型名称", { exact: true }).first().inputValue(),
        "unsaved-company-model",
        "tab changes preserve unsaved provider fields",
      );
      await page.getByLabel("模型名称", { exact: true }).first().fill("");
      await page.getByRole("tab", { name: "个性化", exact: true }).click();
      await page
        .getByRole("button", { name: "恢复默认外观", exact: true })
        .click();
      await page.getByText("已自动保存", { exact: true }).waitFor();
      await page.screenshot({
        path: join(screenshots, "appearance-light.png"),
      });
      await page.setViewportSize({ width: 390, height: 844 });
      await page.evaluate(() => window.scrollTo(0, 0));
      await page.screenshot({
        path: join(screenshots, "appearance-mobile.png"),
      });
      assert.ok(
        await page.evaluate(
          () =>
            document.documentElement.scrollWidth <=
            document.documentElement.clientWidth,
        ),
        "appearance has no mobile horizontal overflow",
      );
      await page.setViewportSize({ width: 1536, height: 1024 });
      await page.getByRole("tab", { name: "索引与后台", exact: true }).click();
      await page
        .getByLabel("索引资源占用", { exact: true })
        .selectOption("low");
      await page.getByRole("button", { name: "保存设置", exact: true }).click();
      await page.getByRole("tab", { name: "存储与备份", exact: true }).click();
      await page
        .getByLabel("至少保留可用空间（GB）", { exact: true })
        .fill("6");
      await page.getByLabel("索引容量上限（GB）", { exact: true }).fill("12");
      await page
        .getByRole("button", { name: "保存存储限制", exact: true })
        .click();
      await until(async () => {
        const value = await api("GET", "/state");
        return (
          value.settings.min_free_disk_gb === 6 &&
          value.settings.max_index_gb === 12 &&
          value.settings.indexing_profile === "low"
        );
      }, "storage and CPU limits are persisted");
      await page
        .locator(".settings-section")
        .filter({
          has: page.getByRole("heading", {
            name: "存储位置与容量",
            exact: true,
          }),
        })
        .evaluate((element) => element.scrollIntoView({ block: "start" }));
      await page.locator(".toast").waitFor({ state: "hidden" });
      await page.screenshot({
        path: join(screenshots, "storage-settings.png"),
      });
      await app.evaluate(({ dialog, app }, parent) => {
        dialog.showOpenDialog = async () => ({
          canceled: false,
          filePaths: [parent],
        });
        app.relaunch = () => {};
      }, destination);
      await page
        .getByRole("button", { name: "迁移到其他位置", exact: true })
        .click();
      const modal = page.getByRole("dialog", {
        name: "迁移知识库存储位置",
        exact: true,
      });
      await modal.waitFor();
      await page.screenshot({
        path: join(screenshots, "storage-migration.png"),
      });
      await api("PATCH", "/appearance", {
        theme: "dark",
        accent: "#f4dacc",
        font_size: 16,
        reading_font: "serif",
        density: "compact",
        sidebar_collapsed: true,
      });
      const closed = app.waitForEvent("close");
      await modal
        .getByRole("button", { name: "迁移并重启", exact: true })
        .click();
      await closed;
      const chosen = join(destination, "Personal-Knowledge");
      await access(join(chosen, "knowledge.sqlite"));
      app = await launch();
      page = await app.firstWindow();
      await page.waitForFunction(() => !!window.knowledge);
      page.on("pageerror", (error) => errors.push(error.message));
      page.on("console", (message) => {
        if (message.type() === "error") errors.push(message.text());
      });
      api = (method, path, body) =>
        page.evaluate(
          ({ method, path, body }) =>
            window.knowledge.request(method, path, body),
          { method, path, body },
        );
      const state = await api("GET", "/state");
      assert.equal(state.documents.length, 1);
      assert.equal(state.settings.paused, true);
      assert.ok(state.data_directory.endsWith("Personal-Knowledge"));
      assert.deepEqual(
        await api("GET", "/appearance"),
        {
          theme: "dark",
          accent: "#f4dacc",
          font_size: 16,
          reading_font: "serif",
          density: "compact",
          sidebar_collapsed: true,
        },
        "all appearance choices survive real data migration and relaunch",
      );
      await until(
        () =>
          page.evaluate(
            () =>
              document.documentElement.dataset.theme === "dark" &&
              getComputedStyle(document.documentElement).fontSize === "16px",
          ),
        "persisted appearance is applied after restart",
      );
      assert.equal(
        await page.evaluate(() =>
          getComputedStyle(document.documentElement).getPropertyValue(
            "--on-primary",
          ),
        ),
        "#171719",
        "light custom colors use readable dark text",
      );
      await page.getByRole("button", { name: "设置", exact: true }).click();
      await page
        .getByRole("button", { name: "恢复默认外观", exact: true })
        .click();
      await page.getByText("已自动保存", { exact: true }).waitFor();
      await page.setViewportSize({ width: 1536, height: 1024 });
      await assert.rejects(access(join(home, "knowledge.sqlite")));
      await access(join(sources, "reference.md"));
      await page.getByRole("button", { name: "资料", exact: true }).click();
      await page
        .getByRole("region", { name: "索引处理进度", exact: true })
        .waitFor();
      await page.getByRole("button", { name: "继续同步", exact: true }).click();
      await writeFile(
        join(sources, "after-move.md"),
        "# AFTER_MOVE_REFERENCE\nWatching resumes from the migrated knowledge base.",
      );
      await until(async () => {
        const value = await api("GET", "/state");
        return (
          value.documents.length === 2 &&
          value.documents.every((document) => document.status === "ready")
        );
      }, "new files are watched after migration");
      await page.screenshot({ path: join(screenshots, "index-progress.png") });
      await page.setViewportSize({ width: 800, height: 900 });
      await page.screenshot({
        path: join(screenshots, "index-progress-narrow.png"),
      });
      assert.ok(
        await page.evaluate(
          () =>
            document.documentElement.scrollWidth <=
            document.documentElement.clientWidth,
        ),
        "no horizontal overflow at narrow width",
      );
      assert.deepEqual(errors, []);
      info = JSON.parse(await readFile(join(chosen, "service.json"), "utf8"));
      const exited = app.waitForEvent("close");
      await app.evaluate(({ app }) => app.quit());
      await exited;
      await until(
        () => {
          try {
            process.kill(info.pid, 0);
            return false;
          } catch {
            return true;
          }
        },
        "normal quit releases the engine",
        8000,
      );
      context.diagnostic("Screenshots: " + screenshots);
    } finally {
      await app.close().catch(() => {});
      await rm(directory, { recursive: true, force: true });
    }
  },
);
