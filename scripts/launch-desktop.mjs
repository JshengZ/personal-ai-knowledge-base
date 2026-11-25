import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import { resolve } from "node:path";
const require = createRequire(import.meta.url);
const build = spawn(process.execPath, ["scripts/build-desktop.mjs"], {
  stdio: "inherit",
});
if (await new Promise((resolve) => build.on("exit", resolve))) process.exit(1);
const processDesktop = spawn(require("electron"), ["."], {
  stdio: "inherit",
  env: {
    ...process.env,
    PKB_HOME: resolve(".local/desktop"),
    ...(process.argv.includes("--dev")
      ? { PKB_DEV_URL: "http://127.0.0.1:5173" }
      : {}),
  },
});
processDesktop.on("exit", (code) => process.exit(code || 0));
