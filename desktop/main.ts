import {
  app,
  BrowserWindow,
  Menu,
  Tray,
  nativeImage,
  ipcMain,
  dialog,
  shell,
  clipboard,
} from "electron";
import { spawn } from "node:child_process";
import {
  readFile,
  writeFile,
  mkdir,
  stat,
  rename,
  open,
} from "node:fs/promises";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";

interface ServiceInfo {
  port: number;
  pid: number;
  token: string;
  version: string;
}
let window: BrowserWindow | null = null;
let tray: Tray | null = null;
let quitting = false;
let service: ServiceInfo;
const home = process.env.PKB_HOME
  ? resolve(process.env.PKB_HOME)
  : app.getPath("userData");
if (process.env.PKB_HOME) app.setPath("userData", home);
const resources = app.isPackaged ? process.resourcesPath : resolve("resources");
const engineName =
  process.platform === "win32" ? "knowledge-engine.exe" : "knowledge-engine";
const engine = app.isPackaged
  ? join(resources, "engine", engineName)
  : [
      resolve("engine/target/debug", engineName),
      resolve("engine/target/release", engineName),
    ].find(existsSync) || resolve("engine/target/debug", engineName);

app.setName("个人知识库");
app.commandLine.appendSwitch("disable-background-networking");
app.commandLine.appendSwitch("disable-component-update");
app.commandLine.appendSwitch("disable-domain-reliability");
app.commandLine.appendSwitch("disable-features", "MediaRouter");
const single = app.requestSingleInstanceLock();
if (!single) app.quit();
else {
  app.on("before-quit", (event) => {
    if (!quitting && service) {
      event.preventDefault();
      void quit();
    }
  });
  app.on("second-instance", () => showWindow());
  app.on("activate", () => showWindow());
  app.on("window-all-closed", () => {});
  app
    .whenReady()
    .then(start)
    .catch((error) => {
      dialog.showErrorBox(
        "个人知识库启动失败",
        error instanceof Error
          ? error.message
          : "本地知识服务未能启动。请重新安装完整安装包。",
      );
      quitting = true;
      app.quit();
    });
}

async function privateWrite(path: string, bytes: string | Buffer) {
  const temporary = path + "." + process.pid + ".tmp";
  await writeFile(temporary, bytes, { mode: 0o600 });
  await rename(temporary, path);
}

async function healthy(info: ServiceInfo) {
  try {
    return (
      await fetch("http://127.0.0.1:" + info.port + "/api/health", {
        headers: { Authorization: "Bearer " + info.token },
        signal: AbortSignal.timeout(1500),
      })
    ).ok;
  } catch {
    return false;
  }
}

async function ensureService(): Promise<ServiceInfo> {
  await mkdir(home, { recursive: true, mode: 0o700 });
  const runtime = {
    parser_runtime: process.execPath,
    parser_script: app.isPackaged
      ? join(resources, "app.asar.unpacked/desktop-dist/parser.mjs")
      : join(app.getAppPath(), "desktop-dist/parser.mjs"),
    resources,
    web_root: app.isPackaged
      ? join(resources, "app.asar.unpacked/dist")
      : join(app.getAppPath(), "dist"),
    mcp_executable: engine,
    electron_runtime: true,
  };
  await privateWrite(join(home, "runtime.json"), JSON.stringify(runtime));
  try {
    const current = JSON.parse(
      await readFile(join(home, "service.json"), "utf8"),
    );
    if (await healthy(current)) return current;
  } catch {}
  if (!existsSync(engine))
    throw new Error("安装包中的知识服务程序缺失，请重新安装。");
  const log = join(home, "service.log");
  try {
    if ((await stat(log)).size > 4 * 1024 * 1024) await rename(log, log + ".1");
  } catch {}
  const output = await open(log, "a", 0o600);
  let child;
  try {
    child = spawn(engine, ["serve", "--home", home], {
      detached: true,
      stdio: ["ignore", "ignore", output.fd],
      windowsHide: true,
    });
  } finally {
    await output.close();
  }
  let spawnError = "";
  child.on("error", (error) => {
    spawnError = error.message;
  });
  child.unref();
  for (let attempt = 0; attempt < 100; attempt++) {
    await new Promise((resolve) => setTimeout(resolve, 180));
    if (spawnError) throw new Error("本地知识服务启动失败。" + spawnError);
    try {
      const current = JSON.parse(
        await readFile(join(home, "service.json"), "utf8"),
      );
      if (await healthy(current)) return current;
    } catch {}
  }
  throw new Error("本地知识服务启动超时。处理记录位于 " + log);
}

async function call(method: string, path: string, body?: unknown, raw = false) {
  if (
    !/^\/[a-zA-Z0-9_/?&=%.:-]*$/.test(path) ||
    path.includes("..") ||
    path.startsWith("//") ||
    !["GET", "POST", "PATCH", "DELETE"].includes(method)
  )
    throw new Error("请求路径无效。");
  const response = await fetch(
    "http://127.0.0.1:" + service.port + "/api" + path,
    {
      method,
      headers: {
        Authorization: "Bearer " + service.token,
        ...(body !== undefined
          ? {
              "Content-Type":
                body instanceof Buffer
                  ? "application/octet-stream"
                  : "application/json",
            }
          : {}),
      },
      body:
        body === undefined
          ? undefined
          : body instanceof Buffer
            ? body
            : JSON.stringify(body),
      signal: AbortSignal.timeout(180000),
    },
  );
  if (raw && response.ok) return Buffer.from(await response.arrayBuffer());
  const value = (await response.json()) as { error?: string };
  if (!response.ok) throw new Error(value.error || "操作失败，请稍后重试。");
  return value;
}

function assertSender(event: Electron.IpcMainInvokeEvent) {
  if (!window || event.sender.id !== window.webContents.id)
    throw new Error("窗口会话不可用。");
}

async function start() {
  service = await ensureService();
  ipcMain.handle("knowledge:request", (event, method, path, body) => {
    assertSender(event);
    return call(method, path, body);
  });
  ipcMain.handle("knowledge:choose-directory", async (event) => {
    assertSender(event);
    const result = await dialog.showOpenDialog(window!, {
      title: "选择资料目录",
      properties: ["openDirectory"],
    });
    return result.canceled ? null : result.filePaths[0];
  });
  ipcMain.handle("knowledge:open-source", async (event, id) => {
    assertSender(event);
    if (typeof id !== "string" || !/^[-a-zA-Z0-9]+$/.test(id))
      throw new Error("资料编号无效。");
    const value = (await call("GET", "/sources/" + id)) as any;
    shell.showItemInFolder(value.document.path);
  });
  ipcMain.handle("knowledge:reveal-data", async (event) => {
    assertSender(event);
    const result = await shell.openPath(home);
    if (result) throw new Error(result);
  });
  ipcMain.handle("knowledge:copy", (event, text) => {
    assertSender(event);
    if (typeof text !== "string" || text.length > 1000000)
      throw new Error("复制内容无效。");
    clipboard.writeText(text);
  });
  ipcMain.handle("knowledge:autostart", (event, enabled) => {
    assertSender(event);
    if (typeof enabled === "boolean")
      app.setLoginItemSettings({
        openAtLogin: enabled,
        args: ["--background"],
      });
    return app.getLoginItemSettings().openAtLogin;
  });
  ipcMain.handle("knowledge:export-backup", async (event) => {
    assertSender(event);
    const chosen = await dialog.showSaveDialog(window!, {
      title: "备份个人知识库",
      defaultPath: join(
        app.getPath("documents"),
        "个人知识库-" + new Date().toISOString().slice(0, 10) + ".pkb",
      ),
      filters: [{ name: "个人知识库备份", extensions: ["pkb"] }],
    });
    if (chosen.canceled || !chosen.filePath) return null;
    const bytes = (await call(
      "GET",
      "/backups/export",
      undefined,
      true,
    )) as Buffer;
    await privateWrite(chosen.filePath, bytes);
    return chosen.filePath;
  });
  ipcMain.handle("knowledge:restore-backup", async (event) => {
    assertSender(event);
    const chosen = await dialog.showOpenDialog(window!, {
      title: "选择知识库备份",
      filters: [{ name: "个人知识库备份", extensions: ["pkb", "zip"] }],
      properties: ["openFile"],
    });
    if (chosen.canceled) return null;
    const path = chosen.filePaths[0];
    if ((await stat(path)).size > 512 * 1024 * 1024)
      throw new Error("备份超过当前支持的大小。");
    return call("POST", "/backups/restore", await readFile(path));
  });
  const iconPath = join(resources, "tray.png");
  if (existsSync(iconPath)) {
    const icon = nativeImage.createFromPath(iconPath);
    if (process.platform === "darwin") icon.setTemplateImage(true);
    tray = new Tray(icon);
    tray.setToolTip("个人知识库 · 后台同步");
    tray.setContextMenu(
      Menu.buildFromTemplate([
        { label: "打开个人知识库", click: () => showWindow() },
        { type: "separator" },
        { label: "退出知识库", click: () => void quit() },
      ]),
    );
    tray.on("click", () => showWindow());
  }
  const menu: Electron.MenuItemConstructorOptions[] = [
    {
      label: "个人知识库",
      submenu: [
        { label: "打开窗口", click: () => showWindow() },
        { type: "separator" },
        {
          label: "退出",
          accelerator: process.platform === "darwin" ? "Command+Q" : "Alt+F4",
          click: () => void quit(),
        },
      ],
    },
    {
      label: "编辑",
      submenu: [
        { role: "undo", label: "撤销" },
        { role: "redo", label: "重做" },
        { type: "separator" },
        { role: "cut", label: "剪切" },
        { role: "copy", label: "复制" },
        { role: "paste", label: "粘贴" },
        { role: "selectAll", label: "全选" },
      ],
    },
    {
      label: "视图",
      submenu: [
        { role: "resetZoom", label: "实际大小" },
        { role: "zoomIn", label: "放大" },
        { role: "zoomOut", label: "缩小" },
        { role: "togglefullscreen", label: "全屏" },
      ],
    },
  ];
  Menu.setApplicationMenu(Menu.buildFromTemplate(menu));
  if (!process.argv.includes("--background")) showWindow();
}

function showWindow() {
  if (!service) return;
  if (window) {
    window.show();
    window.focus();
    return;
  }
  window = new BrowserWindow({
    width: 1488,
    height: 1056,
    minWidth: 760,
    minHeight: 580,
    backgroundColor: "#fcfbf8",
    title: "个人知识库",
    show: false,
    webPreferences: {
      preload: join(__dirname, "preload.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webSecurity: true,
    },
  });
  window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  window.webContents.on("will-navigate", (event, url) => {
    const allowed =
      process.env.PKB_DEV_URL || "http://127.0.0.1:" + service.port;
    if (new URL(url).origin !== new URL(allowed).origin) event.preventDefault();
  });
  window.webContents.session.webRequest.onBeforeRequest(
    { urls: ["http://*/*", "https://*/*"] },
    (details, callback) => {
      const url = new URL(details.url);
      callback({ cancel: !["127.0.0.1", "localhost"].includes(url.hostname) });
    },
  );
  window.loadURL(
    process.env.PKB_DEV_URL || "http://127.0.0.1:" + service.port + "/",
  );
  window.once("ready-to-show", () => window?.show());
  window.on("close", (event) => {
    if (!quitting && tray) {
      event.preventDefault();
      window?.hide();
    }
  });
  window.on("closed", () => {
    window = null;
  });
}

async function quit() {
  quitting = true;
  try {
    await call("POST", "/shutdown", {});
  } catch {}
  tray?.destroy();
  app.quit();
}
