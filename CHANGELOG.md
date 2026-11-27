# Changelog / 更新日志

## 1.0.2 — 2026-11-27

- Named source collections retain user names independently of detected projects. File/search scopes include whole collections and subprojects; removal clears selectors and retrieval immediately and archives authored project context.
- 原生批量选择目录，每个目录独立命名；集合／子项目分别筛选；移除后下拉、搜索和 MCP 立即撤销，手写背景保留归档；部分失败可单独重试。
- New MCP connections automatically share eligible collections and later additions. Legacy selected scopes and independent memory permissions remain unchanged until explicitly edited.
- 新连接默认自动共享允许 Agent 使用的全部资料与后续新增；旧连接保留指定范围，可一次切换，无需重写客户端接入配置。
- One shared exclusion engine covers scanning, watching, parsing, cleanup and reads: Nuitka output suffixes, generated-bundle markers, dependencies/caches, category controls, binary checks, exact paths and glob rules. Skip reasons are visible; old excluded chunks/vectors are reclaimed even while discovery is paused.
- 统一自适应排除 Nuitka 构建目录、明确生成标记、依赖／缓存、不支持媒体／安装包和二进制内容；可查看原因、按文件／目录／扩展名排除或额外纳入。
- Independent review found and verified fixes for in-flight parsing after collection deletion, stale names in search hits, ancestor directory links, and edits during a partially failed batch request. Long directory forms keep their heading and action buttons visible.
- 独立审查修复在途解析写回已删除集合、搜索名称不同步、上层目录链接漏检，以及批量提交中编辑导致失败项丢失；长弹窗固定标题和操作区。
- Database schema 2 migration preserves existing chunk/vector IDs and client tokens. Older applications reject the upgraded schema; export a backup before upgrading.
- 数据库升级到版本 2，保留片段、向量与连接令牌；升级前建议备份，升级后使用 v1.0.2 或更新版本。

## 1.0.1 — 2026-11-26

- Simplified the desktop to one navigation rail and consistent forms, tables, dialogs and controls. Added persistent light/dark/system appearance, preset/custom accents, four font sizes, reading fonts, density and sidebar preferences; removed the sample-data entry point and generator.
- 客户端采用单层侧栏与统一的表单、列表、弹窗和控件；新增可保存的明亮／深色／跟随系统、预设／自定义强调色、四档字号、阅读字体、密度及侧栏偏好；移除示例资料入口及生成能力。

### Built-in guide / 内置使用导览

- Optional first-run guide over eight real interface steps, with automatic navigation, Previous/Next, Skip, close and Escape. Replay from the sidebar or Settings; restore the previous view, scroll and drafts. Completion/dismissal persists independently of provider and credential drafts, with visible save retry.
- 首次启动提供可跳过的 8 步真实界面导览，支持自动导航、上一步／下一步、跳过、关闭和 Escape；侧栏与设置可重放，退出还原原位置与草稿；完成／跳过记录独立保存并提供失败重试。
- Fresh libraries remain empty; the guide does not create sample data, invoke models, or configure clients. 新安装保持空知识库，导览不生成示例、不调用模型或自动配置客户端。

### Reliability fixes / 可靠性修复

- Theme expanded dropdowns, selection and keyboard focus with the app palette; normalize action heights, spacing and dialog footers; truncate long selected values while preserving full menu labels; replace decorative copy with direct instructions.
- Match settings saves to their section, preserving unrelated draft providers and credentials.
- Restrict short request timeouts to reads; await definite outcomes for model summaries and atomic restores. Validate backups and import data into a canonical schema before changing the current library.
- Account for reusable database pages; process removals in bounded durable jobs even when growth is blocked. Preserve warmed local-model children across cancelled queries and separate vectors for changed API routes.
- Journal interrupted migrations, reuse verified copies on explicit retry, preserve unknown/changed copies, and keep a normal quit from interrupting migration.
- Build metadata distinguishes unpublished working-tree packages from artifacts built from an exact source commit.
- Normalize Windows archive paths in package audits; use packaged executables directly in GUI checks. Verify short and long model inputs before packaging, preserve MSVC exception/path-remapping options, and commit local CPU vectors one chunk at a time while retaining API batching.
- Give background local CPU chunks an independent five-minute request limit, with cancellation on pause, provider/profile changes and shutdown; interactive fallback and department API deadlines stay bounded separately.

- 展开选单、选中及键盘焦点使用统一主题；规范按钮高度、间距与弹窗操作区；长名称省略显示并在菜单内保留全文；移除装饰性文案。
- 设置按分组保存，保留其他分组的模型与凭据草稿。
- 短超时仅用于读取；摘要与恢复等待确定结果。备份须先校验并导入标准结构，再替换当前库。
- 容量统计区分可复用页；禁止新增索引时仍以持久化小批任务清理已移除资料。本地模型在查询取消后继续预热，更换 API 路径隔离旧向量。
- 迁移增加作业记录，明确重试可复用完整副本；保留未知或已变化副本，正常退出等待迁移结束。
- 构建记录区分未发布工作区安装包和来自确定源码提交的安装包。
- 后台 CPU 向量计算使用独立的五分钟片段请求上限，暂停、更换模型／占用档位及退出可取消；交互检索与部门 API 保留各自时限。
- 包审计统一 Windows 路径，界面测试直接使用打包程序；打包前验证短／长文本向量，保留 MSVC 异常及路径重映射选项；本地 CPU 逐片段保存向量，API 批量处理保留。

### English

- Windows assisted installer with program-folder selection; independent data-location setup and verified migration with preserved agent tokens/keychain namespace and safe old-file cleanup.
- Free-space reserve, index-size limits, actionable disk-full messages, parser bounds and cleanup of temporary backup files.
- MCP initialize/tools-list no longer wait for backend health. HTTP handshakes bypass database writes; bounded request work and full-text fallback keep protocol responses responsive.
- Move tokenization outside the write lock, seek embedding batches with a cursor, use bounded read queries and incremental progress counters instead of repeated full-table counts.
- Separate file/chunk/vector progress, measured throughput/ETA, CPU profiles and an automatic local-work budget. Department Embedding APIs/API keys and the bundled Qwen CPU model remain supported; API batch size is configurable.
- Bounded character overlap and grouped short DOCX paragraphs with citations; project/build-manifest detection, project-scoped MCP guidance and explicit unsupported/empty-format reporting.
- Rotating service logs, cancellable parsing/model startup, Windows Job Objects for worker cleanup, orderly tray shutdown, and an English Windows executable name.

### 简体中文

- Windows 安装向导可选择程序目录；首次启动选择独立数据位置，可校验迁移并保留连接令牌和系统凭据，成功启动后清理旧文件。
- 增加剩余空间保护、索引容量限制、磁盘满提示、解析规模限制及备份临时文件清理。
- MCP 握手和工具发现不再等待后端健康检查；HTTP 握手绕开数据库写入，繁忙查询限时返回并保留全文降级。
- 分词移出写锁，向量任务使用游标批处理，查询限时，进度使用增量计数，减少大库重复扫描。
- 文件、分块、向量分阶段进度和实测速率/耗时估计；提供 CPU 档位与自动模式本地预算，完整保留部门 Embedding API/API Key 和随包 Qwen CPU 模型，并可设置 API 批量大小。
- 修复长行重叠放大，合并短 Word 段落并保留引用；扩展工程识别、项目限定调用说明，显示不支持格式和空提取。
- 增加滚动日志、解析/模型取消、Windows Job Object 子进程管理、托盘退出清理及英文 Windows 程序名。

Installers remain unsigned. Hosted synthetic tests do not substitute for acceptance with a real organization API/client or a large private corpus. / 安装包仍未签名；云端合成测试不替代真实公司接口、客户端和大规模私有资料验收。

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
