import { useState } from "react";
import {
  BookOpenIcon,
  ArrowRightIcon,
  FolderOpenIcon,
  CheckIcon,
} from "@phosphor-icons/react";
import { useKnowledge } from "../context";
import { request } from "../api";
import type { Settings, Root, ConnectionConfig } from "../types";
import { Busy } from "../components/ui";
import { ProviderFields } from "./Settings";

export function Welcome() {
  const { data, refresh, notify, navigate } = useKnowledge();
  const [step, setStep] = useState(data.roots.length ? 1 : 0);
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [chat, setChat] = useState(data.settings.chat);
  const [apiKey, setApiKey] = useState("");
  const [autoSummarize, setAutoSummarize] = useState(true);
  const [connected, setConnected] = useState(false);
  const choose = async () => {
    try {
      const directory = await window.knowledge?.chooseDirectory();
      if (directory) setPath(directory);
    } catch (error) {
      setError(error instanceof Error ? error.message : "目录选择失败");
    }
  };
  const next = async () => {
    setBusy(true);
    setError("");
    try {
      if (step === 0) {
        await request<Root>("POST", "/roots", { path, policy: "agents" });
        setStep(1);
      } else if (step === 1) {
        await request<Settings>("PATCH", "/settings", {
          chat,
          auto_summarize: autoSummarize,
          ...(apiKey ? { keys: { chat: apiKey } } : {}),
        });
        await request("POST", "/models/test/chat", {});
        setStep(2);
      } else await finish();
    } catch (error) {
      setError(error instanceof Error ? error.message : "操作失败");
    } finally {
      setBusy(false);
    }
  };
  const finish = async () => {
    await request("PATCH", "/settings", { onboarding_complete: true });
    await refresh();
    navigate("projects");
  };
  const connect = async () => {
    setBusy(true);
    setError("");
    try {
      const state = await request<import("../types").Snapshot>("GET", "/state");
      const config = await request<ConnectionConfig>("POST", "/connections", {
        name: "ZCode",
        kind: "zcode",
        project_ids: state.projects.map((project) => project.id),
        include_profile: false,
        can_propose: false,
      });
      await request(
        "POST",
        "/connections/" + config.connection.id + "/install-zcode",
        {},
      );
      setConnected(true);
      notify("已添加到 ZCode，新建会话即可开始查询。");
    } catch (error) {
      setError(error instanceof Error ? error.message : "连接配置失败");
    } finally {
      setBusy(false);
    }
  };
  return (
    <main className="welcome">
      <div className="welcome-brand">
        <BookOpenIcon size={25} weight="regular" />
        个人知识库
      </div>
      <div className="welcome-content">
        <nav className="setup-progress" aria-label="设置进度">
          {["资料目录", "公司模型", "Agent 连接"].map((label, index) => (
            <span key={label} className={step === index ? "current" : ""}>
              {index < step ? <CheckIcon size={14} /> : null}
              {label}
            </span>
          ))}
        </nav>
        {step === 0 ? (
          <>
            <h1>从你的资料开始。</h1>
            <p className="welcome-description">
              把项目、文档和笔记放进自己的知识库。日常在 ZCode 等 Agent
              中使用，新增资料会在后台自动整理。
            </p>
            <label className="welcome-directory">
              <span className="muted storage-location-summary">
                知识库存储位置：{data.data_directory.replace(/^\\\\\?\\/, "")}
                。以后可在设置中迁移；先添加具体的项目或文档目录。
              </span>
              选择一个资料目录
              <div className="input-action">
                <input
                  value={path}
                  onChange={(event) => setPath(event.target.value)}
                  placeholder={
                    window.knowledge
                      ? "选择你的项目或工作文档文件夹"
                      : "填写此电脑上的完整文件夹路径"
                  }
                />
                {window.knowledge && (
                  <button onClick={() => void choose()}>
                    <FolderOpenIcon size={18} />
                    选择目录
                  </button>
                )}
              </div>
            </label>
            <p className="muted">
              此目录的资料将允许公司模型和你连接的 Agent
              使用。你可以随时在资料页调整范围。
            </p>
            <div className="welcome-actions">
              <button
                className="primary"
                disabled={busy || !path.trim()}
                onClick={() => void next()}
              >
                {busy ? (
                  <Busy />
                ) : (
                  <>
                    加入知识库
                    <ArrowRightIcon size={17} />
                  </>
                )}
              </button>
            </div>
          </>
        ) : step === 1 ? (
          <>
            <h1>连接公司模型。</h1>
            <p className="welcome-description">
              用公司提供的 API
              地址、模型名称和凭据，整理项目与生成带出处的回答。
            </p>
            <div className="welcome-model">
              <ProviderFields
                kind="chat"
                provider={chat}
                onChange={setChat}
                apiKey={apiKey}
                onKeyChange={setApiKey}
                showProtocol
              />
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={autoSummarize}
                  onChange={(event) => setAutoSummarize(event.target.checked)}
                />
                自动整理项目摘要与待确认记忆
              </label>
            </div>
            <div className="welcome-actions">
              <button
                className="primary"
                disabled={busy || !chat.base_url || !chat.model}
                onClick={() => void next()}
              >
                {busy ? (
                  <Busy />
                ) : (
                  <>
                    测试并继续
                    <ArrowRightIcon size={17} />
                  </>
                )}
              </button>
              <button
                className="text-action"
                disabled={busy}
                onClick={() => setStep(2)}
              >
                稍后配置
              </button>
            </div>
          </>
        ) : (
          <>
            <h1>在 Agent 中使用。</h1>
            <p className="welcome-description">
              为 ZCode 添加本机 MCP
              连接，就能在日常会话中查询你的项目和历史资料。
            </p>
            <div className="welcome-agent">
              <h2>ZCode</h2>
              <p>共享刚刚加入的项目。个人记忆和写回权限可在连接页单独开启。</p>
              <button
                disabled={busy || connected}
                onClick={() => void connect()}
              >
                {busy ? (
                  <Busy />
                ) : connected ? (
                  <>
                    <CheckIcon size={17} />
                    已配置 ZCode
                  </>
                ) : (
                  "一键配置 ZCode"
                )}
              </button>
            </div>
            <div className="welcome-actions">
              <button
                className="primary"
                disabled={busy}
                onClick={() => void next()}
              >
                进入知识库
                <ArrowRightIcon size={17} />
              </button>
              <button
                className="text-action"
                disabled={busy}
                onClick={() => void finish().then(() => navigate("agents"))}
              >
                连接其他 Agent
              </button>
            </div>
          </>
        )}
        {error && (
          <p className="notice error" role="alert">
            {error}
          </p>
        )}
      </div>
      <footer className="welcome-footer">
        本机保存 · 持续更新 · 通过 MCP 使用
      </footer>
    </main>
  );
}
