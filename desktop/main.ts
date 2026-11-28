import {
  app,
  BrowserWindow,
  Menu,
  Tray,
  nativeImage,
  nativeTheme,
  screen,
  ipcMain,
  dialog,
  shell,
  clipboard,
} from "electron";
import { localFetch } from "./local-http";
import { spawn } from "node:child_process";
import {
  readFile,
  writeFile,
  mkdir,
  stat,
  rename,
  readdir,
  statfs,
} from "node:fs/promises";
import { existsSync, readFileSync, mkdirSync, realpathSync } from "node:fs";
import { join, resolve, dirname, basename, isAbsolute } from "node:path";

interface ServiceInfo {
  port: number;
  pid: number;
  token: string;
  version: string;
  runtime?: Record<string, unknown>;
}
let window: BrowserWindow | null = null;
let tray: Tray | null = null;
let quitting = false;
let service: ServiceInfo;
const bootstrap = process.env.PKB_HOME
  ? resolve(process.env.PKB_HOME)
  : app.getPath("userData");
app.setPath("userData", bootstrap);
let home = bootstrap;
let startupError = "";
let storageBusy = false;
let storagePhase = "";
let storageExitNotice = false;
let initialAppearance: Record<string, unknown> | undefined;
function locateData() {
  let current = bootstrap;
  const visited = new Set<string>();
  for (let attempt = 0; attempt < 8; attempt++) {
    if (visited.has(current)) throw new Error("存储位置配置形成循环。");
    visited.add(current);
    const pointer = join(current, "storage-location.json");
    if (!existsSync(pointer))
      return existsSync(current) ? realpathSync.native(current) : current;
    const value = JSON.parse(readFileSync(pointer, "utf8"));
    const next = value.data_directory;
    if (typeof next !== "string" || !isAbsolute(next) || !existsSync(next))
      throw new Error(
        "知识库存储目录不可用，请连接原数据盘后重新启动。不会在 C 盘创建替代知识库。",
      );
    if (next === current) return realpathSync.native(current);
    current = next;
  }
  throw new Error("存储位置转移过多，请重新配置连接。");
}
try {
  home = locateData();
  for (const name of ["browser", "tmp", "logs", "crashes"])
    mkdirSync(join(home, name), { recursive: true, mode: 0o700 });
  app.setPath("sessionData", join(home, "browser"));
  app.setPath("temp", join(home, "tmp"));
  app.setPath("crashDumps", join(home, "crashes"));
  app.setAppLogsPath(join(home, "logs"));
} catch (error) {
  startupError = error instanceof Error ? error.message : "无法打开存储目录。";
}
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
    if (!quitting && (service || storageBusy)) {
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

function relaunch() {
  quitting = true;
  app.relaunch();
  app.exit(0);
}

async function chooseDataLocation() {
  const chosen = await dialog.showOpenDialog({
    title: "选择知识库存储文件夹（索引、缓存、日志和备份）",
    buttonLabel: "使用此位置",
    properties: ["openDirectory", "createDirectory"],
  });
  if (chosen.canceled) return null;
  const parent = chosen.filePaths[0];
  return basename(parent).toLowerCase() === "personal-knowledge"
    ? parent
    : join(parent, "Personal-Knowledge");
}

async function folderBytes(path: string): Promise<number> {
  let total = 0;
  for (const entry of await readdir(path, { withFileTypes: true })) {
    if (entry.isSymbolicLink()) continue;
    const child = join(path, entry.name);
    if (entry.isDirectory()) total += await folderBytes(child);
    else if (entry.isFile()) total += (await stat(child)).size;
  }
  return total;
}

async function storageInfo() {
  const [disk, entries] = await Promise.all([
    statfs(home),
    readdir(home, { withFileTypes: true }),
  ]);
  const items = await Promise.all(
    entries
      .filter((entry) => !entry.isSymbolicLink())
      .map(async (entry) => ({
        name: entry.name,
        bytes: entry.isDirectory()
          ? await folderBytes(join(home, entry.name))
          : (await stat(join(home, entry.name))).size,
      })),
  );
  return {
    data_directory: home,
    install_directory: dirname(process.execPath),
    available_bytes: disk.bavail * disk.bsize,
    total_bytes: items.reduce((sum, item) => sum + item.bytes, 0),
    items: items.sort((a, b) => b.bytes - a.bytes).slice(0, 15),
  };
}

function processAlive(pid: number) {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

async function stopService(info: ServiceInfo) {
  try {
    await localFetch("http://127.0.0.1:" + info.port + "/api/shutdown", {
      method: "POST",
      headers: {
        Authorization: "Bearer " + info.token,
        "Content-Type": "application/json",
      },
      body: "{}",
      signal: AbortSignal.timeout(3000),
    });
  } catch {}
  for (let attempt = 0; attempt < 30; attempt++) {
    if (!processAlive(info.pid)) return;
    await new Promise((done) => setTimeout(done, 200));
  }
  // Terminate only this knowledge service's recorded process tree. This also
  // handles a v1.0.0 service whose event loop can no longer answer shutdown.
  if (process.platform === "win32") {
    await new Promise<void>((done) => {
      const child = spawn("taskkill", ["/PID", String(info.pid), "/T", "/F"], {
        windowsHide: true,
        stdio: "ignore",
      });
      child.on("error", () => done());
      child.on("exit", () => done());
    });
  } else {
    try {
      process.kill(-info.pid, "SIGKILL");
    } catch {
      try {
        process.kill(info.pid, "SIGKILL");
      } catch {}
    }
  }
  for (let attempt = 0; attempt < 25; attempt++) {
    if (!processAlive(info.pid)) return;
    await new Promise((done) => setTimeout(done, 200));
  }
  throw new Error("知识服务仍被占用，原数据已保留。请关闭 Agent 连接后重试。");
}

async function stopForMove() {
  storagePhase = "正在暂停同步并关闭知识服务";
  let current: ServiceInfo;
  try {
    current = JSON.parse(await readFile(join(home, "service.json"), "utf8"));
  } catch {
    return;
  }
  if (!processAlive(current.pid)) return;
  try {
    await localFetch("http://127.0.0.1:" + current.port + "/api/settings", {
      method: "PATCH",
      headers: {
        Authorization: "Bearer " + current.token,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ paused: true }),
      signal: AbortSignal.timeout(2000),
    });
  } catch {}
  await stopService(current);
}

async function moveStorage(destination: string) {
  if (storageBusy) throw new Error("正在迁移，请等待完成。");
  storageBusy = true;
  try {
    await stopForMove();
    storagePhase = "正在复制并校验知识库，原数据暂时保留";
    await new Promise<void>((done, reject) => {
      const child = spawn(
        engine,
        ["migrate-data", "--home", home, "--destination", destination],
        { windowsHide: true, stdio: ["ignore", "ignore", "pipe"] },
      );
      let errors = "";
      child.stderr.on("data", (bytes) => {
        errors = (errors + bytes.toString()).slice(-4000);
      });
      child.on("error", reject);
      child.on("exit", (code) =>
        code === 0
          ? done()
          : reject(new Error(errors.trim() || "迁移失败，原数据已保留。")),
      );
    });
    const selected = locateData();
    // When the current data directory is already on another drive, update the
    // stable bootstrap pointer too, without leaving a growing redirect chain.
    await privateWrite(
      join(bootstrap, "storage-location.json"),
      JSON.stringify({ version: 1, data_directory: selected }),
    );
    storagePhase = "校验通过，正在重启并释放旧位置的知识库文件";
    relaunch();
  } catch (error) {
    if (locateData() !== home) {
      relaunch();
      return;
    }
    if (service) service = await ensureService();
    throw error;
  } finally {
    storageBusy = false;
    storagePhase = "";
  }
}

async function confirmInitialStorage() {
  if (
    process.env.PKB_HOME ||
    existsSync(join(bootstrap, "storage-location.json"))
  )
    return true;
  while (true) {
    const disk = await statfs(home);
    const answer = await dialog.showMessageBox({
      type: "info",
      title: "选择知识库存储位置",
      message: "程序安装位置与知识库存储位置互相独立。",
      detail:
        "当前位置：" +
        home +
        "\n可用空间：" +
        ((disk.bavail * disk.bsize) / 1024 ** 3).toFixed(1) +
        " GB\n\n索引、向量、缓存、日志和备份将写入数据目录。建议选择 D 盘等空间充足的位置。已有知识库会经过校验后迁移，原始资料留在原处。",
      buttons: ["选择数据位置", "使用当前位置", "退出"],
      defaultId: 0,
      cancelId: 2,
    });
    if (answer.response === 2) {
      quitting = true;
      app.quit();
      return false;
    }
    if (answer.response === 1) {
      if (disk.bavail * disk.bsize < 5 * 1024 ** 3) {
        dialog.showErrorBox(
          "存储空间不足",
          "当前位置不足 5 GB，请选择其他数据盘。",
        );
        continue;
      }
      await privateWrite(
        join(bootstrap, "storage-location.json"),
        JSON.stringify({ version: 1, data_directory: home }),
      );
      return true;
    }
    const destination = await chooseDataLocation();
    if (!destination) continue;
    try {
      await moveStorage(destination);
      return false;
    } catch (error) {
      dialog.showErrorBox(
        "未能迁移知识库",
        error instanceof Error
          ? error.message
          : "原数据已保留，请重新选择位置。",
      );
    }
  }
}

async function healthy(info: ServiceInfo) {
  try {
    return (
      await localFetch("http://127.0.0.1:" + info.port + "/api/health", {
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
    if (await healthy(current)) {
      if (
        current.version === app.getVersion() &&
        JSON.stringify(current.runtime) === JSON.stringify(runtime)
      )
        return current;
      await stopForMove();
    }
  } catch {}
  if (!existsSync(engine))
    throw new Error("安装包中的知识服务程序缺失，请重新安装。");
  const log = join(home, "service.log");
  const child = spawn(engine, ["serve", "--role", "service", "--home", home], {
    detached: true,
    stdio: ["ignore", "ignore", "ignore"],
    windowsHide: true,
    env: {
      ...process.env,
      TMPDIR: join(home, "tmp"),
      TEMP: join(home, "tmp"),
      TMP: join(home, "tmp"),
    },
  });
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
  const response = await localFetch(
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
      // Backups have a bounded format size but disk duration is unpredictable.
      // Aborting an atomic restore would report failure while it still commits.
      signal:
        (method === "GET" && !path.startsWith("/backups/")) ||
        path === "/search" ||
        path === "/ask"
          ? AbortSignal.timeout(180000)
          : undefined,
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
  if (startupError && existsSync(join(bootstrap, "storage-location.json")))
    throw new Error(startupError);
  if (!(await confirmInitialStorage())) return;
  if (startupError) throw new Error(startupError);
  service = await ensureService();
  try {
    const appearance = (await call("GET", "/appearance")) as { theme?: string };
    initialAppearance = appearance;
    if (["light", "dark", "system"].includes(appearance.theme || ""))
      nativeTheme.themeSource = appearance.theme as "light" | "dark" | "system";
  } catch {
    /* The renderer will report a service failure if it persists. */
  }
  nativeTheme.on("updated", () => {
    window?.setBackgroundColor(
      nativeTheme.shouldUseDarkColors ? "#171719" : "#ffffff",
    );
  });
  const monitor = setInterval(() => {
    if (!quitting && !storageBusy && service && !processAlive(service.pid)) {
      // Do not leave Chromium file locks behind after an engine crash.
      quitting = true;
      tray?.destroy();
      window?.destroy();
      app.exit(0);
    }
  }, 1500);
  monitor.unref();
  ipcMain.handle("knowledge:request", (event, method, path, body) => {
    assertSender(event);
    if (storageBusy) throw new Error("正在迁移知识库，请等待重启完成。");
    return call(method, path, body);
  });
  ipcMain.handle("knowledge:theme", (event, theme) => {
    assertSender(event);
    if (!["light", "dark", "system"].includes(theme))
      throw new Error("外观选项无效");
    nativeTheme.themeSource = theme;
    window?.setBackgroundColor(
      nativeTheme.shouldUseDarkColors ? "#171719" : "#ffffff",
    );
  });
  ipcMain.handle("knowledge:choose-directories", async (event) => {
    assertSender(event);
    const result = await dialog.showOpenDialog(window!, {
      title: "选择资料目录（可多选）",
      properties: ["openDirectory", "multiSelections"],
    });
    return result.canceled ? [] : result.filePaths;
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
  ipcMain.handle("knowledge:storage-info", async (event) => {
    assertSender(event);
    return storageInfo();
  });
  ipcMain.handle("knowledge:storage-progress", (event) => {
    assertSender(event);
    return storagePhase;
  });
  ipcMain.handle("knowledge:prepare-storage-move", async (event) => {
    assertSender(event);
    const destination = await chooseDataLocation();
    if (!destination) return null;
    await call("POST", "/storage/validate", { path: destination });
    const [source, disk] = await Promise.all([
      storageInfo(),
      statfs(dirname(destination)),
    ]);
    const available = disk.bavail * disk.bsize;
    // The engine distinguishes fresh copies from a verified migration retry.
    // A ready copy must not require another library-sized amount of free space.
    return {
      path: destination,
      source_bytes: source.total_bytes,
      available_bytes: available,
    };
  });
  ipcMain.handle("knowledge:move-storage", (event, destination) => {
    assertSender(event);
    if (typeof destination !== "string" || !isAbsolute(destination))
      throw new Error("存储位置无效。");
    return moveStorage(destination);
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
  const area = screen.getDisplayNearestPoint(
    screen.getCursorScreenPoint(),
  ).workAreaSize;
  window = new BrowserWindow({
    width: Math.min(1320, Math.max(640, area.width - 48)),
    height: Math.min(920, Math.max(520, area.height - 48)),
    minWidth: Math.min(640, area.width),
    minHeight: Math.min(520, area.height),
    backgroundColor: nativeTheme.shouldUseDarkColors ? "#171719" : "#ffffff",
    title: "个人知识库",
    show: false,
    webPreferences: {
      additionalArguments: [
        "--pkb-appearance=" + JSON.stringify(initialAppearance || {}),
      ],
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
  if (storageBusy) {
    window?.show();
    if (!storageExitNotice) {
      storageExitNotice = true;
      try {
        await dialog.showMessageBox({
          type: "info",
          title: "正在迁移数据",
          message: "迁移尚未完成",
          detail: (storagePhase || "正在处理数据") + "。完成后将自动重启应用。",
          buttons: ["继续迁移"],
        });
      } finally {
        storageExitNotice = false;
      }
    }
    return;
  }
  quitting = true;
  try {
    if (service) await stopService(service);
  } catch (error) {
    dialog.showErrorBox(
      "知识服务尚未退出",
      error instanceof Error ? error.message : "请关闭 Agent 连接后重试。",
    );
    quitting = false;
    return;
  }
  tray?.destroy();
  app.quit();
}
