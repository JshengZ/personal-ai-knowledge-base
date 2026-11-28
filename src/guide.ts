import type { SettingsTab, View } from "./context";

export type GuideStatus = "skipped" | "completed";
export interface GuideStep {
  id: string;
  view: View;
  tab?: SettingsTab;
  target: string;
  location: string;
  title: string;
  description: string;
  detail: string;
}

// Stable IDs anchor to real controls. Steps only change presentation state;
// they never call a folder, model, search, connection or memory mutation.
export const guideSteps: readonly GuideStep[] = [
  {
    id: "sources",
    view: "sources",
    target: '[data-guide="add-source"]',
    location: "资料",
    title: "添加资料目录",
    description:
      "选择需要索引的项目或文档文件夹。以后新增、修改和删除文件，索引会自动更新。",
    detail:
      "可一次选择多个目录、分别命名；在“资料目录”中调整访问权限和排除规则。",
  },
  {
    id: "progress",
    view: "sources",
    target: '[data-guide="index-progress"]',
    location: "资料 → 处理进度",
    title: "查看索引进度",
    description:
      "这里分别显示目录扫描、文档解析和向量计算的进度。全文就绪后，即可按关键词检索。",
    detail: "向量计算可以稍后完成。同步可暂停，资源占用可在设置中调整。",
  },
  {
    id: "models",
    view: "settings",
    tab: "models",
    target: '[data-guide="embedding-mode"]',
    location: "设置 → 模型与检索",
    title: "选择检索方式",
    description:
      "语义检索可使用部门 Embedding API、随包 CPU 模型，或切换为仅全文检索。",
    detail:
      "知识库窗口中的问答与项目摘要需要对话模型；使用 Agent 自带模型时可稍后配置。",
  },
  {
    id: "agents",
    view: "agents",
    target: '[data-guide="add-connection"]',
    location: "Agent 连接",
    title: "通过 MCP 接入 Agent",
    description:
      "创建连接时先选择共享范围，可自动共享或指定项目。ZCode 可一键配置。",
    detail:
      "选择自动共享后，后续新增资料持续纳入。个人记忆与记忆建议分别授权。",
  },
  {
    id: "search",
    view: "search",
    target: '[data-guide="search"]',
    location: "搜索与问答",
    title: "检索资料与核对来源",
    description:
      "输入关键词或问题，按资料集合、项目和文件类型筛选。点击检索结果或回答中的引用，可以查看原文位置。",
    detail: "尚未配置对话模型时，仍可搜索原文。",
  },
  {
    id: "projects",
    view: "projects",
    target: '[data-guide="nav-projects"]',
    location: "项目",
    title: "按项目查看资料",
    description:
      "添加资料后，每个目录保留自定义名称，其内部代码项目单独识别。这里可维护项目目标、当前方案与关键决策。",
    detail: "项目摘要可手动填写，也可使用已配置的对话模型生成。",
  },
  {
    id: "profile",
    view: "profile",
    target: '[data-guide="nav-profile"]',
    location: "关于我",
    title: "确认个人记忆",
    description:
      "模型或 Agent 提交的记忆建议会先等待确认。可以修改、忽略或删除，也可以手动添加。",
    detail: "原资料变化后，相关记忆会标记为待复核。",
  },
  {
    id: "storage",
    view: "settings",
    tab: "storage",
    target: '[data-guide="storage-limits"]',
    location: "设置 → 存储与备份",
    title: "管理存储与外观",
    description:
      "设置索引容量和预留空间，或在桌面端迁移数据位置。程序安装目录与知识库数据目录相互独立。",
    detail:
      "“个性化”可调整主题、颜色、字号和密度。可随时从“使用指南”重新打开导览。",
  },
];
