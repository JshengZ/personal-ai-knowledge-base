use crate::{models::Models, state::AppState, types::*};
use anyhow::Result;
use serde::Serialize;

#[derive(Default)]
pub struct EmbeddingMetrics {
    pub model: String,
    pub processed: u64,
    pub seconds: f64,
}

#[derive(Serialize)]
pub struct SemanticProgress {
    pub mode: &'static str,
    pub model: String,
    pub total_chunks: u64,
    pub completed_vectors: u64,
    pub counts_ready: bool,
    pub vectors_per_second: Option<f64>,
    pub estimated_seconds: Option<u64>,
    pub protected: bool,
    pub notice: Option<String>,
}

pub fn snapshot(app: &AppState, settings: &Settings) -> Result<SemanticProgress> {
    let counts = app.store.counts()?;
    let total = counts.get("chunks").copied().unwrap_or(0).max(0) as u64;
    let model = app.models.embedding_model(settings);
    let done = counts
        .get(&format!("model:{model}"))
        .copied()
        .unwrap_or(0)
        .max(0) as u64;
    let ready = counts.get("initialized") == Some(&1);
    let remote = Models::remote_embeddings(settings);
    let metrics = app.embedding_metrics.lock().unwrap();
    let rate = if metrics.model == model && metrics.processed >= 32 && metrics.seconds >= 10.0 {
        Some(metrics.processed as f64 / metrics.seconds)
    } else {
        None
    };
    let estimate = rate
        .filter(|rate| *rate > 0.0)
        .map(|rate| (total.saturating_sub(done) as f64 / rate).ceil() as u64);
    let local_auto = settings.embedding_mode == "auto" && !remote;
    let scale = local_auto && total > settings.auto_local_chunk_limit;
    let slow = local_auto
        && metrics.seconds >= 20.0
        && rate.is_some_and(|rate| rate < 2.0)
        && estimate.is_some_and(|seconds| seconds > 7200);
    let protected = local_auto && (!ready || scale || slow);
    let notice = if settings.embedding_mode == "disabled" {
        None
    } else if local_auto && !ready {
        Some("正在统计旧版索引，先使用全文检索；统计期间不启动大规模本地向量计算。".into())
    } else if scale {
        Some(format!("全库已有 {total} 个片段，超过自动模式的本地预算 {}。已暂缓 CPU 全量向量化，全文检索继续可用。可配置部门 Embedding API，或明确选择本地模式继续。",settings.auto_local_chunk_limit))
    } else if settings.embedding_mode == "local" && total > settings.auto_local_chunk_limit {
        Some("你已明确选择本地模式，CPU 模型会继续处理大量片段。请关注耗时、存储和资源占用，也可随时切换到部门 Embedding API 或仅全文。".into())
    } else if slow {
        Some("实测本地向量化速度较低，预计超过两小时，自动模式已暂缓后台向量计算。请配置部门 Embedding API，或选择本地模式继续。".into())
    } else if !remote && estimate.is_some_and(|seconds| seconds > 7200) {
        Some("按当前 CPU 速度，全库向量化预计超过两小时。可使用部门 Embedding API 或仅全文检索；当前本地模式仍继续工作。".into())
    } else {
        None
    };
    Ok(SemanticProgress {
        mode: if settings.embedding_mode == "disabled" {
            "disabled"
        } else if remote {
            "api"
        } else {
            "local"
        },
        model,
        total_chunks: total,
        completed_vectors: done,
        counts_ready: ready,
        vectors_per_second: rate,
        estimated_seconds: estimate,
        protected,
        notice,
    })
}
