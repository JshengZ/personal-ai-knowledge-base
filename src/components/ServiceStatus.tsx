import { useEffect, useState } from "react";
import { request } from "../api";
import { useKnowledge } from "../context";
import { CaretDownIcon } from "@phosphor-icons/react";
interface RuntimeStatus {
  pid: number;
  role: string;
  version: string;
  uptime_seconds: number;
  maintenance: boolean;
  queries: {
    active: number;
    capacity: number;
    keyword_active: number;
    keyword_capacity: number;
  };
  active_tasks: {
    kind: string;
    started_at: string;
    details: {
      path?: string;
      name?: string;
      tool?: string;
      model?: string;
      batch_chunks?: number;
    };
  }[];
  watchers: Record<string, { active: boolean; error: string | null }>;
  source_activity: {
    root_id: string;
    changes_10m: number;
    high_activity: boolean;
  }[];
  index?: {
    queue: {
      queued: number;
      waiting: number;
      ready: number;
      next_due_at: number | null;
    };
    paused: boolean;
    quiet_seconds: number;
    reconcile_seconds: number;
    semantic: {
      mode: string;
      total_chunks: number;
      completed_vectors: number;
      estimated_seconds: number | null;
    };
  };
  index_error?: string;
  audit_events_dropped: number;
  watcher_events_dropped: number;
}
const taskNames: Record<string, string> = {
  file_index: "文档处理",
  embedding: "向量计算",
  scan: "目录核对",
  query: "查询",
};
export function ServiceStatus() {
  const { data } = useKnowledge();
  const [status, setStatus] = useState<RuntimeStatus | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true,
      busy = false;
    const poll = async () => {
      if (busy || document.hidden) return;
      busy = true;
      try {
        const value = await request<RuntimeStatus>("GET", "/status");
        if (active) {
          setStatus(value);
          setError("");
        }
      } catch (error) {
        if (active)
          setError(error instanceof Error ? error.message : "运行状态暂不可用");
      } finally {
        busy = false;
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 5000);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, []);
  const queue = status?.index?.queue,
    semantic = status?.index?.semantic;
  const due = queue?.next_due_at
    ? Math.max(0, queue.next_due_at - Math.floor(Date.now() / 1000))
    : 0;
  const high =
    status?.source_activity.filter(
      (source) =>
        source.high_activity &&
        data.roots.some((root) => root.id === source.root_id),
    ) || [];
  return (
    <div className="runtime-status">
      <details>
        <summary>
          <span>运行状态</span>
          <span className="muted">
            {status
              ? `查询 ${status.queries.active}/${status.queries.capacity} · 待处理 ${queue?.queued ?? "—"} · 等待静默 ${queue?.waiting ?? "—"}`
              : error || "正在读取"}
          </span>
          <CaretDownIcon
            size={14}
            className="runtime-chevron"
            aria-hidden="true"
          />
        </summary>
        {status && (
          <div className="runtime-details">
            <dl className="runtime-metrics">
              <div>
                <dt>后台服务</dt>
                <dd>
                  PID {status.pid} · {status.role} · v{status.version}
                </dd>
              </div>
              <div>
                <dt>预留全文通道</dt>
                <dd>
                  {status.queries.keyword_active} /{" "}
                  {status.queries.keyword_capacity}
                </dd>
              </div>
              <div>
                <dt>文件队列</dt>
                <dd>
                  {queue?.ready ?? "—"} 可处理 · {queue?.waiting ?? "—"}{" "}
                  等待静默{due > 0 ? ` · 最近 ${due} 秒后可处理` : ""}
                </dd>
              </div>
              <div>
                <dt>向量进度</dt>
                <dd>
                  {semantic?.mode === "disabled"
                    ? "已关闭 · 全文可用"
                    : semantic
                      ? `${semantic.completed_vectors.toLocaleString()} / ${semantic.total_chunks.toLocaleString()}`
                      : "暂不可用"}
                  {semantic?.estimated_seconds
                    ? ` · 预计 ${Math.ceil(semantic.estimated_seconds / 60)} 分钟`
                    : ""}
                </dd>
              </div>
              <div>
                <dt>文件监听</dt>
                <dd>
                  {
                    Object.values(status.watchers).filter(
                      (watcher) => watcher.active,
                    ).length
                  }{" "}
                  个目录使用原生监听 · 兜底核对{" "}
                  {status.index
                    ? Math.round(status.index.reconcile_seconds / 60)
                    : "—"}{" "}
                  分钟
                </dd>
              </div>
              <div>
                <dt>服务状态</dt>
                <dd>
                  {status.maintenance
                    ? "备份、恢复或迁移中"
                    : status.index?.paused
                      ? "新增同步已暂停"
                      : "可接受查询"}{" "}
                  · 静默窗口{" "}
                  {status.index?.quiet_seconds
                    ? `${status.index.quiet_seconds / 60} 分钟`
                    : "立即处理"}
                </dd>
              </div>
            </dl>
            {status.active_tasks.length ? (
              <ul className="runtime-tasks">
                {status.active_tasks.map((task, index) => (
                  <li key={task.started_at + index}>
                    <strong>{taskNames[task.kind] || task.kind}</strong>
                    <span>
                      {task.details.path ||
                        task.details.name ||
                        task.details.tool ||
                        task.details.model ||
                        "处理中"}
                    </span>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="muted">当前没有活动任务。</p>
            )}
            {Object.entries(status.watchers)
              .filter(([, watcher]) => !watcher.active)
              .map(([id, watcher]) => (
                <p className="form-error" key={id}>
                  {data.roots.find((root) => root.id === id)?.name ||
                    "资料目录"}
                  ：监听未生效，使用兜底核对。{watcher.error}
                </p>
              ))}
            {status.index_error && (
              <p className="form-error">{status.index_error}</p>
            )}
            {(status.audit_events_dropped > 0 ||
              status.watcher_events_dropped > 0) && (
              <p className="notice">
                事件缓冲已溢出：监听 {status.watcher_events_dropped}，调用记录{" "}
                {status.audit_events_dropped}。监听溢出会触发补扫。
              </p>
            )}
          </div>
        )}
        {error && status && <p className="form-error">{error}</p>}
      </details>
      {high.map((source) => (
        <p className="notice" key={source.root_id}>
          {data.roots.find((root) => root.id === source.root_id)?.name}：近 10
          分钟检测到 {source.changes_10m}{" "}
          次变更。文件会在静默后处理；可检查构建、缓存和临时目录的排除范围。
        </p>
      ))}
    </div>
  );
}
