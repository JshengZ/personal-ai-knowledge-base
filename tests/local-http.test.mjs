import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { build } from "esbuild";

test(
  "desktop loopback HTTP ignores environment proxies, handles empty bodies and cancels requests",
  { timeout: 15000 },
  async () => {
    const directory = await mkdtemp(join(tmpdir(), "pkb-local-http-")),
      file = join(directory, "local-http.mjs");
    let intercepted = 0;
    const proxy = createServer((_request, response) => {
      intercepted++;
      response.writeHead(502);
      response.end();
    });
    const server = createServer((request, response) => {
      if (request.url === "/empty") {
        response.writeHead(204);
        response.end();
      } else if (request.url !== "/wait") {
        response.setHeader("Content-Type", "application/json");
        response.end('{"ok":true}');
      }
    });
    await Promise.all([
      new Promise((resolve) => proxy.listen(0, "127.0.0.1", resolve)),
      new Promise((resolve) => server.listen(0, "127.0.0.1", resolve)),
    ]);
    try {
      await build({
        entryPoints: ["desktop/local-http.ts"],
        outfile: file,
        platform: "node",
        format: "esm",
        bundle: true,
      });
      const { stdout } = await promisify(execFile)(
        process.execPath,
        [
          "--input-type=module",
          "-e",
          `import assert from 'node:assert/strict';import{localFetch}from${JSON.stringify(pathToFileURL(file).href)};const base=${JSON.stringify("http://127.0.0.1:" + server.address().port)};assert.equal((await(await localFetch(base)).json()).ok,true);assert.equal((await localFetch(base+'/empty')).status,204);await assert.rejects(localFetch(base+'/wait',{signal:AbortSignal.timeout(100)}));await assert.rejects(localFetch('http://external.invalid'));console.log('loopback passed');`,
        ],
        {
          env: {
            ...process.env,
            NODE_USE_ENV_PROXY: "1",
            HTTP_PROXY: "http://127.0.0.1:" + proxy.address().port,
            http_proxy: "http://127.0.0.1:" + proxy.address().port,
            NO_PROXY: "",
            no_proxy: "",
          },
          timeout: 10000,
        },
      );
      assert.match(stdout, /loopback passed/);
      assert.equal(intercepted, 0);
    } finally {
      for (const value of [server, proxy]) {
        value.closeAllConnections();
        await new Promise((resolve) => value.close(resolve));
      }
      await rm(directory, { recursive: true, force: true });
    }
  },
);
