import { useEffect, useState, type FormEvent } from "react";
import {
  PencilSimpleLineIcon,
  ArrowUpRightIcon,
  PlusIcon,
} from "@phosphor-icons/react";
import { useKnowledge } from "../context";
import { request } from "../api";
import { Busy, Empty, FileIcon, Modal, date } from "../components/ui";
import type { Project, Document } from "../types";
import { QuestionBar } from "./Search";

export function Projects() {
  const { data, projectId, addRoot, openSource, notify, refresh, navigate } =
    useKnowledge();
  const [editing, setEditing] = useState(false);
  const [summarizing, setSummarizing] = useState(false);
  const [projectFiles, setProjectFiles] = useState<{
    id: string;
    documents: Document[];
  } | null>(null);
  useEffect(() => {
    if (!projectId) return;
    let active = true;
    request<{ documents: Document[] }>(
      "GET",
      "/documents?project_id=" + encodeURIComponent(projectId) + "&limit=5",
    )
      .then((result) => {
        if (active)
          setProjectFiles({ id: projectId, documents: result.documents });
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [projectId, data.last_indexed, data.stats.documents]);
  const project = data.projects.find((project) => project.id === projectId);
  if (!project)
    return (
      <Empty
        title="从一个项目开始"
        action={
          <button className="primary" onClick={addRoot}>
            <PlusIcon size={17} />
            添加资料目录
          </button>
        }
      >
        选择你的项目或文档目录，资料会在后台持续更新。
      </Empty>
    );
  const documents =
    projectFiles?.id === projectId
      ? projectFiles.documents
      : data.documents.filter((document) => document.project_id === projectId);
  const summarize = async () => {
    if (!data.settings.chat.base_url) {
      navigate("settings");
      notify("请先连接公司的对话模型。");
      return;
    }
    setSummarizing(true);
    try {
      await request("POST", "/projects/" + projectId + "/summarize", {});
      await refresh();
      notify("项目摘要已更新，新的记忆建议可在「关于我」中确认。");
    } catch (error) {
      notify(error instanceof Error ? error.message : "摘要生成失败", true);
    } finally {
      setSummarizing(false);
    }
  };
  return (
    <>
      <div className="breadcrumb">
        项目 <span>/</span> <span>{project.name}</span>
      </div>
      <QuestionBar compact initialProject={projectId} />
      <header className="project-heading">
        <div>
          <h1>{project.name}</h1>
          <p>最近更新于 {date(project.updated_at)}</p>
        </div>
        <button className="text-action" onClick={() => setEditing(true)}>
          <PencilSimpleLineIcon size={18} weight="regular" />
          编辑项目
        </button>
      </header>
      {project.summary_stale && (
        <div className="notice">
          <span>引用资料有变化，项目摘要需要更新。</span>
          <button
            className="text-action"
            onClick={() => void summarize()}
            disabled={summarizing}
          >
            {summarizing ? <Busy /> : "更新摘要"}
          </button>
        </div>
      )}
      <div className="project-content">
        <article className="project-brief">
          <section>
            <h2>项目目标</h2>
            {project.goal ? (
              <p>{project.goal}</p>
            ) : (
              <p className="muted">补充项目目标，或让公司模型根据资料整理。</p>
            )}
          </section>
          <section>
            <h2>当前方案</h2>
            {project.approach ? (
              <p>
                {project.approach}{" "}
                {project.summary_evidence.length > 0 && (
                  <button
                    className="inline-link"
                    onClick={() =>
                      openSource(
                        project.summary_evidence[0].document_id,
                        project.summary_evidence[0].chunk_id || undefined,
                      )
                    }
                  >
                    查看依据
                  </button>
                )}
              </p>
            ) : (
              <p className="muted">
                项目资料就绪后，可以生成带依据的方案摘要。
              </p>
            )}
          </section>
          <section>
            <h2>关键决策</h2>
            {project.decisions.length > 0 ? (
              <ol className="decision-list">
                {project.decisions.map((decision, index) => (
                  <li key={index}>
                    <time>{decision.date}</time>
                    <span>{decision.text}</span>
                  </li>
                ))}
              </ol>
            ) : (
              <p className="muted">你可以手动记录，或从项目资料中整理。</p>
            )}
          </section>
          <div className="brief-actions">
            <button
              className="text-action"
              onClick={() => void summarize()}
              disabled={summarizing}
            >
              {summarizing ? (
                <Busy>正在整理资料</Busy>
              ) : (
                <>
                  根据资料整理
                  <ArrowUpRightIcon size={16} />
                </>
              )}
            </button>
          </div>
        </article>
        <aside className="project-documents">
          <h2>项目资料</h2>
          {documents.length > 0 ? (
            <div className="document-list">
              {documents.slice(0, 5).map((document) => (
                <button
                  className="document-row"
                  key={document.id}
                  onClick={() => openSource(document.id)}
                  disabled={document.status !== "ready"}
                >
                  <FileIcon extension={document.extension} />
                  <span>
                    <strong>{document.title}</strong>
                    <small>
                      {document.status === "ready"
                        ? document.ocr
                          ? "文字内容 · 含 OCR"
                          : document.extension.toUpperCase() +
                            " · " +
                            document.chunks +
                            " 个片段"
                        : document.status === "error"
                          ? "处理失败，可在资料页重试"
                          : document.status === "empty"
                            ? "未提取到文字"
                            : "正在索引"}
                    </small>
                  </span>
                  <ArrowUpRightIcon className="row-arrow" size={16} />
                </button>
              ))}
            </div>
          ) : (
            <p className="muted">正在等待目录扫描。</p>
          )}
          <button
            className="inline-link all-documents"
            onClick={() => navigate("sources")}
          >
            查看全部资料
          </button>
        </aside>
      </div>
      {editing && (
        <ProjectEditor project={project} onClose={() => setEditing(false)} />
      )}
    </>
  );
}

function ProjectEditor({
  project,
  onClose,
}: {
  project: Project;
  onClose: () => void;
}) {
  const { refresh, notify } = useKnowledge();
  const [name, setName] = useState(project.name);
  const [goal, setGoal] = useState(project.goal);
  const [approach, setApproach] = useState(project.approach);
  const [decisions, setDecisions] = useState(
    project.decisions
      .map((decision) => decision.date + " | " + decision.text)
      .join("\n"),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const save = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError("");
    const rows = decisions
      .split("\n")
      .filter((row) => row.trim())
      .map((row) => {
        const separator = row.indexOf("|");
        return separator < 0
          ? { date: "", text: row.trim() }
          : {
              date: row.slice(0, separator).trim(),
              text: row.slice(separator + 1).trim(),
            };
      });
    try {
      await request("PATCH", "/projects/" + project.id, {
        name,
        goal,
        approach,
        decisions: rows,
      });
      await refresh();
      notify("项目已保存");
      onClose();
    } catch (error) {
      setError(error instanceof Error ? error.message : "保存失败");
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal title="编辑项目" onClose={onClose} onSubmit={save} busy={busy}>
      <label>
        项目名称
        <input
          required
          maxLength={100}
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
      </label>
      <label>
        项目目标
        <textarea
          rows={3}
          maxLength={2000}
          value={goal}
          onChange={(event) => setGoal(event.target.value)}
        />
      </label>
      <label>
        当前方案
        <textarea
          rows={4}
          maxLength={4000}
          value={approach}
          onChange={(event) => setApproach(event.target.value)}
        />
      </label>
      <label>
        关键决策
        <textarea
          rows={4}
          value={decisions}
          onChange={(event) => setDecisions(event.target.value)}
          placeholder="2026-11-25 | 采用的方案或项目决定"
        />
        <small>每行一条，可以用日期和竖线分隔。</small>
      </label>
      <p className="muted">手动保存的内容作为你确认的项目记录。</p>
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
