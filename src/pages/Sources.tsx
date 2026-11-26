import { useDeferredValue, useEffect, useState, type FormEvent } from "react";
import {
  PlusIcon,
  ArrowsClockwiseIcon,
  PauseIcon,
  PlayIcon,
  PencilSimpleIcon,
  TrashIcon,
  FolderIcon,
} from "@phosphor-icons/react";
import { request } from "../api";
import { useKnowledge } from "../context";
import type { Document, Root } from "../types";
import { Empty, FileIcon, Modal, date } from "../components/ui";
import { IndexProgress } from "../components/IndexProgress";

const statusNames: Record<string, string> = {
  ready: "全文就绪",
  indexing: "正在索引",
  error: "未完成，查看原因",
  empty: "未提取到文字",
};
const policyNames: Record<string, string> = {
  local: "仅本机检索",
  company: "允许公司模型",
  agents: "允许公司模型与 Agent",
};

function sourcePath(document: Document, roots: Root[]) {
  const path = document.path.replace(/^\\\\\?\\/, "").replace(/\\/g, "/");
  const root = roots.find((item) => item.id === document.root_id);
  const base = root?.path
    .replace(/^\\\\\?\\/, "")
    .replace(/\\/g, "/")
    .replace(/\/$/, "");
  return root && base && path.startsWith(base + "/")
    ? root.name + " / " + path.slice(base.length + 1).replace(/\//g, " / ")
    : path;
}

export function Sources() {
  const { data, addRoot, openSource, notify, refresh, navigate } =
    useKnowledge();
  const [tab, setTab] = useState<"files" | "roots">("files");
  const [query, setQuery] = useState("");
  const deferred = useDeferredValue(query);
  const [scope, setScope] = useState("");
  const [status, setStatus] = useState("");
  const [documents, setDocuments] = useState<Document[]>([]);
  const [total, setTotal] = useState(0);
  const [editing, setEditing] = useState<Root | null>(null);
  const [removing, setRemoving] = useState<Root | null>(null);
  const [busy, setBusy] = useState(false);
  const [loadError, setLoadError] = useState("");
  const load = async (offset = 0) => {
    const parameters = new URLSearchParams({
      offset: String(offset),
      limit: "100",
    });
    if (scope) parameters.set("project_id", scope);
    if (deferred) parameters.set("query", deferred);
    if (status) parameters.set("status", status);
    const result = await request<{ documents: Document[]; total: number }>(
      "GET",
      "/documents?" + parameters,
    );
    setDocuments((previous) =>
      offset ? [...previous, ...result.documents] : result.documents,
    );
    setTotal(result.total);
  };
  useEffect(() => {
    let current = true;
    const parameters = new URLSearchParams({
      limit: "100",
      ...(scope ? { project_id: scope } : {}),
      ...(deferred ? { query: deferred } : {}),
      ...(status ? { status } : {}),
    });
    request<{ documents: Document[]; total: number }>(
      "GET",
      "/documents?" + parameters,
    )
      .then((result) => {
        if (current) {
          setDocuments(result.documents);
          setTotal(result.total);
          setLoadError("");
        }
      })
      .catch((error) => {
        if (current) setLoadError(error.message);
      });
    return () => {
      current = false;
    };
  }, [
    scope,
    deferred,
    status,
    data.last_indexed,
    data.last_scan,
    data.stats.pending,
  ]);
  const action = async (callback: () => Promise<unknown>, message?: string) => {
    setBusy(true);
    try {
      await callback();
      await refresh();
      if (message) notify(message);
    } catch (error) {
      notify(error instanceof Error ? error.message : "操作失败", true);
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <header className="page-heading">
        <div>
          <h1>资料与同步</h1>
          <p>目录持续更新，检索随时可用。</p>
        </div>
        <button className="primary" onClick={addRoot}>
          <PlusIcon size={17} />
          添加资料目录
        </button>
      </header>
      <IndexProgress data={data} onSettings={() => navigate("settings")} />
      {!!data.format_warnings?.length && (
        <details className="notice">
          <summary>发现暂不支持的 Office 文件格式，尚未进入索引</summary>
          {data.format_warnings.map((item) => (
            <p key={item.root_id + item.extension}>
              {item.root_name}：{item.count} 个 .{item.extension} 文件。可在
              Office 中另存为 .docx、.pptx、.xlsx，或导出为 PDF 后加入。
            </p>
          ))}
        </details>
      )}
      <div className="section-toolbar">
        <div className="tabs" aria-label="资料视图">
          <button
            className={tab === "files" ? "selected" : ""}
            onClick={() => setTab("files")}
            aria-pressed={tab === "files"}
          >
            文件
          </button>
          <button
            className={tab === "roots" ? "selected" : ""}
            onClick={() => setTab("roots")}
            aria-pressed={tab === "roots"}
          >
            资料目录
          </button>
        </div>
        <div className="actions">
          <button
            className="text-action"
            onClick={() =>
              void action(() =>
                request("PATCH", "/settings", {
                  paused: !data.settings.paused,
                }),
              )
            }
            disabled={busy}
          >
            {data.settings.paused ? (
              <PlayIcon size={16} />
            ) : (
              <PauseIcon size={16} />
            )}{" "}
            {data.settings.paused ? "继续同步" : "暂停同步"}
          </button>
          <button
            className="text-action"
            onClick={() =>
              void action(
                () => request("POST", "/scan", { force: false }),
                "已开始核对资料目录",
              )
            }
            disabled={busy || data.scanning}
          >
            <ArrowsClockwiseIcon
              size={16}
              className={data.scanning ? "spin" : ""}
            />
            立即核对
          </button>
        </div>
      </div>
      {tab === "files" ? (
        <>
          <div className="file-filters">
            <label className="sr-only" htmlFor="filename-search">
              搜索文件名
            </label>
            <input
              id="filename-search"
              placeholder="搜索文件名"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            <label className="sr-only" htmlFor="file-project">
              项目范围
            </label>
            <select
              id="file-project"
              value={scope}
              onChange={(event) => setScope(event.target.value)}
            >
              <option value="">全部项目</option>
              {data.projects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
                </option>
              ))}
            </select>
            <label className="sr-only" htmlFor="file-status">
              索引状态
            </label>
            <select
              id="file-status"
              value={status}
              onChange={(event) => setStatus(event.target.value)}
            >
              <option value="">全部状态</option>
              {Object.entries(statusNames).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </div>
          <div className="section-heading">
            <p className="muted">
              {total} 份资料
              {data.stats.pending > 0
                ? " · " + data.stats.pending + " 份待处理"
                : ""}
            </p>
            <span className="muted">
              {data.settings.paused ? "后台同步已暂停" : "后台自动更新"}
            </span>
          </div>
          {loadError && (
            <p className="notice error" role="alert">
              {loadError}
            </p>
          )}
          {documents.length ? (
            <div className="file-table">
              <div className="file-table-head">
                <span>文件</span>
                <span>所属项目</span>
                <span>状态</span>
                <span>更新时间</span>
              </div>
              {documents.map((document) => (
                <div className="file-table-row" key={document.id}>
                  <button
                    className="file-name"
                    onClick={() => openSource(document.id)}
                    disabled={document.status !== "ready"}
                  >
                    <FileIcon extension={document.extension} />
                    <span>
                      <strong>{document.title}</strong>
                      <small title={document.path}>
                        {sourcePath(document, data.roots)}
                        {document.ocr ? " · OCR" : ""}
                      </small>
                    </span>
                  </button>
                  <span className="file-project">
                    {data.projects.find(
                      (project) => project.id === document.project_id,
                    )?.name || "项目"}
                  </span>
                  <div className="file-status">
                    <span
                      className={
                        document.status === "error" ? "error-text" : ""
                      }
                    >
                      {statusNames[document.status] || document.status}
                    </span>
                    {document.error && (
                      <details>
                        <summary>查看原因</summary>
                        <p>{document.error}</p>
                        <button
                          className="text-action"
                          onClick={() =>
                            void action(
                              () => request("POST", "/scan", { force: true }),
                              "已安排重新索引",
                            )
                          }
                        >
                          重新索引
                        </button>
                      </details>
                    )}
                  </div>
                  <time>{date(document.updated_at)}</time>
                </div>
              ))}
            </div>
          ) : (
            <Empty
              title={query || status ? "没有符合条件的文件" : "等待资料加入"}
              action={
                !data.roots.length ? (
                  <button onClick={addRoot}>选择资料目录</button>
                ) : undefined
              }
            >
              {data.stats.pending
                ? "正在处理文件，完成后会显示在这里。"
                : "可以添加目录，或调整当前筛选条件。"}
            </Empty>
          )}
          {documents.length < total && (
            <button
              onClick={() =>
                void load(documents.length).catch((error) =>
                  notify(error.message, true),
                )
              }
            >
              加载更多
            </button>
          )}
        </>
      ) : (
        <div className="root-list">
          {data.roots.length ? (
            data.roots.map((root) => (
              <section className="root-row" key={root.id}>
                <div className="root-heading">
                  <FolderIcon size={29} weight="regular" />
                  <div>
                    <h2>{root.name}</h2>
                    <p className="path-text">{root.path}</p>
                  </div>
                  <span className="root-state">
                    {root.enabled ? "监听中" : "已暂停"}
                  </span>
                </div>
                <p>
                  {policyNames[root.policy]}{" "}
                  <span className="muted">
                    ·{" "}
                    {root.last_scan
                      ? "最近核对 " + date(root.last_scan, true)
                      : "等待首次核对"}
                  </span>
                </p>
                {root.error && <p className="notice error">{root.error}</p>}
                <div className="actions">
                  <button
                    className="text-action"
                    onClick={() => setEditing(root)}
                  >
                    <PencilSimpleIcon size={16} />
                    管理目录
                  </button>
                  <button
                    className="text-action"
                    onClick={() =>
                      void action(() =>
                        request("PATCH", "/roots/" + root.id, {
                          enabled: !root.enabled,
                        }),
                      )
                    }
                  >
                    {root.enabled ? "暂停此目录" : "开启此目录"}
                  </button>
                  <button
                    className="text-action danger-action"
                    onClick={() => setRemoving(root)}
                  >
                    <TrashIcon size={16} />
                    移除目录
                  </button>
                </div>
              </section>
            ))
          ) : (
            <Empty
              title="选择需要持续更新的目录"
              action={
                <button className="primary" onClick={addRoot}>
                  添加资料目录
                </button>
              }
            >
              项目文件、工作文档和笔记都可以加入。
            </Empty>
          )}
        </div>
      )}
      {editing && (
        <RootDialog root={editing} onClose={() => setEditing(null)} />
      )}
      {removing && (
        <Modal
          title="移除资料目录"
          onClose={() => setRemoving(null)}
          onSubmit={(event) => {
            event.preventDefault();
            void action(async () => {
              await request("DELETE", "/roots/" + removing.id);
              setRemoving(null);
            }, "资料目录已移除");
          }}
          submitLabel="移除目录"
          busy={busy}
        >
          <p>
            将从知识库中移除「{removing.name}
            」的索引，并更新依赖这些资料的记忆。
          </p>
          <p className="muted">原始文件继续保存在 {removing.path}。</p>
        </Modal>
      )}
    </>
  );
}

export function RootDialog({
  root,
  onClose,
}: {
  root?: Root;
  onClose: () => void;
}) {
  const { refresh, notify } = useKnowledge();
  const [path, setPath] = useState(root?.path || "");
  const [name, setName] = useState(root?.name || "");
  const [policy, setPolicy] = useState(root?.policy || "agents");
  const [exclusions, setExclusions] = useState(
    (root?.exclusions || ["**/dist/**", "**/*.min.js", "**/.env*"]).join("\n"),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const choose = async () => {
    try {
      const chosen = await window.knowledge?.chooseDirectory();
      if (chosen) setPath(chosen);
    } catch (error) {
      setError(error instanceof Error ? error.message : "无法选择目录");
    }
  };
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      await request(
        root ? "PATCH" : "POST",
        root ? "/roots/" + root.id : "/roots",
        {
          path,
          name,
          policy,
          exclusions: exclusions
            .split("\n")
            .map((value) => value.trim())
            .filter(Boolean),
        },
      );
      await refresh();
      notify(root ? "目录设置已保存" : "资料目录已加入，后台正在建立索引。");
      onClose();
    } catch (error) {
      setError(error instanceof Error ? error.message : "保存失败");
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      title={root ? "管理资料目录" : "添加资料目录"}
      onClose={onClose}
      onSubmit={submit}
      busy={busy}
      submitLabel={root ? "保存" : "加入知识库"}
    >
      <label>
        资料目录
        <div className="input-action">
          <input
            required
            value={path}
            onChange={(event) => setPath(event.target.value)}
            placeholder={
              window.knowledge ? "选择资料文件夹" : "填写此电脑上的完整目录路径"
            }
          />
          {window.knowledge && (
            <button type="button" onClick={() => void choose()}>
              选择目录
            </button>
          )}
        </div>
      </label>
      <label>
        名称
        <input
          value={name}
          onChange={(event) => setName(event.target.value)}
          maxLength={100}
          placeholder="默认使用文件夹名称"
        />
      </label>
      <label>
        资料用途
        <select
          value={policy}
          onChange={(event) => setPolicy(event.target.value as Root["policy"])}
        >
          <option value="agents">允许公司模型与已连接的 Agent 使用</option>
          <option value="company">允许公司模型，仅在知识库窗口使用</option>
          <option value="local">仅本机检索和阅读</option>
        </select>
      </label>
      <details className="advanced">
        <summary>排除规则</summary>
        <label>
          每行一条规则
          <textarea
            rows={4}
            value={exclusions}
            onChange={(event) => setExclusions(event.target.value)}
          />
          <small>
            支持 **/*.tmp 这样的文件路径匹配。依赖缓存、Git 内部数据会自动跳过。
          </small>
        </label>
      </details>
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
