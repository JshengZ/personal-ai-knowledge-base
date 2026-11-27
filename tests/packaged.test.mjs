import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, rm, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { _electron as electron } from "playwright";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { PDFDocument, StandardFonts } from "pdf-lib";
import { createCanvas } from "@napi-rs/canvas";
import ExcelJS from "exceljs";

async function until(callback, label, timeout = 90000) {
  const start = Date.now();
  while (Date.now() - start < timeout) {
    const value = await callback();
    if (value) return value;
    await delay(200);
  }
  throw new Error("Timed out: " + label);
}

test(
  "installed app: native setup, all bundled parsers, local RAG, project filters and MCP",
  { timeout: 480000 },
  async () => {
    const fixture = await mkdtemp(join(tmpdir(), "pkb-installed-"));
    const home = join(fixture, "data");
    await mkdir(home);
    const first = join(fixture, "推荐排序");
    const second = join(fixture, "订单校验");
    await mkdir(first);
    await mkdir(second);
    await writeFile(
      join(first, "排序实验.md"),
      "# 推荐排序\n使用 LightGBM LambdaRank。NDCG@10 为 0.462。\n" +
        "推荐排序的长文本验证，保留项目背景与原文出处。".repeat(130),
    );
    await writeFile(
      join(second, "订单设计.md"),
      "# 订单校验\n使用幂等键避免重复订单。技术决定需保留原文依据。\n",
    );
    const pdf = await PDFDocument.create();
    const font = await pdf.embedFont(StandardFonts.Helvetica);
    pdf.addPage([600, 800]).drawText("PACKAGED PDF CITATION CHECK", {
      x: 30,
      y: 700,
      font,
      size: 16,
    });
    await writeFile(join(second, "包装验证.pdf"), await pdf.save());
    const canvas = createCanvas(1500, 240);
    const context = canvas.getContext("2d");
    context.fillStyle = "white";
    context.fillRect(0, 0, 1500, 240);
    context.fillStyle = "black";
    context.font = "70px Arial";
    context.fillText("PACKAGED OCR CHECK", 50, 140);
    await writeFile(join(second, "扫描资料.png"), canvas.toBuffer("image/png"));
    const workbook = new ExcelJS.Workbook();
    const sheet = workbook.addWorksheet("校验效果");
    sheet.addRow(["项目", "效果"]);
    sheet.addRow(["订单", 0.462]);
    sheet.addRow(["差值", { formula: "B2-0.438", result: 0.024 }]);
    await writeFile(
      join(second, "效果表.xlsx"),
      await workbook.xlsx.writeBuffer(),
    );
    const executablePath =
      process.env.PKB_PACKAGED_APP ||
      (process.platform === "win32"
        ? resolve("release/win-x64/win-unpacked/Personal-Knowledge.exe")
        : resolve(
            "release/mac-" +
              process.arch +
              "/mac/个人知识库.app/Contents/MacOS/个人知识库",
          ));
    const app = await electron.launch({
      executablePath,
      env: { ...process.env, PKB_HOME: home },
      timeout: 30000,
    });
    let client;
    try {
      const page = await app.firstWindow();
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      page.on("console", (message) => {
        if (message.type() === "error") errors.push(message.text());
      });
      await app.evaluate(({ dialog }, path) => {
        dialog.showOpenDialog = async () => ({
          canceled: false,
          filePaths: [path],
        });
      }, first);
      await page.getByRole("button", { name: "跳过导览", exact: true }).click();
      await page
        .getByRole("button", { name: "添加资料目录", exact: true })
        .click();
      const initialFolder = page.getByRole("dialog", { name: "添加资料目录" });
      await initialFolder
        .getByRole("button", { name: "选择目录", exact: true })
        .click();
      await initialFolder
        .getByRole("button", { name: "加入知识库", exact: true })
        .click();
      await initialFolder.waitFor({ state: "hidden" });
      const api = (method, path, body) =>
        page.evaluate(
          ({ method, path, body }) =>
            window.knowledge.request(method, path, body),
          { method, path, body },
        );
      await api("POST", "/roots", { path: second, policy: "agents" });
      let lastProgress = 0;
      let lastState;
      let state;
      try {
        state = await until(
          async () => {
            const value = await api("GET", "/state");
            lastState = value;
            if (Date.now() - lastProgress >= 15000) {
              lastProgress = Date.now();
              console.log(
                JSON.stringify({
                  packagedIndexProgress: true,
                  documents: value.documents.map(
                    ({ title, status, error }) => ({ title, status, error }),
                  ),
                  stats: value.stats,
                  semantic: value.semantic_progress,
                  embeddingBusy: value.embedding_busy,
                  semanticError: value.semantic_error,
                }),
              );
            }
            assert.ok(
              !value.documents.some((document) => document.status === "error"),
              "fixture documents must parse successfully",
            );
            return value.documents.length === 5 &&
              value.documents.every(
                (document) => document.status === "ready",
              ) &&
              value.stats.vectors >= value.stats.chunks &&
              !value.embedding_busy
              ? value
              : false;
          },
          "five real documents parsed and embedded",
          process.platform === "win32" ? 360000 : 90000,
        );
      } catch (error) {
        console.error(
          JSON.stringify({
            packagedIndexFailure: true,
            documents: lastState?.documents.map(({ title, status, error }) => ({
              title,
              status,
              error,
            })),
            stats: lastState?.stats,
            semantic: lastState?.semantic_progress,
            semanticError: lastState?.semantic_error,
          }),
        );
        const log = await readFile(join(home, "service.log"), "utf8").catch(
          () => "Service log unavailable",
        );
        console.error(log.slice(-16000));
        throw error;
      }
      assert.equal(state.projects.length, 2);
      const project = state.projects.find(
        (project) => project.name === "订单校验",
      );
      const order = state.documents.find(
        (document) => document.title === "订单设计.md",
      );
      const image = state.documents.find(
        (document) => document.title === "扫描资料.png",
      );
      const ocr = await api("GET", "/sources/" + image.id);
      assert.match(
        ocr.chunks.map((chunk) => chunk.text).join("\n"),
        /PACKAGED.*OCR/i,
      );
      const spreadsheet = state.documents.find(
        (document) => document.title === "效果表.xlsx",
      );
      const cells = await api("GET", "/sources/" + spreadsheet.id);
      assert.equal(cells.chunks[0].location.sheet, "校验效果");
      assert.match(cells.chunks[0].text, /0\.024/);
      const source = state.documents.find(
        (document) => document.title === "包装验证.pdf",
      );
      const pages = await api("GET", "/sources/" + source.id);
      assert.equal(pages.chunks[0].location.page, 1);
      await page.getByRole("button", { name: "资料", exact: true }).click();
      await page.locator("#file-project").selectOption(project.id);
      await until(
        async () =>
          (await page.locator("#file-project").inputValue()) === project.id &&
          (await page.locator(".file-table-row .file-name").count()) === 4 &&
          (await page
            .locator(".file-name")
            .filter({ hasText: "排序实验.md" })
            .count()) === 0,
        "project filter narrows the document list",
      );
      await page
        .locator("button.file-name")
        .filter({ hasText: "订单设计.md" })
        .waitFor();
      assert.equal(
        await page
          .locator("button.file-name")
          .filter({ hasText: "排序实验.md" })
          .count(),
        0,
      );
      await page.getByRole("button", { name: "关于我", exact: true }).click();
      await page.getByRole("button", { name: "添加记忆", exact: true }).click();
      const dialog = page.getByRole("dialog");
      await dialog
        .getByLabel("记忆内容", { exact: true })
        .fill("回答技术问题时，请保留原文依据。");
      await dialog
        .getByRole("button", { name: "保存并确认", exact: true })
        .click();
      await dialog.waitFor({ state: "hidden" });
      await page
        .getByRole("button", { name: "Agent 连接", exact: true })
        .click();
      await page.getByRole("button", { name: "添加连接", exact: true }).click();
      await dialog.getByLabel("客户端", { exact: true }).selectOption("other");
      await dialog
        .getByLabel("连接名称", { exact: true })
        .fill("安装包验收 Agent");
      await dialog
        .getByLabel("共享范围", { exact: true })
        .selectOption("selected");
      await dialog.getByLabel("推荐排序", { exact: true }).uncheck();
      assert.ok(
        await dialog.getByLabel("订单校验", { exact: true }).isChecked(),
      );
      await dialog.getByLabel("共享已确认的个人记忆").check();
      await dialog.getByLabel("允许 Agent 提交记忆建议，等待我确认").check();
      await dialog
        .getByRole("button", { name: "生成连接", exact: true })
        .click();
      await page
        .getByRole("heading", { name: "接入 Agent", exact: true })
        .waitFor();
      await dialog
        .getByRole("button", { name: "Codex TOML", exact: true })
        .click();
      assert.match(await dialog.locator("pre").innerText(), /mcp_servers/);
      await page.keyboard.press("Escape");
      const connections = await api("GET", "/state");
      const config = await api(
        "GET",
        "/connections/" + connections.connections[0].id + "/config",
      );
      client = new Client({ name: "packaged-smoke", version: "1.0.1" });
      await client.connect(
        new StdioClientTransport({
          command: config.server.command,
          args: config.server.args,
          env: { ...process.env, ...config.server.env },
          stderr: "pipe",
        }),
      );
      assert.equal((await client.listTools()).tools.length, 5);
      const result = await client.callTool({
        name: "get_context",
        arguments: { query: "如何避免重复订单" },
      });
      assert.notEqual(result.isError, true);
      const recalled = JSON.parse(result.content[0].text);
      assert.equal(recalled.mode, "hybrid");
      assert.equal(recalled.projects.length, 1);
      assert.ok(recalled.sources.some((hit) => hit.document_id === order.id));
      assert.equal(recalled.confirmed_memories.length, 1);
      assert.ok(recalled.sources.every((hit) => hit.project_id === project.id));
      const proposal = await client.callTool({
        name: "propose_memory",
        arguments: {
          content: "有订单幂等校验设计经验",
          category: "experience",
          project_id: project.id,
          source_ids: [order.id],
        },
      });
      assert.equal(JSON.parse(proposal.content[0].text).status, "pending");
      await page.getByRole("button", { name: /^关于我/ }).click();
      await page.getByText("有订单幂等校验设计经验", { exact: true }).waitFor();
      await page.getByRole("button", { name: "确认", exact: true }).click();
      await page.getByRole("button", { name: /^已确认/ }).click();
      await page.getByText("有订单幂等校验设计经验", { exact: true }).waitFor();
      const manual = page
        .locator(".memory-row")
        .filter({ hasText: "回答技术问题时，请保留原文依据。" });
      await manual
        .getByRole("button", { name: "删除记忆", exact: true })
        .click();
      await dialog.getByRole("button", { name: "删除", exact: true }).click();
      await dialog.waitFor({ state: "hidden" });
      await manual.waitFor({ state: "hidden" });
      assert.deepEqual(errors, []);
      console.log(
        JSON.stringify({
          packaged: true,
          documents: 5,
          localVectors: state.stats.vectors,
          projects: 2,
          pdf: true,
          ocr: true,
          xlsx: true,
          mcp: true,
          memoryReview: true,
          consoleErrors: errors,
        }),
      );
    } finally {
      if (client) await client.close();
      await app.close();
      await until(
        () =>
          readFile(join(home, "service.json")).then(
            () => false,
            () => true,
          ),
        "app exit must finish shutting down its backend",
        15000,
      );
      await rm(fixture, { recursive: true, force: true });
    }
  },
);
