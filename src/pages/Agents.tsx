import { useState, type FormEvent } from "react";
import {
  PlusIcon,
  PlugIcon,
  CopyIcon,
  ArrowUpRightIcon,
  PencilSimpleIcon,
  CheckIcon,
} from "@phosphor-icons/react";
import { useKnowledge } from "../context";
import { request, copyText } from "../api";
import type { AgentConnection, ConnectionConfig } from "../types";
import { Select, Busy, Empty, Modal, date } from "../components/ui";

export function Agents() {
  const { data, notify, refresh } = useKnowledge();
  const [editing, setEditing] = useState<AgentConnection | "new" | null>(null);
  const [config, setConfig] = useState<ConnectionConfig | null>(null);
  const [busyId, setBusyId] = useState("");
  const [testResults, setTestResults] = useState<Record<string, string>>({});
  const action = async (id: string, callback: () => Promise<void>) => {
    setBusyId(id);
    try {
      await callback();
      await refresh();
    } catch (error) {
      notify(error instanceof Error ? error.message : "连接操作失败", true);
    } finally {
      setBusyId("");
    }
  };
  return (
    <>
      <div className="breadcrumb">
        知识库 <span>/</span> <span>Agent 连接</span>
      </div>
      <header className="page-heading">
        <div>
          <h1>Agent 连接</h1>
          <p>管理 MCP 连接与资料访问权限。</p>
        </div>
        <button
          className="primary"
          onClick={() => setEditing("new")}
          data-guide="add-connection"
        >
          <PlusIcon size={17} />
          添加连接
        </button>
      </header>
      {data.connections.length ? (
        <div className="connection-list">
          {data.connections.map((connection) => (
            <section className="connection-row" key={connection.id}>
              <div className="connection-heading">
                <PlugIcon size={31} weight="regular" />
                <div>
                  <h2>{connection.name}</h2>
                  <p className="muted">
                    {connection.last_used
                      ? "最近调用 " + date(connection.last_used, true)
                      : "等待客户端首次调用"}
                  </p>
                </div>
                <label className="switch-label">
                  <input
                    type="checkbox"
                    checked={connection.enabled}
                    onChange={(event) =>
                      void action(connection.id, async () => {
                        await request(
                          "PATCH",
                          "/connections/" + connection.id,
                          { enabled: event.target.checked },
                        );
                      })
                    }
                  />
                  {connection.enabled ? "已启用" : "已停用"}
                </label>
              </div>
              <div className="connection-scope">
                <p>共享项目</p>
                <span>
                  {connection.project_ids
                    .map(
                      (id) =>
                        data.projects.find((project) => project.id === id)
                          ?.name,
                    )
                    .filter(Boolean)
                    .join("、") || "尚未选择项目"}
                </span>
                <p>个人记忆</p>
                <span>
                  {connection.include_profile ? "允许读取已确认记忆" : "未共享"}
                </span>
                <p>记忆建议</p>
                <span>
                  {connection.can_propose ? "允许提交，等待你确认" : "未启用"}
                </span>
              </div>
              <div className="actions">
                {connection.kind === "zcode" && (
                  <button
                    className="primary"
                    disabled={busyId === connection.id || !connection.enabled}
                    onClick={() =>
                      void action(connection.id, async () => {
                        const result = await request<{ message: string }>(
                          "POST",
                          "/connections/" + connection.id + "/install-zcode",
                          {},
                        );
                        notify(result.message);
                      })
                    }
                  >
                    配置 ZCode
                    <ArrowUpRightIcon size={16} />
                  </button>
                )}
                <button
                  disabled={busyId === connection.id}
                  onClick={() =>
                    void action(connection.id, async () => {
                      setConfig(
                        await request<ConnectionConfig>(
                          "GET",
                          "/connections/" + connection.id + "/config",
                        ),
                      );
                    })
                  }
                >
                  <CopyIcon size={16} />
                  接入配置
                </button>
                <button
                  className="text-action"
                  disabled={busyId === connection.id || !connection.enabled}
                  onClick={() =>
                    void action(connection.id, async () => {
                      const result = await request<{
                        message: string;
                        projects: number;
                        sources: number;
                      }>("POST", "/connections/" + connection.id + "/test", {});
                      setTestResults((previous) => ({
                        ...previous,
                        [connection.id]:
                          result.message +
                          " 当前可检索 " +
                          result.projects +
                          " 个项目。",
                      }));
                    })
                  }
                >
                  {busyId === connection.id ? <Busy /> : "测试共享范围"}
                </button>
                <button
                  className="text-action"
                  onClick={() => setEditing(connection)}
                >
                  <PencilSimpleIcon size={16} />
                  编辑权限
                </button>
              </div>
              {testResults[connection.id] && (
                <p className="connection-test" role="status">
                  <CheckIcon size={16} />
                  {testResults[connection.id]}
                </p>
              )}
            </section>
          ))}
        </div>
      ) : (
        <Empty
          title="暂无 Agent 连接"
          action={
            <button className="primary" onClick={() => setEditing("new")}>
              添加 Agent 连接
            </button>
          }
        >
          选择共享的项目，生成 MCP 配置后，你可以在 Agent 的日常会话中查询资料。
        </Empty>
      )}
      {editing && (
        <ConnectionEditor
          connection={editing === "new" ? undefined : editing}
          onClose={() => setEditing(null)}
          onCreated={(config) => {
            setEditing(null);
            setConfig(config);
          }}
        />
      )}
      {config && (
        <ConfigDialog config={config} onClose={() => setConfig(null)} />
      )}
    </>
  );
}

export function ConnectionEditor({
  connection,
  onClose,
  onCreated,
}: {
  connection?: AgentConnection;
  onClose: () => void;
  onCreated?: (config: ConnectionConfig) => void;
}) {
  const { data, refresh, notify } = useKnowledge();
  const [name, setName] = useState(connection?.name || "ZCode");
  const [kind, setKind] = useState(connection?.kind || "zcode");
  const [projects, setProjects] = useState(
    connection?.project_ids || data.projects.map((project) => project.id),
  );
  const [profile, setProfile] = useState(connection?.include_profile || false);
  const [propose, setPropose] = useState(connection?.can_propose || false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      const result = await request<ConnectionConfig>(
        connection ? "PATCH" : "POST",
        connection ? "/connections/" + connection.id : "/connections",
        {
          name,
          kind,
          project_ids: projects,
          include_profile: profile,
          can_propose: propose,
        },
      );
      await refresh();
      if (connection) {
        notify("连接权限已更新");
        onClose();
      } else if (onCreated) onCreated(result);
      else onClose();
    } catch (error) {
      setError(error instanceof Error ? error.message : "保存失败");
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      title={connection ? "编辑连接权限" : "添加 Agent 连接"}
      onClose={onClose}
      onSubmit={submit}
      submitLabel={connection ? "保存权限" : "生成连接"}
      busy={busy}
    >
      <label>
        连接名称
        <input
          value={name}
          maxLength={60}
          required
          onChange={(event) => setName(event.target.value)}
        />
      </label>
      {!connection && (
        <label>
          客户端
          <Select
            aria-label="客户端"
            value={kind}
            onChange={(event) => {
              setKind(event.target.value);
              setName(
                (
                  {
                    zcode: "ZCode",
                    codex: "Codex",
                    claude: "Claude Code",
                    other: "我的 Agent",
                  } as Record<string, string>
                )[event.target.value],
              );
            }}
          >
            <option value="zcode">ZCode</option>
            <option value="codex">Codex</option>
            <option value="claude">Claude Code</option>
            <option value="other">其他支持 MCP 的 Agent</option>
          </Select>
        </label>
      )}
      <fieldset>
        <legend>允许读取的项目</legend>
        {data.projects.length ? (
          data.projects.map((project) => (
            <label className="checkbox-label" key={project.id}>
              <input
                type="checkbox"
                checked={projects.includes(project.id)}
                onChange={(event) =>
                  setProjects((previous) =>
                    event.target.checked
                      ? [...previous, project.id]
                      : previous.filter((id) => id !== project.id),
                  )
                }
              />
              {project.name}
            </label>
          ))
        ) : (
          <p className="muted">添加资料目录后，可以回来选择项目。</p>
        )}
      </fieldset>
      <label className="checkbox-label">
        <input
          type="checkbox"
          checked={profile}
          onChange={(event) => setProfile(event.target.checked)}
        />
        共享已确认的个人记忆
      </label>
      <label className="checkbox-label">
        <input
          type="checkbox"
          checked={propose}
          onChange={(event) => setPropose(event.target.checked)}
        />
        允许 Agent 提交记忆建议，等待我确认
      </label>
      <p className="muted">
        目录设为“允许公司模型与 Agent”后，连接才能读取其中的资料。
      </p>
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}

export function ConfigDialog({
  config,
  onClose,
}: {
  config: ConnectionConfig;
  onClose: () => void;
}) {
  const { notify, refresh } = useKnowledge();
  const [format, setFormat] = useState(
    config.connection.kind === "codex" ? "toml" : "json",
  );
  const [busy, setBusy] = useState(false);
  const name = config.name.replaceAll("-", "_");
  const toml =
    "[mcp_servers." +
    name +
    "]\ncommand = " +
    JSON.stringify(config.server.command) +
    "\nargs = " +
    JSON.stringify(config.server.args) +
    "\n\n[mcp_servers." +
    name +
    ".env]\n" +
    Object.entries(config.server.env)
      .map(([key, value]) => key + " = " + JSON.stringify(value))
      .join("\n");
  const text =
    format === "toml"
      ? toml
      : format === "http"
        ? JSON.stringify(config.http, null, 2)
        : JSON.stringify(config.config, null, 2);
  const install = async () => {
    setBusy(true);
    try {
      const result = await request<{ message: string }>(
        "POST",
        "/connections/" + config.connection.id + "/install-zcode",
        {},
      );
      await refresh();
      notify(result.message);
    } catch (error) {
      notify(error instanceof Error ? error.message : "配置失败", true);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal title="接入 Agent" onClose={onClose} wide>
      <p>
        {config.connection.name} ·{" "}
        {config.connection.kind === "zcode"
          ? "可以直接添加到本机 ZCode，原配置会自动备份。"
          : "将下面的配置添加到客户端的 MCP 设置中。"}
      </p>
      {config.connection.kind === "zcode" && (
        <button
          className="primary"
          disabled={busy}
          onClick={() => void install()}
        >
          {busy ? <Busy /> : "一键配置 ZCode"}
        </button>
      )}
      <div className="tabs" aria-label="配置格式">
        {[
          ["json", "通用 JSON"],
          ["toml", "Codex TOML"],
          ["http", "本机 HTTP"],
        ].map(([value, label]) => (
          <button
            key={value}
            aria-pressed={format === value}
            className={format === value ? "selected" : ""}
            onClick={() => setFormat(value)}
          >
            {label}
          </button>
        ))}
      </div>
      <pre className="config-code">{text}</pre>
      <button
        onClick={() =>
          void copyText(text)
            .then(() => notify("接入配置已复制"))
            .catch(() => notify("复制失败", true))
        }
      >
        <CopyIcon size={16} />
        复制配置
      </button>
      <details className="advanced">
        <summary>Agent 调用说明</summary>
        <p>{config.instructions}</p>
        <button
          className="text-action"
          onClick={() =>
            void copyText(config.instructions).then(() =>
              notify("使用规则已复制"),
            )
          }
        >
          复制使用规则
        </button>
      </details>
      <p className="muted">
        配置完成后，在客户端新建会话，问一个与你的项目有关的问题，并检查原文引用。
      </p>
    </Modal>
  );
}
