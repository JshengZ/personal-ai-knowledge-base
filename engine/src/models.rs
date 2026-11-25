use crate::types::{Provider, RuntimeConfig, Settings};
use anyhow::{bail, Context, Result};
use reqwest::{redirect::Policy, Client};
use serde_json::{json, Value};
use std::{path::Path, process::Stdio, sync::Arc, time::Duration};
use tokio::{process::Child, sync::Mutex};

pub struct Models {
    client: Client,
    credential_namespace: String,
    runtime: RuntimeConfig,
    local: Mutex<Option<LocalEmbedding>>,
}

struct LocalEmbedding {
    child: Child,
    port: u16,
}

impl Models {
    pub fn new(home: &Path, runtime: RuntimeConfig) -> Result<Arc<Self>> {
        let fingerprint = blake3::hash(home.to_string_lossy().as_bytes())
            .to_hex()
            .to_string();
        Ok(Arc::new(Self {
            client: Client::builder()
                .redirect(Policy::none())
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(120))
                .build()?,
            credential_namespace: format!("personal-knowledge-{}", &fingerprint[..16]),
            runtime,
            local: Mutex::new(None),
        }))
    }

    pub fn save_key(&self, kind: &str, value: &str) -> Result<()> {
        let entry = keyring::Entry::new(&self.credential_namespace, kind)?;
        if value.is_empty() {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(error) => Err(error).context("无法清除系统凭据"),
            }
        } else {
            entry
                .set_password(value)
                .context("无法保存到系统凭据库，请检查系统授权")
        }
    }

    fn key(&self, kind: &str, provider: &Provider) -> Result<String> {
        if !provider.has_key {
            return Ok(String::new());
        }
        keyring::Entry::new(
            &self.credential_namespace,
            provider.credential_id.as_deref().unwrap_or(kind),
        )?
        .get_password()
        .context("当前用户的模型凭据不可用，请在设置中重新填写")
    }

    pub fn validate_provider(provider: &Provider) -> Result<()> {
        if provider.base_url.is_empty() {
            return Ok(());
        }
        let url =
            url::Url::parse(&provider.base_url).context("请填写完整的 http 或 https API 地址")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            bail!("API 地址需使用 http 或 https，凭据请填写在 API Key 栏中");
        }
        if provider.model.trim().is_empty() {
            bail!("请填写模型名称");
        }
        if !provider.endpoint.is_empty()
            && (!provider.endpoint.starts_with('/')
                || provider.endpoint.starts_with("//")
                || provider.endpoint.contains("://"))
        {
            bail!("自定义接口路径应以单个 / 开头");
        }
        Ok(())
    }

    fn endpoint(provider: &Provider, suffix: &str) -> Result<String> {
        Self::validate_provider(provider)?;
        if provider.base_url.is_empty() {
            bail!("请先在设置中配置公司的模型接口");
        }
        let mut url = url::Url::parse(&provider.base_url)?;
        if !provider.endpoint.is_empty() {
            url.set_path(&provider.endpoint);
            return Ok(url.to_string());
        }
        let base = provider.base_url.trim_end_matches('/');
        if base.ends_with(suffix) {
            return Ok(base.into());
        }
        Ok(format!("{base}/{suffix}"))
    }

    async fn post(
        &self,
        kind: &str,
        provider: &Provider,
        suffix: &str,
        payload: Value,
    ) -> Result<Value> {
        let key = self.key(kind, provider)?;
        let mut request = self
            .client
            .post(Self::endpoint(provider, suffix)?)
            .json(&payload);
        if !key.is_empty() {
            request = if provider.protocol == "anthropic" {
                request
                    .header("x-api-key", &key)
                    .header("anthropic-version", "2023-06-01")
            } else {
                request.bearer_auth(&key)
            };
        }
        let response = request
            .send()
            .await
            .context("模型连接失败，请检查公司网络、API 地址和证书")?;
        let status = response.status();
        if !status.is_success() {
            bail!(
                "模型服务返回 HTTP {}，请检查接口路径、模型权限或服务额度",
                status.as_u16()
            );
        }
        let bytes = response.bytes().await?;
        if bytes.len() > 16 * 1024 * 1024 {
            bail!("模型响应过大");
        }
        serde_json::from_slice(&bytes).context("模型返回内容不是有效的 JSON")
    }

    pub async fn chat(&self, settings: &Settings, system: &str, user: &str) -> Result<String> {
        let provider = &settings.chat;
        let result = if provider.protocol == "anthropic" {
            self.post("chat",provider,"messages",json!({"model":provider.model,"max_tokens":2048,"system":system,"messages":[{"role":"user","content":user}]})).await?
        } else {
            self.post("chat",provider,"chat/completions",json!({"model":provider.model,"max_tokens":2048,"messages":[{"role":"system","content":system},{"role":"user","content":user}]})).await?
        };
        let text = if provider.protocol == "anthropic" {
            result["content"].as_array().map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        } else {
            let content = &result["choices"][0]["message"]["content"];
            content.as_str().map(str::to_string).or_else(|| {
                content.as_array().map(|parts| {
                    parts
                        .iter()
                        .filter_map(|part| part["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
            })
        }
        .filter(|text| !text.trim().is_empty())
        .context("对话模型没有返回正文，请检查模型和协议配置")?;
        Ok(text)
    }

    pub fn embedding_available(&self, settings: &Settings) -> bool {
        if settings.embedding_mode == "disabled" {
            return false;
        }
        if !settings.embedding.base_url.is_empty() && settings.embedding_mode != "local" {
            return true;
        }
        self.local_assets().is_some()
    }

    pub fn local_assets(&self) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        let platform = if cfg!(target_os = "windows") {
            "win-x64"
        } else if cfg!(target_arch = "aarch64") {
            "mac-arm64"
        } else {
            "mac-x64"
        };
        let binary_name = if cfg!(target_os = "windows") {
            "llama-server.exe"
        } else {
            "llama-server"
        };
        let root = self.runtime.resources.join("runtime");
        let binary = root.join(platform).join(binary_name);
        let model = root.join("models/Qwen3-Embedding-0.6B-Q8_0.gguf");
        if binary.is_file() && model.is_file() {
            Some((binary, model))
        } else {
            None
        }
    }

    pub fn embedding_model(&self, settings: &Settings) -> String {
        if !settings.embedding.base_url.is_empty() && settings.embedding_mode != "local" {
            format!(
                "remote:{}:{}",
                settings.embedding.base_url.trim_end_matches('/'),
                settings.embedding.model
            )
        } else {
            "local:Qwen3-Embedding-0.6B-Q8_0:v1".into()
        }
    }

    pub fn remote_embeddings(settings: &Settings) -> bool {
        !settings.embedding.base_url.is_empty() && settings.embedding_mode != "local"
    }

    async fn local_port(&self) -> Result<u16> {
        let mut local = self.local.lock().await;
        if let Some(server) = local.as_mut() {
            if server.child.try_wait()?.is_none() {
                return Ok(server.port);
            }
        }
        let (binary, model) = self.local_assets().context(
            "本地检索模型资源尚未安装，请在设置中配置公司 Embedding 接口或使用完整安装包",
        )?;
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let threads = std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(2)
            .clamp(1, 6);
        let mut command = tokio::process::Command::new(binary);
        command.arg("--model").arg(model).args([
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--embeddings",
            "--pooling",
            "last",
            "--ctx-size",
            "4096",
            "--n-gpu-layers",
            "0",
            "--threads",
            &threads.to_string(),
        ]);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command.spawn().context("无法启动随包检索模型")?;
        for _ in 0..180 {
            if child.try_wait()?.is_some() {
                bail!("本地检索模型启动失败，请检查运行资源和 CPU 支持情况");
            }
            if let Ok(response) = self
                .client
                .get(format!("http://127.0.0.1:{port}/health"))
                .timeout(Duration::from_secs(1))
                .send()
                .await
            {
                if response.status().is_success() {
                    *local = Some(LocalEmbedding { child, port });
                    return Ok(port);
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        bail!("本地检索模型启动超时")
    }

    pub async fn embed(
        &self,
        settings: &Settings,
        inputs: &[String],
        query: bool,
    ) -> Result<Vec<Vec<f32>>> {
        if inputs.is_empty() {
            return Ok(vec![]);
        }
        let remote = Self::remote_embeddings(settings);
        let is_qwen = !remote || settings.embedding.model.to_lowercase().contains("qwen");
        let inputs: Vec<String> = inputs.iter().map(|input| {
            if query && is_qwen {format!("Instruct: Retrieve relevant personal work documents for the question.\nQuery: {input}")} else {input.clone()}
        }).collect();
        let response = if remote {
            self.post(
                "embedding",
                &settings.embedding,
                "embeddings",
                json!({"model":settings.embedding.model,"input":inputs}),
            )
            .await?
        } else {
            let port = self.local_port().await?;
            let response = self
                .client
                .post(format!("http://127.0.0.1:{port}/v1/embeddings"))
                .json(&json!({"model":"Qwen3-Embedding-0.6B","input":inputs}))
                .send()
                .await?;
            if !response.status().is_success() {
                bail!("本地向量化失败，请尝试缩短输入或重新索引");
            }
            response.json().await?
        };
        let records = response["data"]
            .as_array()
            .context("Embedding 接口需要返回 data 数组")?;
        if records.len() != inputs.len() {
            bail!("Embedding 返回数量与输入不一致");
        }
        let mut ordered: Vec<Option<Vec<f32>>> = vec![None; inputs.len()];
        for (fallback, record) in records.iter().enumerate() {
            let index = record["index"].as_u64().unwrap_or(fallback as u64) as usize;
            if index >= ordered.len() || ordered[index].is_some() {
                bail!("Embedding 索引重复或越界");
            }
            let vector: Vec<f32> = serde_json::from_value(record["embedding"].clone())
                .context("Embedding 向量格式无效")?;
            if vector.is_empty()
                || vector.len() > 8192
                || vector.iter().any(|v| !v.is_finite())
                || vector.iter().all(|v| *v == 0.0)
            {
                bail!("Embedding 返回无效向量");
            }
            ordered[index] = Some(vector);
        }
        let vectors: Vec<Vec<f32>> = ordered.into_iter().map(Option::unwrap).collect();
        if vectors
            .iter()
            .any(|vector| vector.len() != vectors[0].len())
        {
            bail!("Embedding 向量维度不一致");
        }
        Ok(vectors)
    }

    pub async fn rerank(
        &self,
        settings: &Settings,
        query: &str,
        documents: &[String],
    ) -> Result<Vec<(usize, f64)>> {
        let response = self.post("rerank",&settings.rerank,"rerank",json!({"model":settings.rerank.model,"query":query,"documents":documents,"top_n":documents.len(),"return_documents":false})).await?;
        let rows = response["results"]
            .as_array()
            .context("Rerank 接口需要返回 results 数组")?;
        let mut seen = std::collections::HashSet::new();
        let mut ranks = vec![];
        for row in rows {
            let index = row["index"].as_u64().context("Rerank 缺少结果位置")? as usize;
            let score = row["relevance_score"]
                .as_f64()
                .or_else(|| row["score"].as_f64())
                .context("Rerank 缺少评分")?;
            if index >= documents.len() || !seen.insert(index) || !score.is_finite() {
                bail!("Rerank 结果位置或评分无效");
            }
            ranks.push((index, score));
        }
        Ok(ranks)
    }

    pub async fn test(&self, settings: &Settings, kind: &str) -> Result<Value> {
        let start = std::time::Instant::now();
        let details = match kind {
            "chat" => {
                json!({"response":self.chat(settings,"请只回答连接成功。","测试连接").await?})
            }
            "embedding" => {
                let result = self
                    .embed(settings, &["用于验证向量接口的示例文本。".into()], false)
                    .await?;
                json!({"dimensions":result[0].len(),"model":self.embedding_model(settings)})
            }
            "rerank" => {
                json!({"results":self.rerank(settings,"排序算法",&["LightGBM 排序实验".into(),"会议安排".into()]).await?})
            }
            _ => bail!("不支持的模型类型"),
        };
        Ok(json!({"ok":true,"elapsed_ms":start.elapsed().as_millis(),"details":details}))
    }

    pub async fn stop(&self) {
        if let Some(mut local) = self.local.lock().await.take() {
            let _ = local.child.kill().await;
        }
    }
}

pub fn structured_json(text: &str) -> Result<Value> {
    let start = text.find('{').context("模型没有返回结构化 JSON")?;
    let end = text.rfind('}').context("模型 JSON 不完整")?;
    Ok(serde_json::from_str(&text[start..=end]).context("模型 JSON 格式无效，请重试")?)
}
