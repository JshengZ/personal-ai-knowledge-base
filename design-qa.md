# 界面验收 / Design QA — 1.0.1

2026-11-26。本地 Mac / Electron / Chrome 已通过；Windows 实机视觉验收仍需在目标电脑完成。所有公开截图来自隔离的虚构验收资料，不包含个人路径、公司文档或凭据。

## 设计依据 / Design basis

参照 Grok 明亮界面的视觉克制，调整为本地资料和 MCP 工作流：单层侧栏、白色主画布、轻量列表、统一控件、按用途分组的设置。

- [资料页设计参考](docs/images/design-reference.png)与[实际页面](docs/images/desktop.png)，另有[完整并排对照](docs/images/design-comparison.png)
- [个性化设计参考](docs/images/appearance-reference.png)与[实际设置](docs/images/appearance.png)
- 参考和实现均以 1536 × 1024 对照检查；另检查 1024、800、390 px 宽度，以及特大字号 18 px。没有用截图替代实际控件。
- Browser plugin 不可用，CUA 入口超时；使用本地 Playwright 库控制 Electron，以及已安装的 Chrome。对概念图和最新实际截图执行 `view_image` 检查。

## 对照记录 / Fidelity ledger

| 比较项 / Point | 参考与实现 / Evidence and result |
| --- | --- |
| 布局 / Layout | 单层侧栏、宽主区域与开放的资料列表；移除旧的项目次级侧栏。资料范围放在列表与检索控件旁。 |
| 颜色 / Palette | 默认真白背景、冷灰侧栏和细边框，黑灰文字；独立的深色主题。预设与自定义强调色在按钮、焦点和选中状态应用；自定义颜色计算可读前景色。 |
| 字体 / Typography | 系统无衬线，正文与控件采用统一比例，默认 14 px、标题 24 px；13/14/16/18 px 四档全部生效。阅读衬线单独控制原文、回答和项目正文。 |
| 间距 / Spacing | 统一 40 px 标准控件、10 px 圆角、分隔线与表单对齐；紧凑密度控制行高和字段间距。弹窗统一标题、内容、操作区与滚动范围。 |
| 图标 / Icons | Phosphor 正常线宽；侧栏与按钮图标对齐。无装饰大图、AI 光效或静态假按钮。外观缩略图为实际的单选控件。 |
| 文案 / Copy | 主标题、导航、设置分组与设计清单对照；文件、进度和时间来自真实的隔离测试数据。保留“处理详情”以承载队列、失败与权限说明。 |
| 窄屏 / Responsive | 侧栏缩为带名称和提示的图标；表单和表格重排。修复特大字号下的长模型名称横向溢出，390 px 无整页横向滚动。 |

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
