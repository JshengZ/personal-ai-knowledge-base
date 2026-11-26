import {
  BookOpenIcon,
  MagnifyingGlassIcon,
  FolderIcon,
  StackIcon,
  UserIcon,
  PlugsIcon,
  GearSixIcon,
  SidebarSimpleIcon,
  DatabaseIcon,
  QuestionIcon,
} from "@phosphor-icons/react";
import { useKnowledge, type View } from "../context";
import { useAppearance } from "../appearance";

const items = [
  { view: "search", label: "搜索与问答", Icon: MagnifyingGlassIcon },
  { view: "sources", label: "资料", Icon: FolderIcon },
  { view: "projects", label: "项目", Icon: StackIcon },
  { view: "profile", label: "关于我", Icon: UserIcon },
  { view: "agents", label: "Agent 连接", Icon: PlugsIcon },
] as const;

export function Sidebar() {
  const { data, view, navigate, startGuide } = useKnowledge();
  const { appearance, update } = useAppearance();
  const pending = data.memories.filter(
    (memory) => memory.status === "pending" || memory.status === "stale",
  ).length;
  const button = (target: View, label: string, Icon: typeof GearSixIcon) => (
    <button
      key={target}
      data-guide={"nav-" + target}
      className={view === target ? "active" : ""}
      onClick={() => navigate(target)}
      aria-label={label}
      title={label}
      aria-current={view === target ? "page" : undefined}
    >
      <Icon size={20} aria-hidden />
      <span className="nav-label">{label}</span>
      {target === "profile" && pending > 0 && (
        <span className="nav-count" aria-label={`${pending} 条记忆待确认`}>
          {pending}
        </span>
      )}
    </button>
  );
  return (
    <aside className="primary-sidebar">
      <div className="sidebar-brand-row">
        <button
          className="wordmark"
          onClick={() => navigate("sources")}
          aria-label="个人知识库首页"
          title="个人知识库"
        >
          <BookOpenIcon size={24} aria-hidden />
          <span className="nav-label">个人知识库</span>
        </button>
        <button
          className="icon-button sidebar-toggle"
          aria-label={appearance.sidebar_collapsed ? "展开侧栏" : "收起侧栏"}
          title={appearance.sidebar_collapsed ? "展开侧栏" : "收起侧栏"}
          onClick={() =>
            update({ sidebar_collapsed: !appearance.sidebar_collapsed })
          }
        >
          <SidebarSimpleIcon size={18} aria-hidden />
        </button>
      </div>
      <nav className="primary-navigation" aria-label="主导航">
        {items.map(({ view, label, Icon }) => button(view, label, Icon))}
      </nav>
      <nav className="secondary-navigation" aria-label="设置">
        {button("settings", "设置", GearSixIcon)}
        <button onClick={startGuide} aria-label="使用指南" title="使用指南">
          <QuestionIcon size={20} aria-hidden />
          <span className="nav-label">使用指南</span>
        </button>
      </nav>
      <div className="sidebar-foot" title="资料保存在当前电脑">
        <DatabaseIcon size={18} aria-hidden />
        <span className="nav-label">本机资料库</span>
      </div>
    </aside>
  );
}
