import { contextBridge, ipcRenderer } from "electron";

contextBridge.exposeInMainWorld("knowledge", {
  request: (method: string, path: string, body?: unknown) =>
    ipcRenderer.invoke("knowledge:request", method, path, body),
  chooseDirectory: () => ipcRenderer.invoke("knowledge:choose-directory"),
  openSource: (id: string) => ipcRenderer.invoke("knowledge:open-source", id),
  revealData: () => ipcRenderer.invoke("knowledge:reveal-data"),
  exportBackup: () => ipcRenderer.invoke("knowledge:export-backup"),
  restoreBackup: () => ipcRenderer.invoke("knowledge:restore-backup"),
  autostart: (enabled?: boolean) =>
    ipcRenderer.invoke("knowledge:autostart", enabled),
  copy: (text: string) => ipcRenderer.invoke("knowledge:copy", text),
});
