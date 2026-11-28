// Test seam at the desktop IPC boundary. The production loopback client uses
// node:http deliberately, so replacing global fetch would not exercise it.
export async function interceptRequests(app, interceptor) {
  await app.evaluate(({ ipcMain }, source) => {
    const original = ipcMain._invokeHandlers.get("knowledge:request");
    if (typeof original !== "function")
      throw new Error("Desktop request handler is unavailable");
    const intercept = (0, eval)("(" + source + ")");
    ipcMain.removeHandler("knowledge:request");
    ipcMain.handle("knowledge:request", (event, method, path, body) =>
      intercept({ method, path, body }, () =>
        original(event, method, path, body),
      ),
    );
  }, interceptor.toString());
}
