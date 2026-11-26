import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import electronBinary from "electron";
import { _electron as electron } from "playwright";
import { createServer } from "node:http";

async function until(fn, label, timeout = 15000) {
  let last;
  const start = Date.now();
  while (Date.now() - start < timeout) {
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
  "optional real-UI guide preserves an empty library, persists dismissal and retains drafts on replay",
  { timeout: 120000 },
  async (context) => {
    const directory = await mkdtemp(join(tmpdir(), "pkb-onboarding-"));
    const home = join(directory, "data");
    await mkdir(home);
    const shots =
      process.env.PKB_TOUR_ARTIFACTS ||
      (await mkdtemp(join(tmpdir(), "pkb-guide-ui-")));
    await mkdir(shots, { recursive: true });
    const launch = () =>
      electron.launch({
        executablePath: process.env.PKB_PACKAGED_APP || electronBinary,
        args: process.env.PKB_PACKAGED_APP ? [] : [resolve(".")],
        env: { ...process.env, PKB_HOME: home },
        timeout: 30000,
      });
    let app = await launch();
    let page;
    const errors = [];
    const attach = async () => {
      page = await app.firstWindow();
      page.on("pageerror", (error) => errors.push(error.message));
      await page.waitForFunction(() => !!window.knowledge);
    };
    let api = async (method, path, body) =>
      page.evaluate(
        ({ method, path, body }) =>
          window.knowledge.request(method, path, body),
        { method, path, body },
      );
    const empty = (state) => {
      for (const key of [
        "roots",
        "projects",
        "documents",
        "memories",
        "connections",
      ])
        assert.equal(state[key].length, 0, key + " stays empty");
    };
    const quit = async () => {
      const info = JSON.parse(
        await readFile(join(home, "service.json"), "utf8"),
      );
      const closed = app.waitForEvent("close");
      await app.evaluate(({ app }) => app.quit());
      await closed;
      await until(
        () => {
          try {
            process.kill(info.pid, 0);
            return false;
          } catch {
            return true;
          }
        },
        "engine exits after guide test",
        10000,
      );
    };
    try {
      await attach();
      await page.getByRole("dialog", { name: "开始使用个人知识库" }).waitFor();
      empty(await api("GET", "/state"));
      await page.getByRole("button", { name: "跳过导览", exact: true }).click();
      await until(
        async () => (await api("GET", "/onboarding")).status === "skipped",
        "skip is saved",
      );
      assert.equal(
        (await api("GET", "/state")).settings.onboarding_complete,
        true,
      );
      empty(await api("GET", "/state"));
      await quit();
      app = await launch();
      await attach();
      await page
        .getByRole("heading", { name: "资料与同步", exact: true })
        .waitFor();
      assert.equal(
        await page.getByRole("dialog").count(),
        0,
        "skipped welcome does not repeat after relaunch",
      );
      await app.evaluate(({ dialog }) => {
        globalThis.__guideWrites = [];
        globalThis.__guideChoosers = 0;
        const original = globalThis.fetch;
        globalThis.fetch = async (input, options) => {
          if (options?.method && options.method !== "GET")
            globalThis.__guideWrites.push(new URL(String(input)).pathname);
          return original(input, options);
        };
        dialog.showOpenDialog = async () => {
          globalThis.__guideChoosers++;
          return { canceled: true, filePaths: [] };
        };
      });
      await page.setViewportSize({ width: 1212, height: 840 });
      await page.getByRole("button", { name: "使用指南", exact: true }).click();
      const steps = [
        "sources",
        "progress",
        "models",
        "agents",
        "search",
        "projects",
        "profile",
        "storage",
      ];
      for (let index = 0; index < steps.length; index++) {
        const card = page.locator(
          '.tour-card[data-step="' +
            steps[index] +
            '"][data-positioned="true"]',
        );
        await card.waitFor();
        await until(
          () =>
            card.evaluate((el) => {
              const r = el.getBoundingClientRect();
              return (
                r.x >= 15 &&
                r.y >= 15 &&
                r.right <= innerWidth - 15 &&
                r.bottom <= innerHeight - 15
              );
            }),
          "guide fits the viewport",
        );
        assert.equal(
          await page.locator(".tour-spotlight").count(),
          1,
          "step has a real target",
        );
        if (index === 0) {
          const box = await page
            .locator('[data-guide="add-source"]')
            .boundingBox();
          await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
          assert.equal(
            await page.getByRole("dialog").count(),
            1,
            "target stays observational during guide",
          );
        }
        if (index === 2 || index === 7)
          await page.screenshot({
            path: join(shots, "guide-" + steps[index] + ".png"),
          });
        if (index < steps.length - 1)
          await page
            .getByRole("button", { name: "下一步", exact: true })
            .click();
      }
      await page.getByRole("button", { name: "上一步", exact: true }).click();
      await page.locator('.tour-card[data-step="profile"]').waitFor();
      await page.keyboard.press("ArrowRight");
      await page.locator('.tour-card[data-step="storage"]').waitFor();
      await page.getByRole("button", { name: "完成导览", exact: true }).click();
      await until(
        async () => (await api("GET", "/onboarding")).status === "completed",
        "completion is saved",
      );
      empty(await api("GET", "/state"));
      assert.equal(await app.evaluate(() => globalThis.__guideChoosers), 0);
      assert.ok(
        (await app.evaluate(() => globalThis.__guideWrites)).every(
          (path) => path === "/api/onboarding",
        ),
        "tour only saves its own dismissal",
      );
      await page.getByRole("button", { name: "设置", exact: true }).click();
      await page.getByRole("tab", { name: "模型与检索", exact: true }).click();
      const model = page.getByLabel("模型名称", { exact: true }).first();
      const key = page.getByLabel("API Key", { exact: true }).first();
      await model.fill("guide-unsaved-model");
      await key.fill("synthetic-unsaved-guide-key");
      await page
        .getByRole("button", { name: "重新查看使用导览", exact: true })
        .click();
      await page.locator('.tour-card[data-positioned="true"]').waitFor();
      for (let n = 0; n < 12; n++) {
        await page.keyboard.press("Tab");
        assert.ok(
          await page.evaluate(
            () => !!document.activeElement?.closest(".product-tour"),
          ),
          "keyboard stays within guide controls",
        );
      }
      await page.keyboard.press("Escape");
      await page
        .getByRole("tab", { name: "模型与检索", exact: true })
        .waitFor();
      assert.equal(await model.inputValue(), "guide-unsaved-model");
      assert.equal(await key.inputValue(), "synthetic-unsaved-guide-key");
      assert.equal((await api("GET", "/state")).settings.chat.model, "");
      await app.evaluate(() => {
        const original = globalThis.fetch;
        let fail = true;
        globalThis.fetch = async (input, options) => {
          if (
            fail &&
            String(input).endsWith("/api/onboarding") &&
            options?.method === "PATCH"
          ) {
            fail = false;
            return new Response(
              JSON.stringify({ error: "Synthetic guide save failure" }),
              { status: 503, headers: { "Content-Type": "application/json" } },
            );
          }
          return original(input, options);
        };
      });
      await page.getByRole("button", { name: "使用指南", exact: true }).click();
      await page.locator('.tour-card[data-positioned="true"]').waitFor();
      await page
        .getByRole("button", { name: "退出使用指南", exact: true })
        .click();
      await page.getByText(/使用指南状态尚未保存/).waitFor();
      assert.equal(
        await page.getByRole("dialog").count(),
        0,
        "failed preference save cannot trap the user",
      );
      await page.getByRole("button", { name: "重试保存", exact: true }).click();
      await page
        .getByText(/使用指南状态尚未保存/)
        .waitFor({ state: "detached" });
      empty(await api("GET", "/state"));
      assert.equal(
        await page.evaluate(() => document.documentElement.style.overflow),
        "",
      );
      assert.deepEqual(errors, []);
      await api("PATCH", "/appearance", {
        theme: "dark",
        font_size: 18,
        density: "compact",
        sidebar_collapsed: true,
      });
      await page.setViewportSize({ width: 640, height: 520 });
      await page.getByRole("button", { name: "使用指南", exact: true }).click();
      await page.getByRole("button", { name: "下一步", exact: true }).click();
      const compactCard = page.locator(
        '.tour-card[data-step="progress"][data-positioned="true"]',
      );
      await until(
        () =>
          compactCard
            .locator(".tour-body")
            .evaluate((el) => el.clientHeight >= 60),
        "minimum window keeps the explanation readable",
      );
      await page.screenshot({ path: join(shots, "guide-minimum-window.png") });
      await page.keyboard.press("Escape");
      for (let round = 0; round < 30; round++) {
        await api("PATCH", "/settings", { onboarding_complete: false });
        await Promise.all([
          api("PATCH", "/settings", { min_free_disk_gb: 6 }),
          api("PATCH", "/onboarding", { status: "skipped" }),
        ]);
        assert.equal(
          (await api("GET", "/state")).settings.onboarding_complete,
          true,
          "concurrent settings save retains the guide flag",
        );
      }
      // A stale legacy settings snapshot cannot invalidate the independent
      // saved guide record used by a subsequent launch.
      await api("PATCH", "/settings", { onboarding_complete: false });
      await quit();
      app = await launch();
      await attach();
      await page
        .getByRole("heading", { name: "资料与同步", exact: true })
        .waitFor();
      assert.equal(await page.getByRole("dialog").count(), 0);
      empty(await api("GET", "/state"));
      await quit();
      context.diagnostic("Guide screenshots: " + shots);
    } finally {
      await app.close().catch(() => {});
      await rm(directory, { recursive: true, force: true });
    }
  },
);

test(
  "project questions update retained search fields and clear stale filters after a guide visit",
  { timeout: 90000 },
  async () => {
    const directory = await mkdtemp(join(tmpdir(), "pkb-guide-search-"));
    const home = join(directory, "data");
    const first = join(directory, "Project One");
    const second = join(directory, "Project Two");
    await Promise.all([mkdir(home), mkdir(first), mkdir(second)]);
    await writeFile(
      join(first, "notes.md"),
      "# 第一项目\n这是第一个项目的背景资料。",
    );
    await writeFile(
      join(second, "notes.md"),
      "# 第二项目\n新的项目问题对应第二个项目的背景资料。",
    );
    const model = createServer(async (request, response) => {
      for await (const chunk of request) {
      }
      response.setHeader("Content-Type", "application/json");
      response.end(
        JSON.stringify({
          choices: [{ message: { content: "第二个项目的回答。[1]" } }],
        }),
      );
    });
    await new Promise((done) => model.listen(0, "127.0.0.1", done));
    const app = await electron.launch({
      executablePath: process.env.PKB_PACKAGED_APP || electronBinary,
      args: process.env.PKB_PACKAGED_APP ? [] : [resolve(".")],
      env: { ...process.env, PKB_HOME: home },
      timeout: 30000,
    });
    let info;
    try {
      const page = await app.firstWindow();
      await page.waitForFunction(() => !!window.knowledge);
      const api = (method, path, body) =>
        page.evaluate(
          ({ method, path, body }) =>
            window.knowledge.request(method, path, body),
          { method, path, body },
        );
      await page.getByRole("button", { name: "跳过导览", exact: true }).click();
      await until(
        async () => (await api("GET", "/state")).settings.onboarding_complete,
        "skip before project fixture setup",
      );
      await api("PATCH", "/settings", {
        embedding_mode: "disabled",
        auto_summarize: false,
        chat: {
          base_url: "http://127.0.0.1:" + model.address().port + "/v1",
          model: "synthetic-chat",
        },
      });
      await api("POST", "/roots", { path: first, policy: "agents" });
      await api("POST", "/roots", { path: second, policy: "agents" });
      const snapshot = await until(async () => {
        const state = await api("GET", "/state");
        return state.documents.length === 2 &&
          state.documents.every((doc) => doc.status === "ready")
          ? state
          : false;
      }, "two source projects are ready");
      const project = snapshot.projects.find(
        (project) => project.name === "Project Two",
      );
      assert.ok(project);
      await app.evaluate(() => {
        globalThis.__guideAskCalls = [];
        const original = globalThis.fetch;
        globalThis.fetch = async (input, options) => {
          if (String(input).endsWith("/api/ask"))
            globalThis.__guideAskCalls.push(JSON.parse(options.body));
          return original(input, options);
        };
      });
      await page.getByRole("button", { name: "使用指南", exact: true }).click();
      for (let n = 0; n < 4; n++)
        await page.getByRole("button", { name: "下一步", exact: true }).click();
      await page
        .locator('.tour-card[data-step="search"][data-positioned="true"]')
        .waitFor();
      await page.keyboard.press("Escape");
      await page
        .getByRole("button", { name: "搜索与问答", exact: true })
        .click();
      await page.getByLabel("文件类型", { exact: true }).selectOption("pdf");
      await page.getByLabel("仅搜索原文", { exact: true }).check();
      await page
        .getByLabel("问题或关键词", { exact: true })
        .fill("旧的未提交问题");
      await page.getByRole("button", { name: "项目", exact: true }).click();
      await page
        .getByLabel("当前项目", { exact: true })
        .selectOption(project.id);
      await page.getByLabel("问这个项目", { exact: true }).fill("新的项目问题");
      await page.getByRole("button", { name: "提问", exact: true }).click();
      await page
        .locator(".answer p")
        .filter({ hasText: "第二个项目的回答" })
        .first()
        .waitFor();
      assert.equal(
        await page.getByLabel("问题或关键词", { exact: true }).inputValue(),
        "新的项目问题",
      );
      assert.equal(
        await page
          .getByLabel("项目范围", { exact: true })
          .filter({ visible: true })
          .inputValue(),
        project.id,
      );
      assert.equal(
        await page.getByLabel("文件类型", { exact: true }).inputValue(),
        "",
      );
      assert.equal(
        await page.getByLabel("仅搜索原文", { exact: true }).isChecked(),
        false,
      );
      let calls = await app.evaluate(() => globalThis.__guideAskCalls);
      assert.equal(calls.length, 1);
      assert.equal(calls[0].project_id, project.id);
      assert.equal(calls[0].extension, null);
      await page.getByRole("button", { name: "查询", exact: true }).click();
      await until(
        async () =>
          (await app.evaluate(() => globalThis.__guideAskCalls)).length === 2,
        "resubmitting uses the displayed question",
      );
      calls = await app.evaluate(() => globalThis.__guideAskCalls);
      assert.equal(calls[1].query, "新的项目问题");
      assert.equal(calls[1].project_id, project.id);
      info = JSON.parse(await readFile(join(home, "service.json"), "utf8"));
    } finally {
      await app.close().catch(() => {});
      if (info)
        await until(
          () => {
            try {
              process.kill(info.pid, 0);
              return false;
            } catch {
              return true;
            }
          },
          "search fixture engine exits",
          10000,
        ).catch(() => {});
      model.closeAllConnections();
      await new Promise((done) => model.close(done));
      await rm(directory, { recursive: true, force: true });
    }
  },
);
