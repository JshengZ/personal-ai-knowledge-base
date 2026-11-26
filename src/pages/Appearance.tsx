import {
  CheckIcon,
  CircleHalfIcon,
  MoonIcon,
  SunIcon,
  EyedropperIcon,
  CheckCircleIcon,
} from "@phosphor-icons/react";
import { accentColors, defaultAppearance, useAppearance } from "../appearance";
import { useKnowledge } from "../context";
import { Select, Busy } from "../components/ui";
import type { Appearance } from "../types";

export function AppearancePage() {
  const { appearance, update, status } = useAppearance();
  const { notify } = useKnowledge();
  const custom = appearance.accent.startsWith("#");
  return (
    <section className="appearance-panel" aria-labelledby="appearance-heading">
      <div className="appearance-heading">
        <h2 id="appearance-heading">个性化</h2>
        <span className="appearance-save-state" role="status">
          {status === "saving" ? (
            <Busy>正在保存</Busy>
          ) : status === "error" ? (
            "尚未保存"
          ) : (
            <>
              <CheckCircleIcon size={16} aria-hidden />
              已自动保存
            </>
          )}
        </span>
      </div>
      <div className="appearance-row">
        <div>
          <h3>外观</h3>
          <p id="theme-description">选择明亮、深色，或跟随系统。</p>
        </div>
        <fieldset
          className="theme-options"
          aria-label="外观"
          aria-describedby="theme-description"
        >
          {(
            [
              { id: "light", label: "明亮", Icon: SunIcon },
              { id: "dark", label: "深色", Icon: MoonIcon },
              { id: "system", label: "跟随系统", Icon: CircleHalfIcon },
            ] as const
          ).map(({ id, label, Icon }) => (
            <label className="theme-option" key={id}>
              <input
                type="radio"
                className="choice-input"
                name="appearance-theme"
                value={id}
                checked={appearance.theme === id}
                onChange={() => update({ theme: id })}
              />
              <span className={"theme-preview theme-preview-" + id} aria-hidden>
                <Icon size={27} />
                <span className="theme-preview-lines">
                  <span />
                  <span />
                  <span />
                </span>
              </span>
              <span className="theme-option-label">
                <span className="radio-indicator">
                  {appearance.theme === id && (
                    <CheckIcon size={10} weight="bold" />
                  )}
                </span>
                {label}
              </span>
            </label>
          ))}
        </fieldset>
      </div>
      <div className="appearance-row">
        <div>
          <h3>主题颜色</h3>
          <p>用于选中状态与主要操作。</p>
        </div>
        <fieldset className="accent-options" aria-label="主题颜色">
          {accentColors.map(({ id, label, color }) => (
            <label
              className="accent-option"
              title={label}
              key={id}
              style={{ "--swatch": color } as React.CSSProperties}
            >
              <input
                className="choice-input"
                type="radio"
                name="appearance-accent"
                value={id}
                checked={appearance.accent === id}
                onChange={() => update({ accent: id })}
              />
              <span className="sr-only">{label}</span>
              <span className="color-swatch" aria-hidden>
                {appearance.accent === id && (
                  <CheckIcon size={16} weight="bold" />
                )}
              </span>
            </label>
          ))}
          <label
            className={"custom-color-control" + (custom ? " selected" : "")}
            title={custom ? appearance.accent : "自定义主题颜色"}
          >
            <EyedropperIcon size={16} aria-hidden />
            <span>自定义</span>
            <input
              aria-label="自定义主题颜色"
              type="color"
              value={
                custom
                  ? appearance.accent
                  : accentColors.find((item) => item.id === appearance.accent)!
                      .color
              }
              onChange={(event) => update({ accent: event.target.value })}
            />
            {custom && (
              <span
                className="custom-color-dot"
                style={{ background: appearance.accent }}
                aria-hidden
              />
            )}
          </label>
        </fieldset>
      </div>
      <div className="appearance-row">
        <div>
          <h3>字体大小</h3>
          <p>整个界面的文字与控件同步缩放。</p>
        </div>
        <fieldset className="segmented" aria-label="字体大小">
          {(
            [
              { size: 13, label: "小" },
              { size: 14, label: "标准" },
              { size: 16, label: "大" },
              { size: 18, label: "特大" },
            ] as const
          ).map(({ size, label }) => (
            <label key={size}>
              <input
                type="radio"
                className="choice-input"
                name="appearance-font-size"
                checked={appearance.font_size === size}
                onChange={() => update({ font_size: size })}
              />
              <span>{label}</span>
            </label>
          ))}
        </fieldset>
      </div>
      <div className="appearance-row">
        <div>
          <h3>
            <label htmlFor="reading-font">阅读字体</label>
          </h3>
          <p>仅用于原文、回答和项目正文。</p>
        </div>
        <Select
          id="reading-font"
          value={appearance.reading_font}
          onChange={(event) =>
            update({
              reading_font: event.target.value as Appearance["reading_font"],
            })
          }
        >
          <option value="sans">无衬线</option>
          <option value="serif">衬线</option>
        </Select>
      </div>
      <div className="appearance-row">
        <div>
          <h3>界面密度</h3>
          <p>调整列表行高和表单间距。</p>
        </div>
        <fieldset className="segmented density-options" aria-label="界面密度">
          {(
            [
              { id: "comfortable", label: "舒适" },
              { id: "compact", label: "紧凑" },
            ] as const
          ).map(({ id, label }) => (
            <label key={id}>
              <input
                type="radio"
                className="choice-input"
                name="appearance-density"
                checked={appearance.density === id}
                onChange={() => update({ density: id })}
              />
              <span>{label}</span>
            </label>
          ))}
        </fieldset>
      </div>
      <div className="appearance-row">
        <div>
          <h3>
            <label htmlFor="sidebar-compact">收起侧栏</label>
          </h3>
          <p>侧栏仅显示图标。</p>
        </div>
        <input
          id="sidebar-compact"
          className="switch-input"
          type="checkbox"
          role="switch"
          checked={appearance.sidebar_collapsed}
          onChange={(event) =>
            update({ sidebar_collapsed: event.target.checked })
          }
        />
      </div>
      <div className="appearance-preview">
        <div>
          <h3>阅读预览</h3>
          <p className="reading-text">
            当前字体的中文、English 与数字 0123456789。
          </p>
          <small>用于查看字体、字号和主题颜色。</small>
        </div>
        <button
          type="button"
          className="primary"
          onClick={() => notify("当前颜色与字号已应用。")}
        >
          预览按钮
        </button>
      </div>
      <div className="appearance-footer">
        <p>更改会立即生效，并保存在本机。</p>
        <button
          className="text-action"
          type="button"
          onClick={() => update({ ...defaultAppearance })}
        >
          恢复默认外观
        </button>
      </div>
    </section>
  );
}
