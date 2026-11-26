import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:net";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

const windows = process.platform === "win32";
const listener = createServer();
await new Promise((accept, reject) => {
  listener.once("error", reject);
  listener.listen(0, "127.0.0.1", accept);
});
const port = listener.address().port;
await new Promise((accept) => listener.close(accept));
const child = spawn(
  resolve(
    `resources/runtime/${windows ? "win-x64" : "mac-" + process.arch}/${windows ? "llama-server.exe" : "llama-server"}`,
  ),
  [
    "--model",
    resolve("resources/runtime/models/Qwen3-Embedding-0.6B-Q8_0.gguf"),
    "--host",
    "127.0.0.1",
    "--port",
    String(port),
    "--embeddings",
    "--pooling",
    "last",
    "--ctx-size",
    "4096",
    "--parallel",
    "1",
    "--batch-size",
    "512",
    "--ubatch-size",
    "256",
    "--cache-ram",
    "0",
    "--n-gpu-layers",
    "0",
    "--threads",
    "2",
    "--threads-batch",
    "2",
  ],
  { windowsHide: true, stdio: ["ignore", "pipe", "pipe"] },
);
let diagnostic = "",
  spawnError;
for (const stream of [child.stdout, child.stderr])
  stream.on("data", (bytes) => {
    diagnostic = (diagnostic + bytes.toString()).slice(-24000);
  });
child.on("error", (error) => {
  spawnError = error;
});
try {
  const started = Date.now();
  let healthy = false;
  while (Date.now() - started < 90000) {
    if (spawnError) throw spawnError;
    assert.equal(
      child.exitCode,
      null,
      "Embedding runtime exited during startup",
    );
    try {
      healthy = (
        await fetch(`http://127.0.0.1:${port}/health`, {
          signal: AbortSignal.timeout(1000),
        })
      ).ok;
    } catch {}
    if (healthy) break;
    await delay(250);
  }
  assert.ok(healthy, "Embedding runtime health check timed out");
  console.log(
    JSON.stringify({
      embeddingRuntimeReady: true,
      milliseconds: Date.now() - started,
    }),
  );
  for (const [name, text] of [
    ["short", "本地语义检索验证"],
    [
      "bounded-long",
      "推荐排序的长文本验证，保留项目背景与原文出处。".repeat(85),
    ],
  ]) {
    const started = Date.now();
    const response = await fetch(`http://127.0.0.1:${port}/v1/embeddings`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ model: "Qwen3-Embedding-0.6B", input: [text] }),
      signal: AbortSignal.timeout(120000),
    });
    const value = await response.json();
    assert.equal(response.ok, true, JSON.stringify(value));
    assert.equal(value.data?.length, 1);
    const vector = value.data[0].embedding;
    assert.equal(vector.length, 1024);
    assert.ok(vector.every(Number.isFinite));
    assert.ok(vector.some((number) => number !== 0));
    console.log(
      JSON.stringify({
        embeddingRuntimeInput: name,
        characters: text.length,
        dimensions: vector.length,
        milliseconds: Date.now() - started,
      }),
    );
  }
} catch (error) {
  console.error(diagnostic);
  throw error;
} finally {
  if (child.exitCode === null) {
    const closed = new Promise((accept) => child.once("exit", accept));
    child.kill();
    await Promise.race([closed, delay(5000)]);
    if (child.exitCode === null) child.kill("SIGKILL");
  }
}
