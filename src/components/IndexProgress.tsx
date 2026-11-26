import {
  CheckCircleIcon,
  CircleNotchIcon,
  PauseCircleIcon,
  StackIcon,
} from "@phosphor-icons/react";
import type { Snapshot } from "../types";

const number = (value: number) => value.toLocaleString("zh-CN");
function duration(seconds: number) {
  if (seconds >= 86400) return "约 " + (seconds / 86400).toFixed(1) + " 天";
  if (seconds >= 3600) return "约 " + (seconds / 3600).toFixed(1) + " 小时";
  return "约 " + Math.max(1, Math.ceil(seconds / 60)) + " 分钟";
}

export function IndexProgress({
  data,
  onSettings,
}: {
  data: Snapshot;
  onSettings: () => void;
}) {
  const semantic = data.semantic_progress;
  if (!semantic) return null;
  const processed = data.stats.ready + data.stats.errors + data.stats.empty;
  const later = Math.max(
    0,
    data.stats.discovered -
      processed -
      data.stats.pending -
      data.stats.indexing,
  );
  const vectorState = semantic.protected
    ? "已暂缓"
    : data.settings.paused
      ? "已暂停"
      : data.embedding_busy
        ? "计算中"
        : "等待或已完成可处理资料";
  const VectorIcon =
    semantic.mode === "disabled" || semantic.protected || data.settings.paused
      ? PauseCircleIcon
      : data.embedding_busy
        ? CircleNotchIcon
        : CheckCircleIcon;
  return (
    <section className="index-progress" aria-label="索引处理进度">
      <h2 className="sr-only">处理进度</h2>
      <div className="index-progress-grid">
        <div className="index-progress-step">
          {data.scanning ? (
            <CircleNotchIcon className="spin" size={22} aria-hidden />
          ) : (
            <CheckCircleIcon size={22} aria-hidden />
          )}
          <div>
            <h3>扫描目录</h3>
            <p>
              {data.scanning
                ? "扫描中"
                : data.last_scan
                  ? "已完成"
                  : "等待扫描"}{" "}
              · 已发现 {number(data.stats.discovered)} 份
            </p>
          </div>
        </div>
        <div className="index-progress-step">
          <StackIcon size={22} aria-hidden />
          <div>
            <h3>解析与分块</h3>
            <p>
              {data.stats.initialized ? number(processed) : "统计中"} /{" "}
              {number(data.stats.discovered)} 份
            </p>
            <small>
              {number(data.stats.chunks)} 个片段 ·{" "}
              {data.stats.indexing ? "解析中" : "全文可用"}
            </small>
          </div>
        </div>
        <div className="index-progress-step">
          <VectorIcon
            size={22}
            className={
              data.embedding_busy && semantic.mode !== "disabled"
                ? "spin"
                : undefined
            }
            aria-hidden
          />
          <div>
            <h3>向量化</h3>
            <p>
              {semantic.mode === "disabled" ? (
                "已关闭 · 全文可用"
              ) : (
                <>
                  {semantic.counts_ready
                    ? number(semantic.completed_vectors)
                    : "统计中"}{" "}
                  / {number(semantic.total_chunks)}
                </>
              )}
            </p>
            {semantic.mode !== "disabled" && (
              <small>
                {vectorState}
                {semantic.vectors_per_second
                  ? " · " + semantic.vectors_per_second.toFixed(2) + " 片段/秒"
                  : data.embedding_busy
                    ? " · 正在采样速度"
                    : ""}
              </small>
            )}
          </div>
        </div>
      </div>
      {semantic.estimated_seconds !== null &&
        semantic.completed_vectors < semantic.total_chunks && (
          <p className="muted">
            按近期速度处理全库剩余片段：{duration(semantic.estimated_seconds)}
            。估算范围受目录权限和排除规则影响。
          </p>
        )}
      {semantic.notice && (
        <div className="notice" role="status">
          <span>{semantic.notice}</span>
          <button onClick={onSettings}>设置向量模型</button>
        </div>
      )}
      <details className="index-progress-details">
        <summary>处理详情</summary>
        <p>
          队列 {number(data.stats.pending)} · 后续批次 {number(later)} ·
          受限或失败 {number(data.stats.errors)} · 未提取到文字{" "}
          {number(data.stats.empty)}
        </p>
        <p>
          {semantic.mode === "api"
            ? "部门 Embedding API"
            : semantic.mode === "local"
              ? "随包 Qwen · CPU"
              : "仅全文检索"}
          。向量数量对应当前模型，总片段为全库统计；只处理已启用且获授权的目录。全文检索无需等待向量完成。
        </p>
      </details>
    </section>
  );
}
