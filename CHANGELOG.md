# Changelog / 更新日志

## 1.0.0 — 2026-11-25

### English

- Local desktop workspace for projects, sources, search, memories, agent connections, and settings.
- Watched folders, persistent indexing, change/rename/delete reconciliation, exclusions, retries, and source-version invalidation.
- Chinese-aware FTS, local Qwen3 embeddings, hybrid retrieval, optional organization model/rerank adapters, and citations.
- MCP stdio and Streamable HTTP with five tools and per-connection permissions; one-click ZCode configuration with backups.
- Local PDF, Office, text/code, notebook, image, and scanned-PDF parsing; bundled English/Chinese OCR resources.
- Confirmed/pending/stale memory review, evidence-backed summaries, and portable knowledge backups excluding credentials.
- Intel Mac DMG and Windows x64 installer, SHA-256 checksums, bilingual documentation, contribution templates, and CI.
- Public builds remap machine-specific compiler paths and exclude local knowledge, credentials, and developer artifacts.
- Normalize PDF.js resource paths for Windows font and WebAssembly loading.

**Validation:** packaged Mac application tested with real local embeddings, PDF/OCR/XLSX, project filtering, MCP, and memory review. Windows installer is cross-built; target workstation, real organization API, and actual ZCode integration need acceptance in those environments. Installers are unsigned. Desktop UI is Simplified Chinese.

### 简体中文

- 提供项目、资料、检索、记忆、Agent 和设置的完整桌面工作区。
- 自动监听目录、持久化索引、文件变更/重命名/删除核对、排除规则、重试与来源失效。
- 中文全文检索、本地 Qwen3 向量、混合排序、公司模型与精排适配、原文引用。
- stdio / Streamable HTTP MCP、五个工具、连接权限及保留原配置的一键 ZCode 接入。
- 本机 PDF、Office、文本代码、Notebook、图片和扫描 PDF 解析，随包中英文 OCR。
- 记忆确认与复核、带证据项目摘要、排除凭据的知识备份。
- Mac Intel / Windows x64 安装包、校验和、中英文文档、贡献模板与 CI。
- 公开构建清理本机编译路径，不包含本地知识、凭据和开发数据。
- 规范 PDF.js 字体与 WebAssembly 路径，修复 Windows PDF 解析。

**验收范围：** Mac 打包应用已实测真实向量、PDF/OCR/XLSX、项目过滤、MCP 和记忆审核。Windows 安装包为交叉构建，目标电脑、真实公司 API 与 ZCode 客户端仍需实地验收。当前未签名，桌面界面为简体中文。
