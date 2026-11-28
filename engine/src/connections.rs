use crate::{state::AppState, types::*};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{collections::BTreeMap, io::Write, path::Path};

pub fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", identifier()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temporary, path)?;
    Ok(())
}

fn tokens(state: &AppState) -> BTreeMap<String, String> {
    std::fs::read(state.home.join("agent-tokens.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn token(state: &AppState, id: &str) -> Result<String> {
    tokens(state)
        .get(id)
        .cloned()
        .context("连接凭据不可用，请重新生成连接")
}

pub fn create(state: &AppState, input: Value) -> Result<Value> {
    let name = input["name"].as_str().unwrap_or("ZCode").trim();
    if name.is_empty() || name.chars().count() > 60 {
        bail!("连接名称应为 1–60 字");
    }
    let ids: Vec<String> = serde_json::from_value(input["project_ids"].clone()).unwrap_or_default();
    let known = state.store.projects()?;
    if ids
        .iter()
        .any(|id| !known.iter().any(|project| project.id == *id))
    {
        bail!("选择的项目不存在");
    }
    let client = Connection {
        id: identifier(),
        name: name.into(),
        kind: input["kind"].as_str().unwrap_or("zcode").into(),
        enabled: true,
        project_ids: ids,
        scope_mode: input["scope_mode"].as_str().unwrap_or("selected").into(),
        include_profile: input["include_profile"].as_bool().unwrap_or(false),
        can_propose: input["can_propose"].as_bool().unwrap_or(false),
        created_at: timestamp(),
        last_used: None,
        config_path: None,
    };
    if !["all","selected"].contains(&client.scope_mode.as_str()) { bail!("共享模式无效"); }
    let secret = format!(
        "{}{}",
        identifier().replace('-', ""),
        identifier().replace('-', "")
    );
    state.store.save_client(&client, Some(&secret))?;
    let mut values = tokens(state);
    values.insert(client.id.clone(), secret);
    private_write(
        &state.home.join("agent-tokens.json"),
        &serde_json::to_vec(&values)?,
    )?;
    config(state, &client.id)
}

pub fn config(state: &AppState, id: &str) -> Result<Value> {
    let client = state.store.client(id)?;
    let secret = token(state, id)?;
    let executable = if state.runtime.mcp_executable.is_file() {
        state.runtime.mcp_executable.clone()
    } else {
        std::env::current_exe()?
    };
    let server = json!({"command":executable,"args":["mcp","--role","mcp-stdio","--home",state.home,"--connection",id],"env":{"PKB_CONNECTION_TOKEN":secret}});
    let name = format!("personal-knowledge-{}", &id[..8]);
    let port = state.store.metadata::<u16>("http_port")?.unwrap_or(17481);
    Ok(
        json!({"connection":client,"name":name,"server":server,"config":{"mcpServers":{name.clone():server}},
        "http":{"url":format!("http://127.0.0.1:{port}/mcp/{id}"),"headers":{"Authorization":format!("Bearer {}",token(state,id)?)}},
        "instructions":"涉及我的项目、历史经验或个人偏好时，先调用个人知识库的 get_context。需要完整依据时调用 read_source，并在回答中引用原资料位置。得到值得长期保存的决定后，可以调用 propose_memory 提交建议。"}),
    )
}

pub fn update(state: &AppState, id: &str, input: Value) -> Result<Connection> {
    let _library=state.library_lock.lock().unwrap();
    let mut client = state.store.client(id)?;
    if let Some(name) = input["name"].as_str() {
        if name.trim().is_empty() {
            bail!("连接名称不能为空");
        }
        client.name = name.trim().into();
    }
    if let Some(enabled) = input["enabled"].as_bool() {
        client.enabled = enabled;
    }
    if let Some(profile) = input["include_profile"].as_bool() {
        client.include_profile = profile;
    }
    if let Some(propose) = input["can_propose"].as_bool() {
        client.can_propose = propose;
    }
    if let Some(mode) = input["scope_mode"].as_str() {
        if !["all","selected"].contains(&mode) { bail!("共享模式无效"); }
        client.scope_mode = mode.into();
    }
    if input.get("project_ids").is_some() {
        if input.get("scope_mode").is_none() { client.scope_mode="selected".into(); }
        let ids: Vec<String> = serde_json::from_value(input["project_ids"].clone())?;
        let projects = state.store.projects()?;
        if ids
            .iter()
            .any(|id| !projects.iter().any(|project| project.id == *id))
        {
            bail!("项目不存在");
        }
        client.project_ids = ids;
    }
    state.store.save_client(&client, None)?;
    Ok(client)
}

pub fn install_zcode(state: &AppState, id: &str) -> Result<Value> {
    let config = config(state, id)?;
    let home = directories::BaseDirs::new()
        .context("无法定位当前用户目录")?
        .home_dir()
        .to_path_buf();
    let path = home.join(".zcode/cli/config.json");
    install_zcode_at(state, id, &path, &home.join(".agents/mcp.json"), &config)
}

pub fn install_zcode_at(
    state: &AppState,
    id: &str,
    path: &Path,
    shared: &Path,
    config: &Value,
) -> Result<Value> {
    let original = std::fs::read(path).ok();
    let mut value: Value = match &original {
        Some(bytes) => serde_json::from_slice(bytes)
            .context("ZCode 配置 JSON 格式异常，请先在 ZCode 中修复后重试")?,
        None => json!({}),
    };
    if !value.is_object() {
        bail!("ZCode 配置顶层应为 JSON 对象");
    }
    if value.get("mcp").is_some_and(|item| !item.is_object()) {
        bail!("ZCode 的 mcp 配置格式异常");
    }
    if value.get("mcp").is_none() {
        value["mcp"] = json!({});
    }
    let current = value["mcp"]
        .get("servers")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !current.is_object() {
        bail!("ZCode 的 mcp.servers 配置格式异常");
    }
    let mut servers = current.as_object().unwrap().clone();
    if servers.is_empty() {
        if let Ok(bytes) = std::fs::read(shared) {
            let existing: Value = serde_json::from_slice(&bytes)
                .context("共享 MCP 配置格式异常，请先修复后再接入")?;
            if let Some(items) = existing["mcpServers"].as_object() {
                servers.extend(items.clone());
            }
        }
    }
    let name = config["name"].as_str().context("连接名称缺失")?;
    servers.insert(name.into(), config["server"].clone());
    value["mcp"]["servers"] = Value::Object(servers);
    let backup = if let Some(original) = &original {
        let backup = state.home.join("config-backups").join(format!(
            "zcode-{}.json",
            chrono::Utc::now().format("%Y%m%d-%H%M%S-%f")
        ));
        private_write(&backup, original)?;
        Some(backup)
    } else {
        None
    };
    if std::fs::read(path).ok() != original {
        bail!("ZCode 配置刚刚发生变化，请重试接入");
    }
    private_write(path, &serde_json::to_vec_pretty(&value)?)?;
    let mut client = state.store.client(id)?;
    client.config_path = Some(path.to_string_lossy().into_owned());
    state.store.save_client(&client, None)?;
    state.store.event(
        "connection",
        "已将知识库添加到 ZCode",
        json!({"connection_id":id,"path":path,"backup":backup}),
    )?;
    Ok(
        json!({"ok":true,"path":path,"backup":backup,"message":"已添加到 ZCode。新建一个会话后可以使用知识库。"}),
    )
}
