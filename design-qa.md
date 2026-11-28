# 界面验收 / Design QA — 1.0.1

2026-11-26。本地 Mac / Electron / Chrome 已通过；Windows 实机视觉验收仍需在目标电脑完成。当前实际界面截图来自隔离空知识库，路径标签已泛化，不包含个人路径、公司文档或凭据。

## 设计依据 / Design basis

参照 Grok 明亮界面的视觉克制，调整为本地资料和 MCP 工作流：单层侧栏、白色主画布、轻量列表、统一控件、按用途分组的设置。

- [资料页设计参考](docs/images/design-reference.png)与[实际页面](docs/images/desktop.png)，另有[完整并排对照](docs/images/design-comparison.png)
- [个性化设计参考](docs/images/appearance-reference.png)与[实际设置](docs/images/appearance.png)
- 参考和实现均以 1536 × 1024 对照检查；另检查 1024、800、390 px 宽度，以及特大字号 18 px。没有用截图替代实际控件。
- Browser plugin 不可用，CUA 入口超时；使用本地 Playwright 库控制 Electron，以及已安装的 Chrome。对概念图和最新实际截图执行 `view_image` 检查。

## 对照记录 / Fidelity ledger

| 比较项 / Point    | 参考与实现 / Evidence and result                                                                                                                               |
| ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 布局 / Layout     | 单层侧栏、宽主区域与开放的资料列表；移除旧的项目次级侧栏。资料范围放在列表与检索控件旁。                                                                       |
| 颜色 / Palette    | 默认真白背景、冷灰侧栏和细边框，黑灰文字；独立的深色主题。预设与自定义强调色在按钮、焦点和选中状态应用；自定义颜色计算可读前景色。                             |
| 字体 / Typography | 系统无衬线，正文与控件采用统一比例，默认 14 px、标题 24 px；13/14/16/18 px 四档全部生效。阅读衬线单独控制原文、回答和项目正文。                                |
| 间距 / Spacing    | 统一 40 px 标准控件、10 px 圆角、分隔线与表单对齐；紧凑密度控制行高和字段间距。弹窗统一标题、内容、操作区与滚动范围。                                          |
| 图标 / Icons      | Phosphor 正常线宽；侧栏与按钮图标对齐。无装饰大图、AI 光效或静态假按钮。外观缩略图为实际的单选控件。                                                           |
| 文案 / Copy       | 移除“工作室”等装饰性文案；空白状态直接说明操作，错误和状态说明保留具体原因。文件、进度和时间来自隔离的虚构测试数据。保留“处理详情”以承载队列、失败与权限说明。 |
| 窄屏 / Responsive | 侧栏缩为带名称和提示的图标；表单和表格重排。修复特大字号下的长模型名称横向溢出，390 px 无整页横向滚动。                                                        |

可解释的适配：侧栏按简洁性目标采用 224 px，收起为 68 px；默认字号保持可配置的 14 px，而非生成图中偏大的栅格文字。文件数量、格式和同步进度由实际数据决定。主题预览按钮用于检查当前颜色与字号。与设计方向对照检查后，以上适配之外没有已知的布局或交互阻断问题。

## 交互 / Interactions

- 真实项目编辑与保存；按项目筛选；全文检索与原文引用阅读。
- 目录、记忆、Agent 连接弹窗打开与 Escape 关闭；标准焦点、选中和禁用状态。
- 所有设置分组可切换；尚未保存的模型字段保留。
- 明亮、深色与系统主题；预设及自定义颜色；四档字号；阅读字体；密度；侧栏；恢复默认外观。
- 外观独立自动保存；连续选择串行合并，失败显示重试，重试后保留选择并恢复保存状态。
- 原生数据迁移并重启后，全部外观选项保留；模型设置和资料权限保持原设置。正常退出释放后台服务。
- 源码 UI 验收无相关页面异常或控制台错误；安装包模型、OCR、检索与 MCP 验收见[验证记录](docs/verification.zh-CN.md)。

The implemented UI was inspected against both design references, including layout, palette, typography, spacing, icons, copy and responsive behavior. Functional tests cover saved appearance, recovery from a failed save, restart/migration, unsaved model fields, real search and source dialogs. The local result is verified; native Windows visual differences remain an environment acceptance item.

## 控件与按钮复查 / Controls and actions review

2026-11-26，v1.0.1 控件复查。最新实现在以下细节上覆盖早期概念图：选单使用应用背景、边框、阴影及轻量选中状态；移除叠加的粗焦点框；主题选项、复选框、分段控件和折叠箭头采用统一状态。明确保留键盘焦点，不以移除焦点换取视觉简洁。

- [明亮设置页](docs/images/appearance.png)、[展开选单](docs/images/controls.png)、[Agent 连接权限表单](docs/images/actions.png)为当前实际 UI 的隔离截图。此前并排图与概念图仅表示早期方向，其中旧文案和控件状态不再是实现要求。
- 1440 / 800 / 640 / 390 px 检查 12 类页面和弹窗，包括设置全部分类、目录管理、项目编辑和连接权限；390 px 使用 18 px 字号。按钮与页面均无横向越界。
- 主要按钮高度、图标尺寸、操作组间距及弹窗底部按钮对齐。长名称在收起的下拉框内省略，在展开菜单内换行；多行复选项按首行对齐。
- 原生 select 与 selectedcontent 保留控件语义和键盘行为；鼠标选择、Space / 方向键 / Enter、复选框、深色及自定义强调色、弹窗选单均通过。相关页面与控制台异常为零。
- 浏览器视觉证据来自已安装 Chrome；Electron 控件和持久化流程另做原生回归。原生 Windows 检查以对应源码和安装包工作流的实际运行记录为准；公司电脑的视觉差异仍需实机验收。

The local post-review UI replaces decorative copy with direct instructions, aligns action groups and dialog footers, and themes expanded native selects and focus states consistently. Twelve page/dialog flows were checked at four widths, including 18 px text at 390 px. Current screenshots supersede legacy concept controls/copy. Target-workstation visual acceptance remains an environment check.

## 可选使用导览 / Optional product guide

首次启动以空知识库为背景展示可跳过的入口。8 个步骤自动导航实际页面并定位真实控件；支持上一步、下一步、跳过、关闭与 Escape。短窗口保留正文阅读空间；无法同时容纳标记与说明时使用居中卡片，保留退出与翻页控件。

隔离 Chrome 流程覆盖 1212／640／390／320 px、明暗主题、14／18 px 字号及保存失败重试。原生 Electron 验证跳过／完成后的重启、8 步无业务写入、表单草稿保留、最小窗口、失败退出与重试，并测试导览访问搜索页后的项目提问。

独立子智能体发现并复测修复了 3 项 P2：缓存搜索页的项目提问状态不同步；并发设置保存覆盖导览标志（真实引擎修复后 600 轮通过）；640×520、18 px、紧凑模式下正文可被挤至 0 高（现有可读居中降级）。另验证目标缺失／迟到、原设置分类、500 px 滚动、草稿及焦点恢复。没有未关闭的可操作发现；未独立验证屏幕阅读器、Windows GUI 或高倍率缩放。

Sources for interaction patterns: [Fluent onboarding](https://fluent2.microsoft.design/onboarding) and [Atlassian Spotlight](https://atlassian.design/components/spotlight/examples). The implementation uses existing React and native browser primitives; it adds no tour dependency or model call. Screenshots and verification correspond to v1.0.1; release artifacts identify their exact source commit.

## 1.0.2 目录与范围 / Collections and scope

2026-11-27。新增目录批量列表、独立名称、内容类别与规则、跳过原因、文件排除、集合范围和自动共享选择，沿用现有明亮／深色主题、字号、间距与控件。目录添加和管理弹窗采用固定标题与底部操作区，中间独立滚动。

独立子智能体检查 1200×820、640×520，包含批量部分失败重试、集合／项目切换、改名、自动／指定范围回显；提交期间整组表单禁用，避免失败项丢失。实际截图：[批量目录](docs/images/collections-batch.png)、[集合选单](docs/images/collections-scopes.png)、[自动共享](docs/images/collections-sharing.png)。合成资料只用于验收，不生成到用户新库。

New collection controls reuse the established theme and sizing. Long folder forms keep their heading and actions visible while the middle scrolls. Independent acceptance checks constrained windows, partial retry, renamed scopes and saved sharing modes; the whole batch form is disabled during submission.

## 1.1.0 资料与运行状态 / Library and runtime status

- 206 个真实临时文件验证 100 → 200 → 205 条分页，后台新增后保持 206 条；一次读取失败保留列表并重试，筛选切换不混入旧范围，加载状态不冒充空库。
- 状态面板默认折叠，沿用线条／字体／主题与间距；展开显示任务与队列，1440 px 双列、640 px 单列，未见横向溢出。
- 新连接范围默认占位，未选择时禁用提交；指定项目初始为空，自动共享需明确选择。设置使用已有 Select 样式提供静默窗口和直连，导览说明同步更新。
- Evidence: `docs/images/runtime-status.png`, `runtime-status-narrow.png`, `sharing-choice.png`, `model-network.png`, `indexing-policy.png`. Fixtures are synthetic and not installed sample data.
