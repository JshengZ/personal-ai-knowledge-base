export async function request<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  if (window.knowledge)
    return window.knowledge.request(method, path, body) as Promise<T>;
  const response = await fetch("/api" + path, {
    method,
    headers:
      body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const value = await response.json();
  if (!response.ok) throw new Error(value.error || "操作失败，请稍后重试。");
  return value as T;
}

export async function copyText(text: string) {
  if (window.knowledge) await window.knowledge.copy(text);
  else await navigator.clipboard.writeText(text);
}

export async function downloadBackup() {
  if (window.knowledge) return window.knowledge.exportBackup();
  const response = await fetch("/api/backups/export");
  if (!response.ok)
    throw new Error((await response.json()).error || "备份失败");
  const url = URL.createObjectURL(await response.blob());
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download =
    "个人知识库-" + new Date().toISOString().slice(0, 10) + ".pkb";
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  return "备份已下载";
}

export async function restoreBackup(file?: File) {
  if (window.knowledge) return window.knowledge.restoreBackup();
  if (!file) throw new Error("请选择备份文件");
  const response = await fetch("/api/backups/restore", {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream" },
    body: file,
  });
  const result = await response.json();
  if (!response.ok) throw new Error(result.error || "恢复失败");
  return result;
}
