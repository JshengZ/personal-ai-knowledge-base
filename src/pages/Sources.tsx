import {
  useDeferredValue,
  useEffect,
  useRef,
  useState,
  type FormEvent,
} from "react";
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
import { Select, Empty, FileIcon, Modal, date } from "../components/ui";
import { DirectoryBatchDialog } from "../components/DirectoryBatchDialog";
import {
  IndexFilterEditor,
  defaultIndexFilters,
} from "../components/IndexFilterEditor";
import {
  ScopePicker,
  scopeParameters,
  validScope,
} from "../components/ScopePicker";
import { ServiceStatus } from "../components/ServiceStatus";
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
  const effectiveScope = validScope(scope, data) ? scope : "";
  const [excluding, setExcluding] = useState<Document | null>(null);
  const [excludeScope, setExcludeScope] = useState("file");
  const [status, setStatus] = useState("");
  const [documents, setDocuments] = useState<Document[]>([]);
  const [total, setTotal] = useState(0);
  const [editing, setEditing] = useState<Root | null>(null);
  const [removing, setRemoving] = useState<Root | null>(null);
  const [busy, setBusy] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [loading, setLoading] = useState(false);
  const [expanding, setExpanding] = useState(false);
  const [refreshCount, setRefreshCount] = useState(0);
  const visibleCount = useRef(100);
  const filterKey = JSON.stringify([
    effectiveScope,
    deferred,
    status,
    data.library_revision,
  ]);
  const currentFilter = useRef("");
  const mounted = useRef(true);
  const running = useRef(false);
  const pending = useRef<{
    key: string;
    parameters: URLSearchParams;
    count: number;
  } | null>(null);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    if (currentFilter.current !== filterKey) {
      currentFilter.current = filterKey;
      visibleCount.current = 100;
      setDocuments([]);
      setTotal(0);
      setLoadError("");
      setExpanding(false);
    }
    pending.current = {
      key: filterKey,
      parameters: new URLSearchParams({
        ...scopeParameters(effectiveScope),
        ...(deferred ? { query: deferred } : {}),
        ...(status ? { status } : {}),
      }),
      count: visibleCount.current,
    };
    if (running.current) return;
    running.current = true;
    setLoading(true);
    // Coalesce background refreshes. A slow IPC request cannot be aborted, so
    // finish it before starting the latest refresh instead of piling up calls.
    void (async () => {
      while (pending.current && mounted.current) {
        const next = pending.current;
        pending.current = null;
        try {
          const rows: Document[] = [];
          let available = 0;
          for (let offset = 0; offset < next.count; offset += 200) {
            const parameters = new URLSearchParams(next.parameters);
            parameters.set("offset", String(offset));
            parameters.set("limit", String(Math.min(200, next.count - offset)));
            const result = await request<{
              documents: Document[];
              total: number;
            }>("GET", "/documents?" + parameters);
            rows.push(...result.documents);
            available = result.total;
            if (
              offset + result.documents.length >= available ||
              !result.documents.length
            )
              break;
            if (!mounted.current || currentFilter.current !== next.key) break;
          }
          if (mounted.current && currentFilter.current === next.key) {
            setDocuments([
              ...new Map(rows.map((row) => [row.id, row])).values(),
            ]);
            setTotal(available);
            setLoadError("");
            if (rows.length >= visibleCount.current || rows.length >= available)
              setExpanding(false);
          }
        } catch (error) {
          if (mounted.current && currentFilter.current === next.key) {
            setLoadError(
              error instanceof Error ? error.message : "资料列表加载失败",
            );
            setExpanding(false);
          }
        }
      }
    })().finally(() => {
      running.current = false;
      if (mounted.current) setLoading(false);
    });
  }, [
    filterKey,
    effectiveScope,
    deferred,
    status,
    data.library_revision,
    data.stats.cleaning,
    data.stats.documents,
    data.last_indexed,
    data.last_scan,
    data.stats.pending,
    refreshCount,
  ]);
  const loadMore = () => {
    setExpanding(true);
    visibleCount.current = Math.max(100, documents.length) + 100;
    setRefreshCount((value) => value + 1);
  };
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
          <p>管理资料目录和索引状态。</p>
        </div>
        <button className="primary" onClick={addRoot} data-guide="add-source">
          <PlusIcon size={17} />
          添加资料目录
        </button>
      </header>
      <IndexProgress data={data} onSettings={() => navigate("settings")} />
      <ServiceStatus />
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
              资料范围
            </label>
            <ScopePicker
              data={data}
              id="file-project"
              value={effectiveScope}
              onChange={(event) => setScope(event.target.value)}
            />

            <label className="sr-only" htmlFor="file-status">
              索引状态
            </label>
            <Select
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
            </Select>
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
              {documents.length ? "。已保留上次加载的资料。" : ""}
              <button
                type="button"
                className="text-action"
                disabled={expanding}
                onClick={() => setRefreshCount((value) => value + 1)}
              >
                重试
              </button>
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
                  <div className="file-row-actions">
                    <time>{date(document.updated_at)}</time>
                    <button
                      type="button"
                      className="text-action"
                      aria-label={`排除索引 ${document.title}`}
                      onClick={() => {
                        setExcluding(document);
                        setExcludeScope("file");
                      }}
                    >
                      排除
                    </button>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <Empty
              title={
                loadError
                  ? "资料暂未加载"
                  : loading
                    ? "正在读取资料"
                    : query || status
                      ? "没有符合条件的文件"
                      : "暂无资料"
              }
              action={
                !loadError && !loading && !data.roots.length ? (
                  <button onClick={addRoot}>选择资料目录</button>
                ) : undefined
              }
            >
              {loadError
                ? "请重试加载。已构建的索引不会因此丢失。"
                : loading
                  ? "正在读取文件列表。"
                  : data.stats.pending
                    ? "正在处理文件，完成后会显示在这里。"
                    : "可以添加目录，或调整当前筛选条件。"}
            </Empty>
          )}
          {documents.length < total && (
            <button onClick={loadMore} disabled={expanding}>
              {expanding ? "正在加载" : "加载更多"}
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
                {data.scan_summaries?.find((item) => item.root_id === root.id)
                  ?.summary &&
                  (() => {
                    const summary = data.scan_summaries!.find(
                      (item) => item.root_id === root.id,
                    )!.summary!;
                    return (
                      <details className="scan-summary">
                        <summary>
                          可索引 {summary.eligible_files} 份 · 已跳过{" "}
                          {summary.skipped_files} 个文件、
                          {summary.skipped_directories} 个目录
                        </summary>
                        <p className="muted">
                          被跳过目录的内部文件未逐个统计。原始文件保留。
                        </p>
                        <dl>
                          {Object.entries(summary.reasons).map(
                            ([reason, count]) => (
                              <div key={reason}>
                                <dt>{reason}</dt>
                                <dd>{count}</dd>
                              </div>
                            ),
                          )}
                        </dl>
                        {summary.examples.map((item, index) => (
                          <p className="path-text" key={index}>
                            {item.path || "此目录"} · {item.reason}
                          </p>
                        ))}
                      </details>
                    );
                  })()}
                {data.projects.filter((project) => project.root_id === root.id)
                  .length >= 20 && (
                  <p className="notice">
                    此集合已识别{" "}
                    {
                      data.projects.filter(
                        (project) => project.root_id === root.id,
                      ).length
                    }{" "}
                    个项目。请检查是否包含插件、依赖或临时目录；可在管理目录中调整排除范围。
                  </p>
                )}
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
              title="暂无资料目录"
              action={
                <button className="primary" onClick={addRoot}>
                  添加资料目录
                </button>
              }
            >
              添加包含文档、笔记或代码的文件夹。
            </Empty>
          )}
        </div>
      )}
      {excluding && (
        <Modal
          title="排除索引"
          onClose={() => setExcluding(null)}
          onSubmit={(event) => {
            event.preventDefault();
            void action(async () => {
              await request("POST", "/sources/" + excluding.id + "/exclude", {
                scope: excludeScope,
              });
              setExcluding(null);
            }, "排除规则已保存，正在回收索引。");
          }}
          submitLabel="排除索引"
          busy={busy}
        >
          <p>{excluding.title}</p>
          <label>
            排除范围
            <Select
              aria-label="排除范围"
              value={excludeScope}
              onChange={(event) => setExcludeScope(event.target.value)}
            >
              <option value="file">仅此文件</option>
              <option value="directory">所在目录及其子目录</option>
              <option value="extension">此集合内的同类文件</option>
            </Select>
          </label>
          <p className="muted">
            后续新增文件会遵守此规则。原始文件保留，可在管理目录中恢复纳入。
          </p>
        </Modal>
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

function EditRootDialog({
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
  const [filters, setFilters] = useState(root?.filters || defaultIndexFilters);
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
          filters,
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
      className="scrollable-form"
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
            aria-label="资料目录"
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
        <Select
          aria-label="资料用途"
          value={policy}
          onChange={(event) => setPolicy(event.target.value as Root["policy"])}
        >
          <option value="agents">允许公司模型与已连接的 Agent 使用</option>
          <option value="company">允许公司模型，仅在知识库窗口使用</option>
          <option value="local">仅本机检索和阅读</option>
        </Select>
      </label>
      <IndexFilterEditor
        value={filters}
        exclusions={exclusions}
        onChange={setFilters}
        onExclusions={setExclusions}
      />

      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}

export function RootDialog({
  root,
  onClose,
}: {
  root?: Root;
  onClose: () => void;
}) {
  return root ? (
    <EditRootDialog root={root} onClose={onClose} />
  ) : (
    <DirectoryBatchDialog onClose={onClose} />
  );
}
