import { useEffect, useRef, useState, type FormEvent } from "react";
import { FolderPlusIcon, XIcon } from "@phosphor-icons/react";
import { request } from "../api";
import { useKnowledge } from "../context";
import type { Root } from "../types";
import { Modal, Select } from "./ui";
import { defaultIndexFilters, IndexFilterEditor } from "./IndexFilterEditor";

type Entry = {
  id: number;
  path: string;
  name: string;
  error?: string;
  warning?: string;
  advice?: string;
};
const basename = (path: string) =>
  path
    .replace(/[/\\]+$/, "")
    .split(/[/\\]/)
    .at(-1) || "";
const key = (path: string) => {
  const value = path
    .replace(/^\\\\\?\\/, "")
    .replaceAll("\\", "/")
    .replace(/\/$/, "");
  return /^[a-z]:/i.test(value) ? value.toLowerCase() : value;
};
export function DirectoryBatchDialog({ onClose }: { onClose: () => void }) {
  const { refresh, notify } = useKnowledge();
  const [entries, setEntries] = useState<Entry[]>([]);
  const [policy, setPolicy] = useState<Root["policy"]>("agents");
  const [filters, setFilters] = useState(defaultIndexFilters);
  const [exclusions, setExclusions] = useState(
    "**/dist/**\n**/*.min.js\n**/.env*",
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [manual, setManual] = useState("");
  const sequence = useRef(0),
    nextId = useRef(0);
  const append = (paths: string[]) =>
    setEntries((previous) => {
      const known = new Set(previous.map((entry) => key(entry.path)));
      return [
        ...previous,
        ...paths
          .filter((path) => {
            if (!path.trim() || known.has(key(path))) return false;
            known.add(key(path));
            return true;
          })
          .map((path) => ({
            id: ++nextId.current,
            path,
            name: basename(path),
          })),
      ];
    });
  const choose = async () => {
    try {
      const paths = await window.knowledge?.chooseDirectories();
      if (paths) append(paths);
    } catch (error) {
      setError(error instanceof Error ? error.message : "无法选择目录");
    }
  };
  const paths = JSON.stringify(entries.map(({ id, path }) => ({ id, path })));
  useEffect(() => {
    const current = ++sequence.current;
    if (!entries.length) return;
    const timer = setTimeout(() => {
      void request<{
        items: {
          path: string;
          error?: string;
          warning?: string;
          advice?: string;
        }[];
      }>("POST", "/roots/inspect", {
        items: entries.map(({ path, name }) => ({
          path,
          name,
          policy,
          filters,
          exclusions: exclusions.split("\n").filter(Boolean),
        })),
      })
        .then((result) => {
          if (sequence.current === current)
            setEntries((previous) =>
              previous.map((entry) => ({
                ...entry,
                error: result.items.find((item) => item.path === entry.path)
                  ?.error,
                warning: result.items.find((item) => item.path === entry.path)
                  ?.warning,
                advice: result.items.find((item) => item.path === entry.path)
                  ?.advice,
              })),
            );
        })
        .catch((error) => {
          if (sequence.current === current) setError(error.message);
        });
    }, 250);
    return () => {
      clearTimeout(timer);
      if (sequence.current === current) sequence.current++;
    };
  }, [paths, filters, policy, exclusions]);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      const result = await request<{
        added: Root[];
        errors: { path: string; error: string }[];
      }>("POST", "/roots/batch", {
        items: entries.map(({ path, name }) => ({
          path,
          name,
          policy,
          filters,
          exclusions: exclusions
            .split("\n")
            .map((value) => value.trim())
            .filter(Boolean),
        })),
      });
      await refresh();
      if (result.errors.length) {
        setEntries((previous) =>
          previous
            .filter((entry) =>
              result.errors.some((item) => item.path === entry.path),
            )
            .map((entry) => ({
              ...entry,
              error: result.errors.find((item) => item.path === entry.path)
                ?.error,
            })),
        );
        setError(
          `已添加 ${result.added.length} 个目录，${result.errors.length} 个未添加。请处理下方原因后重试。`,
        );
      } else {
        notify(`已添加 ${result.added.length} 个资料集合，后台正在建立索引。`);
        onClose();
      }
    } catch (error) {
      setError(error instanceof Error ? error.message : "添加失败");
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      title="添加资料目录"
      wide
      className="scrollable-form"
      onClose={onClose}
      onSubmit={submit}
      submitLabel="加入知识库"
      busy={busy}
      submitDisabled={!entries.length || entries.length > 50}
    >
      <fieldset
        disabled={busy}
        className="directory-batch-form"
        aria-label="资料目录与索引设置"
      >
        <div className="directory-select-heading">
          {window.knowledge ? (
            <button type="button" onClick={() => void choose()} disabled={busy}>
              <FolderPlusIcon size={17} />
              选择目录
            </button>
          ) : (
            <div className="input-action">
              <input
                aria-label="目录路径"
                placeholder="填写资料目录路径"
                value={manual}
                onChange={(event) => setManual(event.target.value)}
              />
              <button
                type="button"
                onClick={() => {
                  append([manual]);
                  setManual("");
                }}
              >
                添加到列表
              </button>
            </div>
          )}
          <p className="muted">
            可一次多选，也可继续添加不同位置的目录。每个目录作为独立资料集合。
          </p>
        </div>
        {entries.length > 50 && (
          <p className="form-error" role="alert">
            每次最多添加 50 个目录，请移除多余目录后提交。
          </p>
        )}
        <div className="directory-batch-list">
          {entries.map((entry) => (
            <section
              className="directory-batch-row"
              key={entry.id}
              aria-label={entry.path}
            >
              <div className="directory-batch-fields">
                <label>
                  资料目录
                  <input
                    aria-label="资料目录"
                    value={entry.path}
                    onChange={(event) =>
                      setEntries((previous) =>
                        previous.map((item) =>
                          item.id === entry.id
                            ? {
                                ...item,
                                path: event.target.value,
                                error: undefined,
                              }
                            : item,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  名称
                  <input
                    aria-label="名称"
                    required
                    maxLength={100}
                    value={entry.name}
                    onChange={(event) =>
                      setEntries((previous) =>
                        previous.map((item) =>
                          item.id === entry.id
                            ? { ...item, name: event.target.value }
                            : item,
                        ),
                      )
                    }
                  />
                </label>
              </div>
              <button
                className="icon-button"
                type="button"
                aria-label={`移除待添加目录 ${entry.name}`}
                disabled={busy}
                onClick={() =>
                  setEntries((previous) =>
                    previous.filter((item) => item.id !== entry.id),
                  )
                }
              >
                <XIcon size={17} />
              </button>
              {entry.error && (
                <p className="form-error" role="alert">
                  {entry.error}
                </p>
              )}
              {entry.advice && <p className="muted">{entry.advice}</p>}
              {entry.warning && (
                <p className="notice">
                  {entry.warning}：默认将跳过，可在自定义规则中调整。
                </p>
              )}
            </section>
          ))}
          {!entries.length && (
            <p className="directory-placeholder">
              选择包含文档、笔记或源代码的资料目录。
            </p>
          )}
        </div>
        <label>
          资料用途
          <Select
            aria-label="资料用途"
            value={policy}
            onChange={(event) =>
              setPolicy(event.target.value as Root["policy"])
            }
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
      </fieldset>
    </Modal>
  );
}
