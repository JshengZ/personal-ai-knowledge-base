import { useEffect, useRef, useState, type FormEvent } from "react";
import {
  ArrowUpRightIcon,
  DownloadSimpleIcon,
  UploadSimpleIcon,
  FolderOpenIcon,
} from "@phosphor-icons/react";
import { useKnowledge } from "../context";
import { request, downloadBackup, restoreBackup } from "../api";
import type { Provider, Settings } from "../types";
import { Busy, Modal, date } from "../components/ui";

export function SettingsPage() {
  const { data, refresh, notify } = useKnowledge();
  const [settings, setSettings] = useState<Settings>(() =>
    structuredClone(data.settings),
  );
  const [keys, setKeys] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [tests, setTests] = useState<Record<string, string>>({});
  const [autostart, setAutostart] = useState(false);
  const [restoring, setRestoring] = useState<File | "desktop" | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (window.knowledge)
      void window.knowledge
        .autostart()
        .then(setAutostart)
        .catch(() => {});
  }, []);
  const save = async () => {
    const result = await request<Settings>("PATCH", "/settings", {
      ...settings,
      keys,
    });
    setSettings(result);
    setKeys({});
    await refresh();
    return result;
  };
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy("save");
    setError("");
    try {
      await save();
      notify("设置已保存");
    } catch (error) {
      setError(error instanceof Error ? error.message : "保存失败");
    } finally {
      setBusy("");
    }
  };
  const test = async (kind: string) => {
    setBusy(kind);
    setError("");
    try {
      await save();
      const result = await request<{
        elapsed_ms: number;
        details: { dimensions?: number };
      }>("POST", "/models/test/" + kind, {});
      setTests((previous) => ({
        ...previous,
        [kind]:
          "连接通过 · " +
          result.elapsed_ms +
          " ms" +
          (result.details.dimensions
            ? " · " + result.details.dimensions + " 维"
            : ""),
      }));
      notify("模型连接测试通过");
    } catch (error) {
      setTests((previous) => ({
        ...previous,
        [kind]: error instanceof Error ? error.message : "测试失败",
      }));
    } finally {
      setBusy("");
    }
  };
  const providerChange = (
    kind: "chat" | "embedding" | "rerank",
    value: Provider,
  ) => setSettings((previous) => ({ ...previous, [kind]: value }));
  const keyChange = (kind: string, value: string) =>
    setKeys((previous) => {
      const next = { ...previous };
      if (value) next[kind] = value;
      else delete next[kind];
      return next;
    });
  return (
    <>
      <div className="breadcrumb">
        知识库 <span>/</span> <span>设置</span>
      </div>
      <header className="page-heading">
        <h1>设置</h1>
      </header>
      <form className="settings-form" onSubmit={submit}>
        <section className="settings-section">
          <div className="settings-label">
            <h2>公司模型</h2>
            <p>用于问答、项目摘要与记忆建议。</p>
          </div>
          <div className="settings-fields">
            <ProviderFields
              kind="chat"
              provider={settings.chat}
              onChange={(provider) => providerChange("chat", provider)}
              apiKey={keys.chat || ""}
              onKeyChange={(value) => keyChange("chat", value)}
              onClearKey={() =>
                setKeys((previous) => ({ ...previous, chat: "" }))
              }
              clearingKey={keys.chat === ""}
              showProtocol
            />
            <div className="actions">
              <button
                type="button"
                onClick={() => void test("chat")}
                disabled={!!busy}
              >
                {busy === "chat" ? <Busy /> : "保存并测试对话模型"}
              </button>
              {tests.chat && (
                <p role="status" className="test-result">
                  {tests.chat}
                </p>
              )}
            </div>
          </div>
        </section>
        <section className="settings-section">
          <div className="settings-label">
            <h2>语义检索</h2>
            <p>将问题和资料转换成向量，查找意思相近的内容。</p>
          </div>
          <div className="settings-fields">
            <label>
              运行方式
              <select
                value={settings.embedding_mode}
                onChange={(event) =>
                  setSettings((previous) => ({
                    ...previous,
                    embedding_mode: event.target
                      .value as Settings["embedding_mode"],
                  }))
                }
              >
                <option value="auto">优先公司接口，使用随包模型作为后备</option>
                <option value="local">使用随包 Qwen3 检索模型</option>
                <option value="disabled">仅使用全文检索</option>
              </select>
            </label>
            {settings.embedding_mode === "auto" && (
              <ProviderFields
                kind="embedding"
                provider={settings.embedding}
                onChange={(provider) => providerChange("embedding", provider)}
                apiKey={keys.embedding || ""}
                onKeyChange={(value) => keyChange("embedding", value)}
                onClearKey={() =>
                  setKeys((previous) => ({ ...previous, embedding: "" }))
                }
                clearingKey={keys.embedding === ""}
                optional
              />
            )}
            <p className="muted">
              {data.embedding_available
                ? "当前检索模型 " + data.embedding_model
                : "本地检索模型资源尚未就绪，可以填写公司 Embedding 接口。"}
            </p>
            {data.semantic_error && (
              <p className="notice error">{data.semantic_error}</p>
            )}
            <div className="actions">
              <button
                type="button"
                onClick={() => void test("embedding")}
                disabled={!!busy || settings.embedding_mode === "disabled"}
              >
                {busy === "embedding" ? <Busy /> : "保存并测试向量模型"}
              </button>
              {tests.embedding && (
                <p role="status" className="test-result">
                  {tests.embedding}
                </p>
              )}
            </div>
          </div>
        </section>
        <section className="settings-section">
          <div className="settings-label">
            <h2>检索精排</h2>
            <p>使用公司 Rerank 服务重新排序候选资料。</p>
          </div>
          <div className="settings-fields">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={settings.rerank_enabled}
                onChange={(event) =>
                  setSettings((previous) => ({
                    ...previous,
                    rerank_enabled: event.target.checked,
                  }))
                }
              />
              启用 Rerank 服务
            </label>
            {settings.rerank_enabled && (
              <>
                <ProviderFields
                  kind="rerank"
                  provider={settings.rerank}
                  onChange={(provider) => providerChange("rerank", provider)}
                  apiKey={keys.rerank || ""}
                  onKeyChange={(value) => keyChange("rerank", value)}
                  onClearKey={() =>
                    setKeys((previous) => ({ ...previous, rerank: "" }))
                  }
                  clearingKey={keys.rerank === ""}
                />
                <div className="actions">
                  <button
                    type="button"
                    disabled={!!busy}
                    onClick={() => void test("rerank")}
                  >
                    {busy === "rerank" ? <Busy /> : "保存并测试精排模型"}
                  </button>
                  {tests.rerank && (
                    <p role="status" className="test-result">
                      {tests.rerank}
                    </p>
                  )}
                </div>
              </>
            )}
          </div>
        </section>
        <section className="settings-section">
          <div className="settings-label">
            <h2>后台与资料处理</h2>
          </div>
          <div className="settings-fields">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={settings.auto_summarize}
                onChange={(event) =>
                  setSettings((previous) => ({
                    ...previous,
                    auto_summarize: event.target.checked,
                  }))
                }
              />
              使用公司模型自动更新项目摘要和记忆建议
            </label>
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={settings.ocr_enabled}
                onChange={(event) =>
                  setSettings((previous) => ({
                    ...previous,
                    ocr_enabled: event.target.checked,
                  }))
                }
              />
              在本机识别扫描件与图片文字
            </label>
            <label>
              单文件大小上限
              <select
                value={settings.max_file_mb}
                onChange={(event) =>
                  setSettings((previous) => ({
                    ...previous,
                    max_file_mb: Number(event.target.value),
                  }))
                }
              >
                {[10, 25, 50, 100].map((value) => (
                  <option key={value} value={value}>
                    {value} MB
                  </option>
                ))}
              </select>
            </label>
            <label>
              目录补扫间隔
              <select
                value={settings.reconcile_seconds}
                onChange={(event) =>
                  setSettings((previous) => ({
                    ...previous,
                    reconcile_seconds: Number(event.target.value),
                  }))
                }
              >
                <option value={60}>1 分钟</option>
                <option value={300}>5 分钟</option>
                <option value={900}>15 分钟</option>
                <option value={3600}>1 小时</option>
              </select>
            </label>
            <label className="checkbox-label">
              <input
                type="checkbox"
                disabled={!window.knowledge}
                checked={autostart}
                onChange={(event) => {
                  const enabled = event.target.checked;
                  void window.knowledge
                    ?.autostart(enabled)
                    .then(setAutostart)
                    .catch((error) => notify(error.message, true));
                }}
              />
              登录电脑后在后台启动
            </label>
            {!window.knowledge && (
              <p className="muted">后台启动可在桌面应用中设置。</p>
            )}
          </div>
        </section>
        {error && (
          <p className="notice error" role="alert">
            {error}
          </p>
        )}
        <div className="settings-save">
          <button className="primary" type="submit" disabled={!!busy}>
            {busy === "save" ? <Busy /> : "保存设置"}
          </button>
        </div>
      </form>
      <section className="settings-section">
        <div className="settings-label">
          <h2>备份与恢复</h2>
          <p>备份包含知识索引、项目和记忆。恢复后重新关联目录及凭据。</p>
        </div>
        <div className="settings-fields">
          <div className="actions">
            <button
              disabled={!!busy}
              onClick={() => {
                setBusy("backup");
                void downloadBackup()
                  .then((path) => {
                    if (path) notify("备份已保存");
                  })
                  .catch((error) => notify(error.message, true))
                  .finally(() => setBusy(""));
              }}
            >
              <DownloadSimpleIcon size={17} />
              备份知识库
            </button>
            <button
              disabled={!!busy}
              onClick={() =>
                window.knowledge
                  ? setRestoring("desktop")
                  : fileInput.current?.click()
              }
            >
              <UploadSimpleIcon size={17} />
              恢复备份
            </button>
            <input
              ref={fileInput}
              className="sr-only"
              type="file"
              accept=".pkb,.zip"
              onChange={(event) => {
                if (event.target.files?.[0])
                  setRestoring(event.target.files[0]);
                event.target.value = "";
              }}
            />
          </div>
          <p className="path-text">{data.data_directory}</p>
          {window.knowledge && (
            <button
              className="text-action"
              onClick={() =>
                void window
                  .knowledge!.revealData()
                  .catch((error) => notify(error.message, true))
              }
            >
              <FolderOpenIcon size={16} />
              打开数据目录
            </button>
          )}
        </div>
      </section>
      <section className="settings-section">
        <div className="settings-label">
          <h2>处理记录</h2>
        </div>
        <div className="settings-fields">
          <p className="muted">
            {data.stats.documents} 份资料 · {data.stats.chunks} 个片段 ·{" "}
            {data.stats.vectors} 个向量 · {data.stats.pending} 个待处理任务
          </p>
          {data.events.length ? (
            <details className="activity-list">
              <summary>查看最近的索引、模型与 Agent 调用记录</summary>
              {data.events.slice(0, 30).map((event) => (
                <div key={event.id}>
                  <time>{date(event.created_at, true)}</time>
                  <p>{event.message}</p>
                  {typeof event.details.error === "string" && (
                    <small>{event.details.error}</small>
                  )}
                </div>
              ))}
            </details>
          ) : (
            <p className="muted">处理和调用记录会保存在本机。</p>
          )}
        </div>
      </section>
      {restoring && (
        <Modal
          title="恢复知识库备份"
          onClose={() => setRestoring(null)}
          busy={busy === "restore"}
          submitLabel="继续恢复"
          onSubmit={(event) => {
            event.preventDefault();
            setBusy("restore");
            void restoreBackup(restoring === "desktop" ? undefined : restoring)
              .then(async (result: any) => {
                await refresh();
                setRestoring(null);
                notify(result?.message || "备份已还原");
              })
              .catch((error) => notify(error.message, true))
              .finally(() => setBusy(""));
          }}
        >
          <p>
            恢复将替换当前索引、项目和记忆。恢复前，应用会在本机保存当前知识库的恢复点。
          </p>
          <p className="muted">
            恢复完成后，先关联资料目录，再重新启用同步和 Agent 连接。
          </p>
        </Modal>
      )}
    </>
  );
}

export function ProviderFields({
  kind,
  provider,
  onChange,
  apiKey,
  onKeyChange,
  onClearKey,
  clearingKey = false,
  showProtocol = false,
  optional = false,
}: {
  kind: string;
  provider: Provider;
  onChange: (value: Provider) => void;
  apiKey: string;
  onKeyChange: (value: string) => void;
  onClearKey?: () => void;
  clearingKey?: boolean;
  showProtocol?: boolean;
  optional?: boolean;
}) {
  return (
    <>
      {showProtocol && (
        <label>
          接口协议
          <select
            value={provider.protocol}
            onChange={(event) =>
              onChange({ ...provider, protocol: event.target.value })
            }
          >
            <option value="openai">OpenAI 兼容接口</option>
            <option value="anthropic">Anthropic 兼容接口</option>
          </select>
        </label>
      )}
      <label>
        API 地址{optional ? "（可选）" : ""}
        <input
          type="url"
          autoComplete="off"
          spellCheck={false}
          placeholder="https://公司模型地址/v1"
          value={provider.base_url}
          onChange={(event) =>
            onChange({ ...provider, base_url: event.target.value })
          }
        />
      </label>
      <label>
        模型名称
        <input
          autoComplete="off"
          spellCheck={false}
          placeholder={
            kind === "embedding"
              ? "例如 Qwen3-Embedding-0.6B"
              : "填写公司提供的模型名称"
          }
          value={provider.model}
          onChange={(event) =>
            onChange({ ...provider, model: event.target.value })
          }
        />
      </label>
      <label>
        API Key
        <input
          type="password"
          autoComplete="new-password"
          value={apiKey}
          onChange={(event) => onKeyChange(event.target.value)}
          placeholder={
            provider.has_key
              ? "已保存，留空继续使用当前凭据"
              : "填写公司提供的凭据"
          }
        />
        <small>
          {clearingKey
            ? "保存后清除当前凭据。"
            : provider.has_key
              ? "当前凭据已保存在系统凭据库。"
              : "填写后保存在当前用户的系统凭据库。"}
        </small>
      </label>
      {provider.has_key && onClearKey && (
        <button className="text-action" type="button" onClick={onClearKey}>
          清除已保存的凭据
        </button>
      )}
      <details className="advanced">
        <summary>自定义接口路径</summary>
        <label>
          路径
          <input
            value={provider.endpoint}
            onChange={(event) =>
              onChange({ ...provider, endpoint: event.target.value })
            }
            placeholder={
              kind === "chat"
                ? "/v1/chat/completions"
                : kind === "embedding"
                  ? "/v1/embeddings"
                  : "/v1/rerank"
            }
          />
          <small>留空时根据 API 地址和协议自动拼接。</small>
        </label>
      </details>
    </>
  );
}
