import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { join, resolve, toNamespacedPath } from "node:path";
import { tmpdir } from "node:os";
import { setTimeout as delay } from "node:timers/promises";
import { createServer } from "node:http";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";

const version = JSON.parse(await readFile("package.json", "utf8")).version;
const binary = resolve(
  "engine/target/debug/knowledge-engine" +
    (process.platform === "win32" ? ".exe" : ""),
);
async function until(fn, label, timeout = 25000) {
  const start = Date.now();
  let error;
  while (Date.now() - start < timeout) {
    try {
      const value = await fn();
      if (value) return value;
    } catch (value) {
      error = value;
    }
    await delay(80);
  }
  throw new Error(label + (error ? ": " + error.message : ""));
}
async function fixture() {
  const directory = await mkdtemp(join(tmpdir(), "pkb-service-runtime-")),
    home = join(directory, "data"),
    sources = join(directory, "sources");
  await mkdir(home);
  await mkdir(sources);
  await writeFile(
    join(home, "runtime.json"),
    JSON.stringify({
      parser_runtime: process.execPath,
      parser_script: resolve("desktop/parser.mjs"),
      resources: resolve("resources"),
      mcp_executable: binary,
      electron_runtime: false,
    }),
  );
  let info,
    child,
    stderr = "";
  const api = async (
    method,
    path,
    body,
    signal = AbortSignal.timeout(35000),
  ) => {
    const response = await fetch(
      "http://127.0.0.1:" + info.port + "/api" + path,
      {
        method,
        headers: {
          Authorization: "Bearer " + info.token,
          "Content-Type": "application/json",
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal,
      },
    );
    const value = await response.json();
    if (!response.ok) throw new Error(JSON.stringify(value));
    return value;
  };
  const readInfo = async () =>
    JSON.parse(await readFile(join(home, "service.json"), "utf8"));
  async function start() {
    child = spawn(
      binary,
      ["serve", "--role", "service", "--home", toNamespacedPath(home)],
      {
        env: { ...process.env, TOKIO_WORKER_THREADS: "1" },
        stdio: ["ignore", "ignore", "pipe"],
      },
    );
    child.stderr.on("data", (bytes) => (stderr += bytes));
    info = await until(async () => {
      const value = await readInfo();
      return value.pid === child.pid && value;
    }, "service starts");
    await api("GET", "/health");
  }
  async function stop() {
    await api("POST", "/shutdown", {});
    await until(async () => {
      try {
        await readInfo();
        return false;
      } catch (error) {
        return error.code === "ENOENT";
      }
    }, "service files released");
    if (child)
      await until(
        () => child.exitCode !== null || child.signalCode !== null,
        "owned child exits",
      );
    assert.doesNotMatch(
      stderr,
      /panicked|runtime.*shut.*down/i,
      "all workers must finish before the runtime exits",
    );
  }
  async function stdio(config, name) {
    const client = new Client({ name, version });
    const transport = new StdioClientTransport({
      command: binary,
      args: [
        "mcp",
        "--role",
        "mcp-stdio",
        "--home",
        toNamespacedPath(home),
        "--connection",
        config.connection.id,
      ],
      env: { ...process.env, ...config.server.env },
      stderr: "pipe",
    });
    await client.connect(transport);
    assert.equal(client.getServerVersion().version, version);
    return { client, transport };
  }
  async function clean() {
    try {
      info = await readInfo();
      await stop();
    } catch {
      if (child?.exitCode === null) child.kill();
    }
    await rm(directory, {
      recursive: true,
      force: true,
      maxRetries: 20,
      retryDelay: 100,
    });
  }
  return {
    directory,
    home,
    sources,
    start,
    stop,
    api,
    readInfo,
    stdio,
    clean,
    adopt: async () => {
      info = await readInfo();
      child = undefined;
      return info;
    },
  };
}
function value(result) {
  assert.notEqual(result.isError, true, JSON.stringify(result));
  return JSON.parse(result.content[0].text);
}

test(
  "all MCP tools work after a real service shutdown; concurrent cold clients share one headless daemon and stdio exits on EOF",
  { timeout: 90000 },
  async (context) => {
    const f = await fixture();
    const clients = [];
    try {
      await f.start();
      await f.api("PATCH", "/settings", {
        embedding_mode: "disabled",
        auto_summarize: false,
      });
      await writeFile(
        join(f.sources, "evidence.md"),
        "HEADLESS_RUNTIME_NEEDLE cited source",
      );
      await f.api("POST", "/roots", { path: f.sources, policy: "agents" });
      const state = await until(async () => {
        const state = await f.api("GET", "/state");
        return state.documents[0]?.status === "ready" && state;
      }, "source ready");
      const implicit = await f.api("POST", "/connections", {
        name: "Explicit scope is required",
        kind: "other",
      });
      assert.equal(implicit.connection.scope_mode, "selected");
      assert.deepEqual(implicit.connection.project_ids, []);
      const config = await f.api("POST", "/connections", {
        name: "Headless test",
        kind: "other",
        scope_mode: "all",
        can_propose: true,
      });
      const http = new Client({ name: "runtime-http", version });
      clients.push({ client: http });
      await http.connect(
        new StreamableHTTPClientTransport(new URL(config.http.url), {
          requestInit: { headers: config.http.headers },
        }),
      );
      assert.equal(http.getServerVersion().version, version);
      await http.close();
      clients.length = 0;
      await f.stop();
      const cold = await Promise.all(
        [0, 1, 2].map((i) => f.stdio(config, "cold-" + i)),
      );
      clients.push(...cold);
      await assert.rejects(
        f.readInfo(),
        { code: "ENOENT" },
        "handshake must not spawn the daemon",
      );
      const started = Date.now();
      const projects = await Promise.all(
        cold.map(({ client }) =>
          client.callTool({ name: "list_projects", arguments: {} }).then(value),
        ),
      );
      assert.ok(projects.every((result) => result.projects.length === 1));
      const info = await f.adopt();
      assert.equal(info.version, version);
      assert.equal((await f.api("GET", "/status")).pid, info.pid);
      const client = cold[0].client,
        document = state.documents[0];
      assert.ok(
        value(
          await client.callTool({
            name: "search",
            arguments: { query: "HEADLESS_RUNTIME_NEEDLE" },
          }),
        ).hits.length,
      );
      assert.ok(
        value(
          await client.callTool({
            name: "get_context",
            arguments: { query: "HEADLESS_RUNTIME_NEEDLE" },
          }),
        ).sources.length,
      );
      assert.ok(
        value(
          await client.callTool({
            name: "read_source",
            arguments: { document_id: document.id },
          }),
        ).chunks.length,
      );
      const proposed = value(
        await client.callTool({
          name: "propose_memory",
          arguments: {
            content: "A headless MCP proposal",
            category: "experience",
            source_ids: [],
          },
        }),
      );
      assert.equal(proposed.status, "pending");
      for (const item of clients) {
        const pid = item.transport?.pid;
        await item.client.close();
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
            "stdio child exits on EOF",
            5000,
          );
      }
      clients.length = 0;
      assert.equal(
        (await f.api("GET", "/health")).ok,
        true,
        "the shared daemon remains available without a UI or stdio client",
      );
      context.diagnostic(
        "Three concurrent cold clients, all five tools, exact protocol version and EOF cleanup passed in " +
          (Date.now() - started) +
          " ms; daemon PID " +
          info.pid,
      );
    } finally {
      for (const { client } of clients) await client.close().catch(() => {});
      await f.clean();
    }
  },
);

test(
  "a slow write returns its committed proposal; retry is idempotent and audit separates computation from transport",
  { timeout: 65000 },
  async () => {
    const f = await fixture();
    let pair, locker;
    try {
      await f.start();
      await f.api("PATCH", "/settings", {
        embedding_mode: "disabled",
        auto_summarize: false,
      });
      const config = await f.api("POST", "/connections", {
        name: "Slow write",
        kind: "other",
        scope_mode: "all",
        can_propose: true,
      });
      pair = await f.stdio(config, "slow-write");
      let locked = "";
      locker = spawn(
        process.execPath,
        [
          "--input-type=module",
          "-e",
          "import{DatabaseSync}from'node:sqlite';const db=new DatabaseSync(process.argv[1]);db.exec('BEGIN IMMEDIATE');console.log('LOCKED');setTimeout(()=>{db.exec('COMMIT');db.close();},15000);",
          join(f.home, "knowledge.sqlite"),
        ],
        { stdio: ["ignore", "pipe", "pipe"] },
      );
      locker.stdout.on("data", (bytes) => (locked += bytes));
      await until(() => locked.includes("LOCKED"), "external writer acquired");
      // Queue a normal UI writer ahead of the MCP mutation, reproducing the old
      // ten-second false failure without a test-only hook in the service.
      const competing = f
        .api("POST", "/connections", {
          name: "Competing writer",
          kind: "other",
        })
        .catch(() => undefined);
      await delay(250);
      const started = Date.now(),
        args = {
          content: "SLOW_WRITE_DEFINITE_RESULT",
          category: "experience",
          source_ids: [],
        };
      const proposal = value(
        await pair.client.callTool({ name: "propose_memory", arguments: args }),
      );
      assert.ok(Date.now() - started > 10000);
      assert.equal(proposal.status, "pending");
      await competing;
      const repeated = value(
        await pair.client.callTool({ name: "propose_memory", arguments: args }),
      );
      assert.equal(repeated.id, proposal.id);
      await until(async () => {
        const events = (await f.api("GET", "/state")).events.filter(
          (event) =>
            event.kind === "mcp" && event.details.tool === "propose_memory",
        );
        return (
          events.some((event) => event.details.phase === "computed") &&
          events.some((event) => event.details.phase === "transport_complete")
        );
      }, "deferred audit is flushed");
      const state = await f.api("GET", "/state");
      assert.equal(
        state.memories.filter((item) => item.content === args.content).length,
        1,
      );
      assert.ok(
        state.events
          .filter((event) => event.kind === "mcp")
          .every((event) => event.details.client_acknowledged === null),
        "transport handoff is never asserted to be client acknowledgement",
      );
    } finally {
      if (pair) await pair.client.close().catch(() => {});
      if (locker && locker.exitCode === null)
        await new Promise((resolve) => locker.once("exit", resolve));
      await f.clean();
    }
  },
);

test(
  "native changes wait for quiet time, coalesce bursts and update without a full-directory rescan",
  { timeout: 60000 },
  async () => {
    const f = await fixture();
    try {
      await f.start();
      await f.api("PATCH", "/settings", {
        embedding_mode: "disabled",
        auto_summarize: false,
      });
      const path = join(f.sources, "draft.md");
      await writeFile(path, "INITIAL_QUIET_VERSION original");
      const root = await f.api("POST", "/roots", {
        path: f.sources,
        policy: "agents",
      });
      const original = await until(async () => {
        const state = await f.api("GET", "/state");
        return state.documents[0]?.status === "ready" && state.documents[0];
      }, "initial file ready");
      const before = (await f.api("GET", "/state")).roots.find(
        (item) => item.id === root.id,
      ).last_scan;
      await delay(1200);
      await writeFile(path, "QUIET_VERSION_SECOND edited");
      const waiting = await until(async () => {
        const status = await f.api("GET", "/status");
        return status.index.queue.waiting === 1 && status;
      }, "edit waits for the default quiet window");
      assert.equal(waiting.index.quiet_seconds, 300);
      assert.equal(waiting.index.reconcile_seconds, 3600);
      assert.ok(waiting.index.queue.next_due_at - Date.now() / 1000 > 290);
      await delay(1000);
      await writeFile(path, "QUIET_VERSION_THIRD edited again");
      await until(async () => {
        const status = await f.api("GET", "/status");
        return status.index.queue.next_due_at > waiting.index.queue.next_due_at;
      }, "later edit extends the quiet window");
      assert.equal(
        (await f.api("GET", "/state")).documents[0].version,
        original.version,
      );
      await f.api("PATCH", "/settings", { file_quiet_seconds: 1 });
      await writeFile(path, "QUIET_VERSION_FINAL settled content");
      await until(async () => {
        const state = await f.api("GET", "/state");
        return (
          state.documents[0]?.status === "ready" &&
          state.documents[0].version !== original.version
        );
      }, "one settled update completes");
      const after = await f.api("GET", "/state");
      assert.equal(
        after.roots.find((item) => item.id === root.id).last_scan,
        before,
        "a file event must not walk all source trees",
      );
      assert.equal(after.documents.length, 1);
      const status = await f.api("GET", "/status");
      assert.equal(status.index.queue.queued, 0);
      assert.ok(status.watchers[root.id].active);
      assert.equal(
        value({
          content: [
            {
              text: JSON.stringify(
                await f.api("POST", "/search", {
                  query: "QUIET_VERSION_FINAL",
                }),
              ),
            },
          ],
        }).hits.length,
        1,
      );
    } finally {
      await f.clean();
    }
  },
);

test(
  "model gateway errors retain status, body and request ID while redacting the API key",
  { timeout: 30000 },
  async () => {
    const f = await fixture(),
      secret = "SYNTHETIC_KEY_TO_REDACT";
    const model = createServer((_request, response) => {
      response.writeHead(504, {
        "Content-Type": "text/plain",
        "x-request-id": "synthetic-gateway-504",
      });
      response.end("CORPORATE_GATEWAY_TIMEOUT " + secret);
    });
    await new Promise((resolve) => model.listen(0, "127.0.0.1", resolve));
    try {
      await f.start();
      await f.api("PATCH", "/settings", {
        embedding_mode: "disabled",
        chat: {
          base_url: "http://127.0.0.1:" + model.address().port + "/v1",
          model: "gateway-test",
          proxy_mode: "direct",
        },
        keys: { chat: secret },
      });
      let error;
      try {
        await f.api("POST", "/models/test/chat", {});
      } catch (value) {
        error = value;
      }
      assert.ok(error);
      assert.match(error.message, /504/);
      assert.match(error.message, /CORPORATE_GATEWAY_TIMEOUT/);
      assert.match(error.message, /synthetic-gateway-504/);
      assert.ok(!error.message.includes(secret));
      assert.doesNotMatch(error.message, /额度|权限|接口路径/);
    } finally {
      await f.api("PATCH", "/settings", { keys: { chat: "" } }).catch(() => {});
      await f.clean();
      model.closeAllConnections();
      await new Promise((resolve) => model.close(resolve));
    }
  },
);

test(
  "quick cold-service shutdown never leaves workers running into runtime teardown",
  { timeout: 35000 },
  async () => {
    for (let i = 0; i < 3; i++) {
      const f = await fixture();
      try {
        await f.start();
        await delay(150);
        await f.stop();
      } finally {
        await f.clean();
      }
    }
  },
);
