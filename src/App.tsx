import { useCallback, useEffect, useRef, useState } from "react";
import {
  BookOpenIcon,
  ArrowRightIcon,
  ArrowsClockwiseIcon,
  WarningCircleIcon,
  XIcon,
  CopyIcon,
  FolderOpenIcon,
} from "@phosphor-icons/react";
import { request, copyText } from "./api";
import { Context, useKnowledge, type View } from "./context";
import { Sidebar } from "./components/Sidebar";
import { useAppearance } from "./appearance";
import type { Snapshot, SourceResponse } from "./types";
import { Busy, Modal, relativeDate } from "./components/ui";
import { Projects } from "./pages/Projects";
import { Search } from "./pages/Search";
import { Sources, RootDialog } from "./pages/Sources";
import { Profile } from "./pages/Profile";
import { Agents } from "./pages/Agents";
import { SettingsPage } from "./pages/Settings";
import { Welcome } from "./pages/Welcome";

export function App() {
  const {
    appearance,
    hydrate,
    status: appearanceStatus,
    error: appearanceError,
    retry: retryAppearance,
  } = useAppearance();
  const [data, setData] = useState<Snapshot | null>(null);
  const [connectionError, setConnectionError] = useState("");
  const [view, setView] = useState<View>("sources");
  const [projectId, setProjectId] = useState("");
  const [source, setSource] = useState<{ id: string; chunkId?: string } | null>(
    null,
  );
  const [rootDialog, setRootDialog] = useState(false);
  const [toast, setToast] = useState<{ text: string; error: boolean } | null>(
    null,
  );
  const notificationTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const refreshing = useRef(false);
  const refresh = useCallback(async () => {
    if (refreshing.current) return;
    refreshing.current = true;
    try {
      const value = await request<Snapshot>("GET", "/state");
      setData(value);
      setConnectionError("");
      setProjectId((current) =>
        value.projects.some((project) => project.id === current)
          ? current
          : value.projects[0]?.id || "",
      );
    } catch (error) {
      setConnectionError(
        error instanceof Error ? error.message : "本地服务连接失败",
      );
    } finally {
      refreshing.current = false;
    }
  }, []);
  useEffect(() => {
    void refresh();
    const timer = setInterval(() => {
      if (!document.hidden) void refresh();
    }, 2500);
    const visible = () => {
      if (!document.hidden) void refresh();
    };
    document.addEventListener("visibilitychange", visible);
    return () => {
      clearInterval(timer);
      document.removeEventListener("visibilitychange", visible);
    };
  }, [refresh]);
  const notify = useCallback((text: string, error = false) => {
    if (notificationTimer.current) clearTimeout(notificationTimer.current);
    setToast({ text, error });
    notificationTimer.current = setTimeout(
      () => setToast(null),
      error ? 9000 : 4500,
    );
  }, []);
  const navigate = useCallback((next: View) => {
    setView(next);
  }, []);
  const selectProject = useCallback((id: string) => {
    setProjectId(id);
    setView("projects");
  }, []);
  const openSource = useCallback(
    (id: string, chunkId?: string) => setSource({ id, chunkId }),
    [],
  );
  const addRoot = useCallback(() => setRootDialog(true), []);
  const closeRoot = useCallback(() => setRootDialog(false), []);
  const closeSource = useCallback(() => setSource(null), []);
  useEffect(() => {
    window.scrollTo(0, 0);
  }, [view]);

  useEffect(() => {
    if (data?.appearance) hydrate(data.appearance);
  }, [data?.appearance, hydrate]);

  if (!data)
    return (
      <div className="startup">
        <BookOpenIcon size={42} weight="regular" />
        <h1>个人知识库</h1>
        {connectionError ? (
          <>
            <p role="alert">{connectionError}</p>
            <button onClick={() => void refresh()}>重新连接</button>
          </>
        ) : (
          <Busy>正在连接本地知识服务</Busy>
        )}
      </div>
    );
  const context = {
    data,
    refresh,
    notify,
    view,
    navigate,
    projectId,
    selectProject,
    openSource,
    addRoot,
  };
  const onboarding = !data.settings.onboarding_complete;
  const working =
    data.scanning || data.stats.pending > 0 || data.embedding_busy;

  return (
    <Context.Provider value={context}>
      {onboarding ? (
        <Welcome />
      ) : (
        <div
          className="application"
          data-sidebar={appearance.sidebar_collapsed ? "collapsed" : "expanded"}
        >
          <Sidebar />
          <main className="workspace">
            {appearanceStatus === "error" && (
              <div className="notice error" role="alert">
                <span>外观尚未保存：{appearanceError}</span>
                <button onClick={retryAppearance}>重试保存</button>
              </div>
            )}
            {view === "projects" && data.projects.length > 0 && (
              <div className="workspace-project-picker">
                <label htmlFor="project-picker">当前项目</label>
                <select
                  id="project-picker"
                  value={projectId}
                  onChange={(event) => setProjectId(event.target.value)}
                >
                  {data.projects.map((project) => (
                    <option key={project.id} value={project.id}>
                      {project.name}
                    </option>
                  ))}
                </select>
              </div>
            )}
            {connectionError && (
              <div className="notice error" role="alert">
                <WarningCircleIcon size={18} />
                <span>{connectionError}</span>
                <button onClick={() => void refresh()}>重试</button>
              </div>
            )}
            {data.storage?.blocked && (
              <div className="notice error" role="alert">
                <WarningCircleIcon size={18} />
                <span>{data.storage.reason}</span>
                <button onClick={() => navigate("settings")}>管理存储</button>
              </div>
            )}
            {view === "projects" && <Projects />}
            {view === "search" && <Search />}
            {view === "sources" && <Sources />}
            {view === "profile" && <Profile />}
            {view === "agents" && <Agents />}
            {view === "settings" && <SettingsPage />}
            <footer className="workspace-footer">
              <button
                className="sync-status"
                onClick={() => navigate("sources")}
                aria-label="查看资料同步状态"
              >
                {working ? (
                  <ArrowsClockwiseIcon className="spin" size={15} />
                ) : (
                  <span
                    className={
                      "status-dot" +
                      (data.settings.paused || data.storage?.blocked
                        ? " paused"
                        : "")
                    }
                  />
                )}{" "}
                {data.storage?.blocked
                  ? "存储保护：索引已暂停"
                  : data.settings.paused
                    ? "自动同步已暂停"
                    : data.semantic_progress?.protected
                      ? "全文可用 · 本地向量计算已暂缓"
                      : working
                        ? data.embedding_busy &&
                          !data.scanning &&
                          data.stats.pending === 0
                          ? "向量化 " +
                            (
                              data.semantic_progress?.completed_vectors || 0
                            ).toLocaleString("zh-CN") +
                            " / " +
                            (
                              data.semantic_progress?.total_chunks || 0
                            ).toLocaleString("zh-CN")
                          : "正在同步资料"
                        : relativeDate(data.last_indexed || data.last_scan)}
              </button>
            </footer>
          </main>
        </div>
      )}
      {rootDialog && <RootDialog onClose={closeRoot} />}
      {source && (
        <SourcePanel
          id={source.id}
          chunkId={source.chunkId}
          onClose={closeSource}
        />
      )}
      {toast && (
        <div
          className={"toast" + (toast.error ? " toast-error" : "")}
          role={toast.error ? "alert" : "status"}
        >
          <span>{toast.text}</span>
          <button onClick={() => setToast(null)} aria-label="关闭提示">
            <XIcon size={16} />
          </button>
        </div>
      )}
    </Context.Provider>
  );
}

function SourcePanel({
  id,
  chunkId,
  onClose,
}: {
  id: string;
  chunkId?: string;
  onClose: () => void;
}) {
  const [source, setSource] = useState<SourceResponse | null>(null);
  const [error, setError] = useState("");
  const [focused, setFocused] = useState(chunkId);
  const [loading, setLoading] = useState(false);
  const { notify } = useKnowledge();
  useEffect(() => {
    setFocused(chunkId);
  }, [chunkId, id]);
  useEffect(() => {
    let current = true;
    setSource(null);
    setError("");
    setLoading(true);
    request<SourceResponse>(
      "GET",
      "/sources/" +
        encodeURIComponent(id) +
        (focused ? "?chunk_id=" + encodeURIComponent(focused) : ""),
    )
      .then((value) => {
        if (current) setSource(value);
      })
      .catch((error) => {
        if (current) setError(error.message);
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [id, focused]);
  const more = async () => {
    if (!source) return;
    setLoading(true);
    try {
      const next = await request<SourceResponse>(
        "GET",
        "/sources/" + id + "?offset=" + source.chunks.length,
      );
      setSource({ ...next, chunks: [...source.chunks, ...next.chunks] });
    } catch (error) {
      notify(error instanceof Error ? error.message : "原文加载失败", true);
    } finally {
      setLoading(false);
    }
  };
  return (
    <Modal title="原文" onClose={onClose} wide>
      {error ? (
        <div className="notice error" role="alert">
          {error}
        </div>
      ) : source ? (
        <div className="source-reader">
          <h3>{source.document.title}</h3>
          <details className="source-metadata">
            <summary>资料信息</summary>
            <p>{source.document.path}</p>
            <p>版本 {source.document.version.slice(0, 12)}</p>
            <p>{source.document.ocr ? "含 OCR 识别内容" : "原文提取"}</p>
          </details>
          <div className="actions">
            <button
              className="text-action"
              onClick={() =>
                void copyText(
                  source.document.title +
                    " · " +
                    (source.chunks[0]?.location.label || "") +
                    " · 版本 " +
                    source.document.version.slice(0, 12),
                )
                  .then(() => notify("引用已复制"))
                  .catch(() => notify("复制失败", true))
              }
            >
              <CopyIcon size={16} />
              复制引用
            </button>
            {window.knowledge && (
              <button
                className="text-action"
                onClick={() =>
                  void window
                    .knowledge!.openSource(id)
                    .catch((error) => notify(error.message, true))
                }
              >
                <FolderOpenIcon size={16} />
                定位原文件
              </button>
            )}
            {focused && (
              <button
                className="text-action"
                onClick={() => setFocused(undefined)}
              >
                查看完整资料
                <ArrowRightIcon size={16} />
              </button>
            )}
          </div>
          <div className="source-blocks">
            {source.chunks.map((chunk) => (
              <section key={chunk.id}>
                <p className="source-location">{chunk.location.label}</p>
                <pre>{chunk.text}</pre>
              </section>
            ))}
          </div>
          {!focused && source.chunks.length < source.total && (
            <button onClick={() => void more()} disabled={loading}>
              {loading ? <Busy /> : "继续读取原文"}
            </button>
          )}
        </div>
      ) : (
        <Busy>正在读取原文</Busy>
      )}
    </Modal>
  );
}
