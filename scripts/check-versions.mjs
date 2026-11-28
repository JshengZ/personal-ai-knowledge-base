import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
const app = JSON.parse(await readFile("package.json", "utf8"));
const lock = JSON.parse(await readFile("package-lock.json", "utf8"));
const cargo = await readFile("engine/Cargo.toml", "utf8"),
  rustLock = await readFile("engine/Cargo.lock", "utf8"),
  mcp = await readFile("engine/src/mcp.rs", "utf8");
assert.equal(cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1], app.version);
assert.equal(
  rustLock.match(/name = "knowledge-engine"\r?\nversion = "([^"]+)"/)?.[1],
  app.version,
);
assert.equal(lock.version, app.version);
assert.equal(lock.packages[""].version, app.version);
assert.doesNotMatch(
  mcp,
  /version\s*=\s*"\d+\.\d+\.\d+"/,
  "MCP must inherit CARGO_PKG_VERSION",
);
console.log(
  "Desktop, engine, locks and MCP version source agree: " + app.version,
);
