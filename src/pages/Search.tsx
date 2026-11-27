import { useEffect, useRef, useState, type FormEvent } from "react";
import { MagnifyingGlassIcon, ArrowUpRightIcon } from "@phosphor-icons/react";
import ReactMarkdown from "react-markdown";
import { request } from "../api";
import { useKnowledge } from "../context";
import type { SearchResult } from "../types";
import {
  ScopePicker,
  scopeParameters,
  validScope,
} from "../components/ScopePicker";
import { Select, Busy, Empty, FileIcon } from "../components/ui";

let submittedQuestion: { query: string; project: string } | null = null;

export function QuestionBar({
  compact = false,
  initialProject,
}: {
  compact?: boolean;
  initialProject?: string;
}) {
  const { navigate, projectId } = useKnowledge();
  const [value, setValue] = useState("");
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!value.trim()) return;
    submittedQuestion = {
      query: value.trim(),
      project: initialProject || projectId,
    };
    navigate("search");
  };
  return (
    <form
      className={"question-bar" + (compact ? " question-compact" : "")}
      onSubmit={submit}
    >
      <label className="sr-only" htmlFor="project-question">
        问这个项目
      </label>
      <input
        id="project-question"
        required
        maxLength={4000}
        placeholder="问这个项目…"
        value={value}
        onChange={(event) => setValue(event.target.value)}
      />
      <button className="primary" type="submit">
        提问
      </button>
    </form>
  );
}

export function Search() {
  const { data, openSource } = useKnowledge();
  const initial = submittedQuestion;
  const [query, setQuery] = useState(initial?.query || "");
  const [scope, setScope] = useState(initial?.project || "");
  const effectiveScope = validScope(scope, data) ? scope : "";
  const [extension, setExtension] = useState("");
  const [result, setResult] = useState<SearchResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [lastQuery, setLastQuery] = useState("");
  const [searchOnly, setSearchOnly] = useState(false);
  const requestSequence = useRef(0);
  const lastRevision = useRef(data.library_revision);
  const run = async (
    text: string,
    project: string,
    only = false,
    fileType = extension,
  ) => {
    if (!text.trim()) return;
    const sequence = ++requestSequence.current;
    setBusy(true);
    setError("");
    setLastQuery(text);
    setResult(null);
    try {
      const value = await request<SearchResult>(
        "POST",
        only ? "/search" : "/ask",
        {
          query: text.trim(),
          ...scopeParameters(project),
          limit: 12,
          extension: fileType || null,
          updated_after: null,
        },
      );
      if (sequence === requestSequence.current) setResult(value);
    } catch (error) {
      if (sequence === requestSequence.current)
        setError(error instanceof Error ? error.message : "查询失败");
    } finally {
      if (sequence === requestSequence.current) setBusy(false);
    }
  };
  useEffect(() => {
    if (lastRevision.current === data.library_revision) return;
    lastRevision.current = data.library_revision;
    requestSequence.current++;
    setResult(null);
    setBusy(false);
  }, [data.library_revision]);
  useEffect(() => {
    const pending = submittedQuestion;
    submittedQuestion = null;
    if (pending) {
      setQuery(pending.query);
      setScope(pending.project);
      setExtension("");
      setSearchOnly(false);
      void run(pending.query, pending.project, false, "");
    }
  }, []);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    void run(query, effectiveScope, searchOnly);
  };
  const answer = result?.answer?.replace(
    /\[(\d+)\]/g,
    (_match, number) => `[${number}](source:${number})`,
  );
  return (
    <>
      <div className="breadcrumb">
        知识库 <span>/</span> <span>搜索与问答</span>
      </div>
      <header className="page-heading">
        <h1>搜索与问答</h1>
      </header>
      <form onSubmit={submit} className="search-form">
        <div className="question-bar" data-guide="search">
          <MagnifyingGlassIcon size={21} weight="regular" aria-hidden />
          <label className="sr-only" htmlFor="search-question">
            问题或关键词
          </label>
          <input
            id="search-question"
            required
            autoComplete="off"
            maxLength={4000}
            placeholder="搜索资料，或问一个问题"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
          <button className="primary" type="submit" disabled={busy}>
            {busy ? <Busy /> : searchOnly ? "搜索" : "查询"}
          </button>
        </div>
        <div className="search-options">
          <label>
            资料范围
            <ScopePicker
              data={data}
              aria-label="项目范围"
              value={effectiveScope}
              onChange={(event) => setScope(event.target.value)}
            />
          </label>
          <label>
            文件类型
            <Select
              aria-label="文件类型"
              value={extension}
              onChange={(event) => setExtension(event.target.value)}
            >
              <option value="">全部类型</option>
              <option value="md">Markdown</option>
              <option value="py">Python</option>
              <option value="pdf">PDF</option>
              <option value="docx">Word</option>
              <option value="xlsx">Excel</option>
              <option value="pptx">PowerPoint</option>
              <option value="csv">CSV</option>
            </Select>
          </label>
          <label className="checkbox-label">
            <input
              type="checkbox"
              checked={searchOnly}
              onChange={(event) => setSearchOnly(event.target.checked)}
            />
            仅搜索原文
          </label>
        </div>
      </form>
      {busy && (
        <div className="search-pending">
          <Busy>正在检索资料{searchOnly ? "" : "并组织回答"}…</Busy>
        </div>
      )}
      {error && (
        <div className="notice error" role="alert">
          {error}
          <button onClick={() => void run(query, scope, searchOnly)}>
            重试
          </button>
        </div>
      )}
      {result && (
        <div className="search-results" aria-live="polite">
          {result.notice && <p className="notice">{result.notice}</p>}
          {answer && (
            <article className="answer">
              <h2>{lastQuery}</h2>
              <ReactMarkdown
                skipHtml
                urlTransform={(url) => (url.startsWith("source:") ? url : "")}
                components={{
                  a: ({ href, children }) => {
                    const index = Number(href?.split(":")[1]) - 1;
                    const source = result.hits[index];
                    return source ? (
                      <button
                        className="citation"
                        onClick={() =>
                          openSource(source.document_id, source.chunk_id)
                        }
                        aria-label={"查看来源 " + source.title}
                      >
                        {children}
                      </button>
                    ) : (
                      <span>{children}</span>
                    );
                  },
                }}
              >
                {answer}
              </ReactMarkdown>
              <p className="answer-model">
                {result.model ? "由公司模型 " + result.model + " 整理" : ""}
              </p>
            </article>
          )}
          {result.hits.length > 0 ? (
            <section className="retrieved-sources">
              <div className="section-heading">
                <h2>相关资料</h2>
                <span className="muted">
                  {result.mode.includes("hybrid")
                    ? "全文与语义检索"
                    : "全文检索"}
                </span>
              </div>
              {result.hits.map((hit) => (
                <button
                  className="search-hit"
                  key={hit.chunk_id}
                  onClick={() => openSource(hit.document_id, hit.chunk_id)}
                >
                  <FileIcon extension={hit.title.split(".").at(-1) || ""} />
                  <span>
                    <strong>{hit.title}</strong>
                    <small>
                      {hit.project_name} · {hit.location.label}
                    </small>
                    <span className="hit-excerpt">
                      {hit.text.slice(0, 240)}
                    </span>
                  </span>
                  <ArrowUpRightIcon size={17} aria-hidden />
                </button>
              ))}
            </section>
          ) : (
            <Empty title="没有找到相关资料">调整关键词或检索范围后重试。</Empty>
          )}
        </div>
      )}
      {!result && !busy && !error && (
        <Empty title="输入关键词或问题">可按项目和文件类型筛选检索范围。</Empty>
      )}
    </>
  );
}
