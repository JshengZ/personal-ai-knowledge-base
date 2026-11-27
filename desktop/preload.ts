import { contextBridge, ipcRenderer } from "electron";

let initialAppearance;
try {
  const argument = process.argv.find((value) =>
    value.startsWith("--pkb-appearance="),
  );
  if (argument)
    initialAppearance = JSON.parse(argument.slice("--pkb-appearance=".length));
} catch {
  /* The authoritative preferences are also present in /state. */
}

contextBridge.exposeInMainWorld("knowledge", {
  initialAppearance,
  request: (method: string, path: string, body?: unknown) =>
    ipcRenderer.invoke("knowledge:request", method, path, body),
  chooseDirectories: () => ipcRenderer.invoke("knowledge:choose-directories"),
  chooseDirectory: () => ipcRenderer.invoke("knowledge:choose-directory"),
  openSource: (id: string) => ipcRenderer.invoke("knowledge:open-source", id),
  revealData: () => ipcRenderer.invoke("knowledge:reveal-data"),
  storageInfo: () => ipcRenderer.invoke("knowledge:storage-info"),
  prepareStorageMove: () =>
    ipcRenderer.invoke("knowledge:prepare-storage-move"),
  storageProgress: () => ipcRenderer.invoke("knowledge:storage-progress"),
  moveStorage: (path: string) =>
    ipcRenderer.invoke("knowledge:move-storage", path),
  exportBackup: () => ipcRenderer.invoke("knowledge:export-backup"),
  restoreBackup: () => ipcRenderer.invoke("knowledge:restore-backup"),
  autostart: (enabled?: boolean) =>
    ipcRenderer.invoke("knowledge:autostart", enabled),
  copy: (text: string) => ipcRenderer.invoke("knowledge:copy", text),
  setTheme: (theme: string) => ipcRenderer.invoke("knowledge:theme", theme),
});
