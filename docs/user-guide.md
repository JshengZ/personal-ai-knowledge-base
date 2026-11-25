# User guide

[English](user-guide.md) · [简体中文](user-guide.zh-CN.md) · [Home](../README.md)

## Installation

Download your platform's installer from [GitHub Releases](https://github.com/JshengZ/personal-ai-knowledge-base/releases/latest). The source-code ZIP is for developers and is not an installer.

- **Intel Mac:** open the `.dmg`, drag **个人知识库** to Applications, then launch it. If macOS blocks this unsigned build, use System Settings → Privacy & Security to authorize opening the app. No terminal command is required.
- **Windows x64:** double-click the `.exe`. It installs for the current user and creates a shortcut. The build is unsigned and may display an unknown-publisher prompt. Managed workstations may require your organization's normal software authorization.

The application bundles its runtime, local embedding model, fonts, and OCR languages. There is no project account to register. Native Apple Silicon and ARM Windows installers are not part of v1.0.0.

## First folder

Click **选择目录** and choose a project or work-document folder, then **加入知识库**. The app indexes that folder and its descendants. The initial permission allows organization-model and agent use; change it later in **资料 → 资料目录** if needed.

You can add multiple folders. Git working directories are recognized as projects. Overlapping folders are managed through the already-added root. Git internals, dependency directories, build output, and common credential files are excluded by default; add glob exclusions in the folder editor.

The **用示例资料体验** button creates separate, synthetic ranking-project files. It does not scan arbitrary folders.

## Model settings

For in-app chat and project summaries, enter the API base URL, deployed model name, protocol, and optional API key in **设置 → 公司模型**. OpenAI-compatible Chat Completions and Anthropic Messages are supported. A custom endpoint can be set in the advanced field. Test the saved settings before using work documents.

For semantic retrieval, choose a mode in **语义检索**:

- **Auto:** use the configured organization Embedding API; otherwise use the bundled Qwen model.
- **Local:** always use the bundled Qwen3-Embedding-0.6B model.
- **Full-text only:** disable vector retrieval.

An organization chat endpoint does not necessarily provide embeddings. Leave the Embedding API blank if none is available. Optional reranking has its own endpoint and model setting. Keys are stored in the OS credential store and are not included in backups or shared installers.

You can skip chat setup when using an agent's existing model: MCP supplies source context and your agent composes the answer.

## Connect an agent

1. Open **Agent 连接 → 添加连接**.
2. Choose the client and authorized projects. Decide whether it may read confirmed personal memories and submit memory suggestions.
3. For ZCode, click **一键配置 ZCode**. Existing settings are preserved and backed up. For nonstandard installations, copy the generated settings into your client instead.
4. For another client, copy the generated JSON, Codex TOML, or local HTTP settings into its MCP configuration UI.
5. Start a new conversation. Ask: “What algorithm does my ranking project use? Call the knowledge tools and cite the original sources.”

The **测试共享范围** button checks the knowledge service and permissions. Actual integration still needs a tool call from your client. Suggested usage rules in **接入配置** help an agent use the knowledge base proactively.

Tools: `get_context`, `search`, `read_source`, `list_projects`, `propose_memory`. Proposals require review; they do not directly become confirmed facts.

## Search, summaries, and memories

- **项目:** review/edit goals, approaches, and decisions; generate an evidence-backed summary using the configured chat API.
- **搜索与问答:** filter by project or file type. Click a result or citation to read its original page, line, paragraph, slide, or cell range.
- **关于我:** confirm, correct, dismiss, or delete memory suggestions. When a source changes, dependent confirmed memories become **待复核**.

Role or skill inferences are suggestions supported by sources. They do not train a model or establish a verified job title.

## Synchronization and backup

New files, modifications, renames, and deletions are tracked inside authorized folders. Periodic reconciliation also catches events missed during sleep or downtime. Use **立即核对**, **暂停同步**, and **继续同步** to control processing. Errors and retry controls appear in the source view.

Closing the window keeps the tray service running. Use the tray or app menu to quit. Enable login startup in Settings if desired. A stdio MCP client can start the service when needed.

Use **设置 → 备份知识库** to export `.pkb` and **恢复备份** to restore it. Original files, API keys, and agent tokens are not transferred. Restore creates a recovery snapshot, pauses synchronization, disables roots, and requires relinking folders, re-entering credentials, and recreating client connections.

## Troubleshooting and boundaries

| Situation | What to check |
| --- | --- |
| Sources appear without an answer | Configure the chat API, or let your MCP agent answer from sources. |
| Only keyword retrieval | Check retrieval mode and its connection test; vector indexing may still be running. |
| An agent sees no sources | Check folder policy, project scope, connection status, and indexing progress. |
| A source is unavailable | Wait for indexing or reconnect the drive; stale/inaccessible originals are withheld. |
| A document fails | Check the source-list error; decrypt protected files and convert legacy Office formats. |
| A model connection fails | Check network, protocol, model name, endpoint, credentials, and organization CA trust. |

The maximum configured file size is 100 MB. Backup uploads are limited to 512 MB and restored databases to 1 GiB. Large OCR pages have resource limits. Audio/video and online enterprise-document connectors are not included. See [verification](verification.md) for tested and pending environments.
