import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { request } from "./api";
import type { Appearance } from "./types";

export const defaultAppearance: Appearance = {
  theme: "light",
  accent: "graphite",
  font_size: 14,
  reading_font: "sans",
  density: "comfortable",
  sidebar_collapsed: false,
};
export const accentColors = [
  { id: "graphite", label: "石墨", color: "#27272a" },
  { id: "blue", label: "海蓝", color: "#245cdb" },
  { id: "green", label: "松绿", color: "#16735c" },
  { id: "violet", label: "鸢紫", color: "#6d47c4" },
  { id: "rose", label: "玫瑰", color: "#ac3c62" },
];
const cacheKey = "pkb.appearance.v1";
function cachedAppearance(): Appearance {
  try {
    const value =
      window.knowledge?.initialAppearance ||
      JSON.parse(localStorage.getItem(cacheKey) || "null");
    if (
      value &&
      ["light", "dark", "system"].includes(value.theme) &&
      [13, 14, 16, 18].includes(value.font_size) &&
      ["sans", "serif"].includes(value.reading_font) &&
      ["comfortable", "compact"].includes(value.density) &&
      typeof value.sidebar_collapsed === "boolean" &&
      (accentColors.some((item) => item.id === value.accent) ||
        /^#[0-9a-f]{6}$/i.test(value.accent))
    )
      return value;
  } catch {
    /* Storage can be unavailable in browser preview. */
  }
  return { ...defaultAppearance };
}

function rgb(hex: string) {
  return [1, 3, 5].map((offset) => parseInt(hex.slice(offset, offset + 2), 16));
}
function luminance(color: number[]) {
  const values = color.map((value) => {
    const s = value / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return values[0] * 0.2126 + values[1] * 0.7152 + values[2] * 0.0722;
}
function contrast(first: number[], second: number[]) {
  const a = luminance(first),
    b = luminance(second);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}
function accessibleInk(color: string, dark: boolean) {
  const start = rgb(color),
    background = rgb(dark ? "#171719" : "#ffffff");
  for (let step = 0; step <= 100; step++) {
    const mixed = start.map((channel) =>
      Math.round(channel + (((dark ? 255 : 0) - channel) * step) / 100),
    );
    if (contrast(mixed, background) >= 4.5)
      return (
        "#" + mixed.map((value) => value.toString(16).padStart(2, "0")).join("")
      );
  }
  return dark ? "#ffffff" : "#000000";
}

export function applyAppearance(value: Appearance) {
  const dark =
    value.theme === "dark" ||
    (value.theme === "system" &&
      matchMedia("(prefers-color-scheme: dark)").matches);
  const root = document.documentElement;
  root.dataset.theme = dark ? "dark" : "light";
  root.dataset.density = value.density;
  root.dataset.readingFont = value.reading_font;
  root.style.fontSize = value.font_size + "px";
  const color =
    value.accent === "graphite" && dark
      ? "#ebebef"
      : accentColors.find((item) => item.id === value.accent)?.color ||
        value.accent;
  root.style.setProperty("--primary", color);
  root.style.setProperty("--accent", accessibleInk(color, dark));
  root.style.setProperty(
    "--on-primary",
    contrast(rgb(color), [255, 255, 255]) >= contrast(rgb(color), [0, 0, 0])
      ? "#ffffff"
      : "#171719",
  );
  root.style.colorScheme = dark ? "dark" : "light";
  void window.knowledge?.setTheme(value.theme).catch(() => {});
}

// Run before React paints; the service snapshot remains authoritative.
applyAppearance(cachedAppearance());

interface AppearanceContextValue {
  appearance: Appearance;
  update: (changes: Partial<Appearance>) => void;
  hydrate: (value: Appearance) => void;
  status: "saved" | "saving" | "error";
  error: string;
  retry: () => void;
}
const AppearanceContext = createContext<AppearanceContextValue | null>(null);
export function AppearanceProvider({ children }: { children: ReactNode }) {
  const [appearance, setAppearance] = useState(cachedAppearance);
  const [status, setStatus] =
    useState<AppearanceContextValue["status"]>("saved");
  const [error, setError] = useState("");
  const current = useRef(appearance);
  const hydrated = useRef(false);
  const pending = useRef<Appearance | null>(null);
  const running = useRef(false);
  const preview = useCallback((value: Appearance) => {
    current.current = value;
    setAppearance(value);
    applyAppearance(value);
    try {
      localStorage.setItem(cacheKey, JSON.stringify(value));
    } catch {
      /* Optional paint cache. */
    }
  }, []);
  const flush = useCallback(async () => {
    if (running.current) return;
    running.current = true;
    setStatus("saving");
    setError("");
    try {
      // Serialize writes and coalesce fast changes so an older response cannot
      // replace a later selection. Failed changes remain visible and retryable.
      while (pending.current) {
        const value = pending.current;
        pending.current = null;
        try {
          await request<Appearance>("PATCH", "/appearance", value);
        } catch (caught) {
          pending.current ||= value;
          throw caught;
        }
      }
      setStatus("saved");
    } catch (caught) {
      setStatus("error");
      setError(
        caught instanceof Error ? caught.message : "外观设置未能保存，请重试。",
      );
    } finally {
      running.current = false;
    }
  }, []);
  const update = useCallback(
    (changes: Partial<Appearance>) => {
      const value = { ...current.current, ...changes };
      preview(value);
      pending.current = value;
      void flush();
    },
    [preview, flush],
  );
  const hydrate = useCallback(
    (value: Appearance) => {
      if (hydrated.current) return;
      hydrated.current = true;
      if (!pending.current && !running.current) preview(value);
    },
    [preview],
  );
  useEffect(() => {
    const query = matchMedia("(prefers-color-scheme: dark)");
    const changed = () => applyAppearance(current.current);
    query.addEventListener("change", changed);
    return () => query.removeEventListener("change", changed);
  }, []);
  return (
    <AppearanceContext.Provider
      value={{
        appearance,
        update,
        hydrate,
        status,
        error,
        retry: () => void flush(),
      }}
    >
      {children}
    </AppearanceContext.Provider>
  );
}
export function useAppearance() {
  const context = useContext(AppearanceContext);
  if (!context) throw new Error("外观设置尚未就绪");
  return context;
}
