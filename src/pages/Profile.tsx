import { useState, type FormEvent } from "react";
import {
  PlusIcon,
  CheckIcon,
  PencilSimpleLineIcon,
  ClockCounterClockwiseIcon,
  TrashIcon,
} from "@phosphor-icons/react";
import { useKnowledge } from "../context";
import { request } from "../api";
import type { Memory } from "../types";
import { Empty, Modal, date } from "../components/ui";

const statuses: Record<string, string> = {
  pending: "待确认",
  confirmed: "已确认",
  stale: "待复核",
  dismissed: "已忽略",
};
const categories: Record<string, string> = {
  role: "工作方向",
  experience: "技术经验",
  preference: "个人偏好",
  decision: "项目决定",
};

export function Profile() {
  const { data, refresh, notify, openSource } = useKnowledge();
  const [filter, setFilter] = useState("pending");
  const [editing, setEditing] = useState<Memory | "new" | null>(null);
  const [removing, setRemoving] = useState<Memory | null>(null);
  const [history, setHistory] = useState<Memory | null>(null);
  const [busyId, setBusyId] = useState("");
  const change = async (id: string, status: string) => {
    setBusyId(id);
    try {
      await request("PATCH", "/memories/" + id, { status });
      await refresh();
      notify(status === "confirmed" ? "记忆已确认" : "记忆已忽略");
    } catch (error) {
      notify(error instanceof Error ? error.message : "操作失败", true);
    } finally {
      setBusyId("");
    }
  };
  const memories = data.memories
    .filter((memory) => filter === "all" || memory.status === filter)
    .sort((a, b) => b.updated_at.localeCompare(a.updated_at));
  return (
    <>
      <div className="breadcrumb">
        知识库 <span>/</span> <span>关于我</span>
      </div>
      <header className="page-heading">
        <div>
          <h1>关于我</h1>
          <p>在资料与经历中，慢慢整理出你确认的记忆。</p>
        </div>
        <button className="primary" onClick={() => setEditing("new")}>
          <PlusIcon size={17} />
          添加记忆
        </button>
      </header>
      <div className="tabs profile-tabs" aria-label="记忆状态">
        {[
          ["pending", "待确认"],
          ["confirmed", "已确认"],
          ["stale", "待复核"],
          ["all", "全部"],
        ].map(([value, label]) => (
          <button
            key={value}
            className={filter === value ? "selected" : ""}
            aria-pressed={filter === value}
            onClick={() => setFilter(value)}
          >
            {label}
            <span>
              {
                data.memories.filter(
                  (memory) => value === "all" || memory.status === value,
                ).length
              }
            </span>
          </button>
        ))}
      </div>
      {memories.length ? (
        <div className="memory-list">
          {memories.map((memory) => (
            <article className="memory-row" key={memory.id}>
              <div className="section-heading">
                <p className="memory-category">
                  {categories[memory.category] || memory.category}
                  {memory.project_id && (
                    <span className="muted">
                      {" "}
                      ·{" "}
                      {
                        data.projects.find(
                          (project) => project.id === memory.project_id,
                        )?.name
                      }
                    </span>
                  )}
                </p>
                <span className={"memory-status " + memory.status}>
                  {statuses[memory.status]}
                </span>
              </div>
              <p className="memory-content">{memory.content}</p>
              <div className="memory-evidence">
                {memory.evidence.length ? (
                  memory.evidence.map((evidence) => (
                    <button
                      className="inline-link"
                      key={evidence.document_id}
                      onClick={() =>
                        openSource(
                          evidence.document_id,
                          evidence.chunk_id || undefined,
                        )
                      }
                    >
                      {evidence.title}
                      {evidence.location ? " · " + evidence.location.label : ""}
                    </button>
                  ))
                ) : (
                  <span className="muted">
                    {memory.origin === "manual"
                      ? "手动记录"
                      : "来自会话的记忆建议"}
                  </span>
                )}
              </div>
              <div className="memory-footer">
                <div className="actions">
                  {memory.status === "pending" && (
                    <button
                      className="text-action"
                      disabled={busyId === memory.id}
                      onClick={() => void change(memory.id, "confirmed")}
                    >
                      <CheckIcon size={16} />
                      确认
                    </button>
                  )}
                  <button
                    className="text-action"
                    onClick={() => setEditing(memory)}
                  >
                    <PencilSimpleLineIcon size={16} />
                    修正
                  </button>
                  {["pending", "stale"].includes(memory.status) && (
                    <button
                      className="text-action"
                      disabled={busyId === memory.id}
                      onClick={() => void change(memory.id, "dismissed")}
                    >
                      忽略
                    </button>
                  )}
                  <button
                    className="text-action"
                    onClick={() => setHistory(memory)}
                    aria-label={"查看记忆修改记录"}
                  >
                    <ClockCounterClockwiseIcon size={16} />
                  </button>
                  <button
                    className="text-action danger-action"
                    onClick={() => setRemoving(memory)}
                    aria-label="删除记忆"
                  >
                    <TrashIcon size={16} />
                  </button>
                </div>
                <time>{date(memory.updated_at)}</time>
              </div>
              {memory.status === "stale" && (
                <p className="muted">
                  原资料已变化。请核对后修正，或将内容作为手动记忆保存。
                </p>
              )}
            </article>
          ))}
        </div>
      ) : (
        <Empty
          title={
            filter === "confirmed"
              ? "你确认过的记忆会出现在这里"
              : filter === "stale"
                ? "当前没有需要复核的记忆"
                : "当前没有待确认的记忆"
          }
        >
          你可以手动记录偏好，也可以连接公司模型，从项目资料中生成建议。
        </Empty>
      )}
      {editing && (
        <MemoryEditor
          memory={editing === "new" ? undefined : editing}
          onClose={() => setEditing(null)}
        />
      )}
      {history && (
        <Modal title="记忆修改记录" onClose={() => setHistory(null)}>
          <p>{history.content}</p>
          {history.history.length ? (
            <ol className="history-list">
              {history.history.map((entry: any, index) => (
                <li key={index}>
                  <time>{date(entry.at, true)}</time>
                  <p>{entry.content}</p>
                  <small>
                    {statuses[entry.status] || entry.status}
                    {entry.reason === "source_changed"
                      ? " · 原资料发生变化"
                      : ""}
                  </small>
                </li>
              ))}
            </ol>
          ) : (
            <p className="muted">这条记忆还没有修改记录。</p>
          )}
        </Modal>
      )}
      {removing && (
        <Modal
          title="删除记忆"
          onClose={() => setRemoving(null)}
          submitLabel="删除"
          onSubmit={(event) => {
            event.preventDefault();
            void request("DELETE", "/memories/" + removing.id)
              .then(async () => {
                await refresh();
                setRemoving(null);
                notify("记忆已删除");
              })
              .catch((error) => notify(error.message, true));
          }}
        >
          <p>{removing.content}</p>
          <p className="muted">删除后，这条记忆将从后续知识库查询中移除。</p>
        </Modal>
      )}
    </>
  );
}

function MemoryEditor({
  memory,
  onClose,
}: {
  memory?: Memory;
  onClose: () => void;
}) {
  const { data, refresh, notify } = useKnowledge();
  const [content, setContent] = useState(memory?.content || "");
  const [category, setCategory] = useState(memory?.category || "preference");
  const [project, setProject] = useState(memory?.project_id || "");
  const [detach, setDetach] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      if (memory)
        await request("PATCH", "/memories/" + memory.id, {
          content,
          status: "confirmed",
          detach_evidence: detach,
        });
      else {
        const created = await request<Memory>("POST", "/memories", {
          content,
          category,
          project_id: project || null,
          source_ids: [],
        });
        await request("PATCH", "/memories/" + created.id, {
          status: "confirmed",
        });
      }
      await refresh();
      notify("记忆已保存并确认");
      onClose();
    } catch (error) {
      setError(error instanceof Error ? error.message : "保存失败");
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      title={memory ? "修正记忆" : "添加记忆"}
      onClose={onClose}
      onSubmit={submit}
      submitLabel="保存并确认"
      busy={busy}
    >
      <label>
        记忆内容
        <textarea
          required
          rows={5}
          maxLength={2000}
          value={content}
          onChange={(event) => setContent(event.target.value)}
          placeholder="例如，技术选型时优先给出实验依据和原文位置。"
        />
      </label>
      {!memory && (
        <>
          <label>
            类型
            <select
              value={category}
              onChange={(event) => setCategory(event.target.value)}
            >
              {Object.entries(categories).map(([value, label]) => (
                <option value={value} key={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label>
            所属项目
            <select
              value={project}
              onChange={(event) => setProject(event.target.value)}
            >
              <option value="">个人长期记忆</option>
              {data.projects.map((project) => (
                <option value={project.id} key={project.id}>
                  {project.name}
                </option>
              ))}
            </select>
          </label>
        </>
      )}
      {memory?.status === "stale" && (
        <label className="checkbox-label">
          <input
            type="checkbox"
            checked={detach}
            onChange={(event) => setDetach(event.target.checked)}
          />
          将修正后的内容作为手动记忆，解除旧资料关联
        </label>
      )}
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
