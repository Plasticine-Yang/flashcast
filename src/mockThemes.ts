// 浏览器模拟宿主的主题数据。
//
// 真实的主题解析在 `flashcast-core::theme`：声明式 JSON → 语义 token → CSS 自定义
// 属性。这里只复刻 UI 需要的那一层（CSS 变量与列表语义），供 `pnpm dev` 与
// `pnpm ui-check` 使用。**模拟宿主不是产品行为**，真实行为由
// `crates/flashcast-core/tests/themes.rs` 的集成测试覆盖。
//
// 变量名称与 `ThemeTokens::css_vars` 一一对应；`pnpm ui-check` 会断言这份映射没有
// 缺口（数量与必需变量名都要对得上）。

import type { Appearance, CssVar, ThemeAppearance, ThemeEntry, ThemeState, SurfaceStyle } from "./types";

/** 浏览器模拟宿主认得的主题。 */
export interface MockTheme {
  styles?: SurfaceStyle[];
  id: string;
  name: string;
  version: string;
  appearance: ThemeAppearance;
  builtin: boolean;
  enabled: boolean;
  /** 声明式 token 的 CSS 变量映射（浅色 / 深色两套）。 */
  palettes: { light: Record<string, string>; dark: Record<string, string> };
}

/** 内置浅色主题的语义 token（与 `light_tokens()` 的映射一致）。 */
export const LIGHT_VARS: Record<string, string> = {
  "--fc-page-bg": "#ffffff",
  "--fc-surface": "#ffffff",
  "--fc-hover-bg": "rgba(13, 15, 20, 0.045)",
  "--fc-border": "rgba(13, 15, 20, 0.10)",
  "--fc-border-strong": "rgba(13, 15, 20, 0.20)",
  "--fc-text": "#0d0f14",
  "--fc-text-muted": "#5f6673",
  "--fc-text-disabled": "#838a96",
  "--fc-accent": "#2b62ff",
  "--fc-accent-ink": "#ffffff",
  "--fc-info-bg": "rgba(13, 15, 20, 0.045)",
  "--fc-warning-bg": "rgba(176, 118, 12, 0.12)",
  "--fc-warning-border": "rgba(176, 118, 12, 0.42)",
  "--fc-warning-text": "#7a4f08",
  "--fc-icon-fallback-bg": "rgba(13, 15, 20, 0.06)",
  "--fc-selection-bg": "rgba(43, 98, 255, 0.09)",
  "--fc-selection-border": "rgba(43, 98, 255, 0.45)",
  "--fc-focus-ring": "rgba(43, 98, 255, 0.95)",
  "--fc-error-bg": "rgba(220, 38, 38, 0.10)",
  "--fc-error-border": "rgba(220, 38, 38, 0.5)",
  "--fc-error-text": "#a11313",
  "--fc-disabled-opacity": "0.5",
  "--fc-font-family":
    'system-ui, -apple-system, "Noto Sans CJK SC", "Source Han Sans SC", "PingFang SC", "Microsoft YaHei", sans-serif',
  "--fc-font-body": "14px",
  "--fc-font-input": "16px",
  "--fc-font-aux": "12px",
  "--fc-space-window-padding": "14px",
  "--fc-space-row-padding": "6px",
  "--fc-space-row-gap": "10px",
  "--fc-space-section-gap": "8px",
  "--fc-row-height": "44px",
  "--fc-radius-window": "14px",
  "--fc-radius-item": "8px",
  "--fc-radius-control": "8px",
  "--fc-shadow": "0 24px 56px rgba(18, 28, 55, 0.18), 0 2px 6px rgba(18, 28, 55, 0.08)",
  "--fc-shadow-overlay": "0 14px 30px rgba(18, 28, 55, 0.26)",
};

/** 内置深色主题的语义 token。 */
export const DARK_VARS: Record<string, string> = {
  "--fc-page-bg": "rgba(24, 25, 28, 0.98)",
  "--fc-surface": "#202226",
  "--fc-hover-bg": "rgba(255, 255, 255, 0.06)",
  "--fc-border": "rgba(255, 255, 255, 0.14)",
  "--fc-border-strong": "rgba(255, 255, 255, 0.28)",
  "--fc-text": "#ecedf0",
  "--fc-text-muted": "#a1a7b3",
  "--fc-text-disabled": "#767c88",
  "--fc-accent": "#7aa2f7",
  "--fc-accent-ink": "#000000",
  "--fc-info-bg": "rgba(255, 255, 255, 0.06)",
  "--fc-warning-bg": "rgba(245, 158, 11, 0.18)",
  "--fc-warning-border": "rgba(245, 158, 11, 0.5)",
  "--fc-warning-text": "#fcd34d",
  "--fc-icon-fallback-bg": "rgba(255, 255, 255, 0.1)",
  "--fc-selection-bg": "rgba(122, 162, 247, 0.24)",
  "--fc-selection-border": "rgba(122, 162, 247, 0.75)",
  "--fc-focus-ring": "rgba(122, 162, 247, 0.95)",
  "--fc-error-bg": "rgba(248, 113, 113, 0.16)",
  "--fc-error-border": "rgba(248, 113, 113, 0.6)",
  "--fc-error-text": "#fecaca",
  "--fc-disabled-opacity": "0.5",
  "--fc-font-family":
    'system-ui, -apple-system, "Noto Sans CJK SC", "Source Han Sans SC", "PingFang SC", "Microsoft YaHei", sans-serif',
  "--fc-font-body": "14px",
  "--fc-font-input": "16px",
  "--fc-font-aux": "12px",
  "--fc-space-window-padding": "14px",
  "--fc-space-row-padding": "6px",
  "--fc-space-row-gap": "10px",
  "--fc-space-section-gap": "8px",
  "--fc-row-height": "44px",
  "--fc-radius-window": "14px",
  "--fc-radius-item": "8px",
  "--fc-radius-control": "6px",
  "--fc-shadow": "0 8px 28px rgba(0, 0, 0, 0.45)",
  "--fc-shadow-overlay": "0 12px 32px rgba(0, 0, 0, 0.55)",
};

/** 变量名清单：UI 检查用它断言映射完整。 */
export const REQUIRED_THEME_VARS: string[] = Object.keys(LIGHT_VARS);

export const SOLID_STYLE: SurfaceStyle = { id: "solid", name: "实底", renderer: "solid",
  light: { fillOpacity: 1, blur: 0, saturation: 1, rim: 0 },
  dark: { fillOpacity: 1, blur: 0, saturation: 1, rim: 0 } };
export const ARC_STYLES: SurfaceStyle[] = [
  { id: "frosted", name: "毛玻璃", renderer: "frosted",
    light: { fillOpacity: .91, blur: 26, saturation: 1.15, rim: 0 },
    dark: { fillOpacity: .88, blur: 26, saturation: 1.15, rim: 0 } },
  { id: "liquid", name: "液态玻璃", renderer: "liquid",
    light: { fillOpacity: .86, blur: 10, saturation: 1.5, rim: 2.5 },
    dark: { fillOpacity: .80, blur: 10, saturation: 1.5, rim: 2.5 } }, SOLID_STYLE
];
const arcLight = { ...LIGHT_VARS, "--fc-page-bg": "#f2f5f8", "--fc-surface": "#e8edf3",
  "--fc-text": "#202c3b", "--fc-text-muted": "#536479", "--fc-text-disabled": "#657489",
  "--fc-accent": "#265b9d", "--fc-selection-bg": "rgba(38,91,157,0.13)",
  "--fc-selection-border": "rgba(38,91,157,0.5)", "--fc-focus-ring": "#265b9d",
  "--fc-radius-window": "16px", "--fc-radius-item": "7px", "--fc-radius-control": "7px" };
const arcDark = { ...DARK_VARS, "--fc-page-bg": "#19212b", "--fc-surface": "#222c39",
  "--fc-text": "#edf3fb", "--fc-text-muted": "#a8b7cb", "--fc-text-disabled": "#8797aa",
  "--fc-accent": "#a2c4ff", "--fc-selection-bg": "rgba(162,196,255,0.16)",
  "--fc-selection-border": "rgba(162,196,255,0.55)", "--fc-focus-ring": "#a2c4ff",
  "--fc-radius-window": "16px", "--fc-radius-item": "7px", "--fc-radius-control": "7px" };

/** 三个默认主题：浅色、深色、跟随系统。 */
export const MOCK_THEMES: MockTheme[] = [
  { id: "flashcast.theme.arc", name: "电弧", version: "0.3.0", appearance: "system",
    builtin: true, enabled: true, styles: ARC_STYLES,
    palettes: { light: arcLight, dark: arcDark } },
  {
    id: "flashcast.theme.light",
    name: "浅色",
    version: "0.1.0",
    appearance: "light",
    builtin: true,
    enabled: true,
    palettes: { light: LIGHT_VARS, dark: LIGHT_VARS },
  },
  {
    id: "flashcast.theme.dark",
    name: "深色",
    version: "0.1.0",
    appearance: "dark",
    builtin: true,
    enabled: true,
    palettes: { light: DARK_VARS, dark: DARK_VARS },
  },
  {
    id: "flashcast.theme.system",
    name: "跟随系统",
    version: "0.1.0",
    appearance: "system",
    builtin: true,
    enabled: true,
    palettes: { light: LIGHT_VARS, dark: DARK_VARS },
  },
];

/** 一个已安装的本地主题包（模拟「用户从本地安装」的外观）。 */
export const MOCK_INSTALLED_THEME: MockTheme = {
  id: "example.solarized",
  name: "Solarized",
  version: "3.0.0",
  appearance: "system",
  styles: [{ ...SOLID_STYLE, id: "paper", name: "纸面" }],
  builtin: false,
  enabled: true,
  palettes: {
    light: { ...LIGHT_VARS, "--fc-page-bg": "#fdf6e3", "--fc-surface": "#eee8d5", "--fc-text": "#073642", "--fc-accent": "#765900", "--fc-text-muted": "#52676b" },
    dark: {
      ...DARK_VARS,
      "--fc-page-bg": "#002b36",
      "--fc-surface": "#002b36",
      "--fc-text": "#eee8d5",
      "--fc-text-muted": "#93a1a1",
      "--fc-accent": "#e1b83c",
      "--fc-accent-ink": "#000000",
      "--fc-selection-bg": "rgba(181, 137, 0, 0.28)",
      "--fc-selection-border": "rgba(181, 137, 0, 0.8)",
      "--fc-focus-ring": "rgba(38, 139, 210, 0.95)",
    },
  },
};

function resolvedAppearance(theme: MockTheme, system: Appearance): Appearance {
  if (theme.appearance === "system") return system;
  return theme.appearance;
}

/** 主题在给定系统外观下的 CSS 自定义属性。 */
export function mockThemeCssVars(theme: MockTheme, system: Appearance): CssVar[] {
  const palette = resolvedAppearance(theme, system);
  const vars = palette === "dark" ? theme.palettes.dark : theme.palettes.light;
  return Object.entries(vars).map(([name, value]) => ({ name, value }));
}

/** 组装一份主题状态（与 `Host::theme_state` 的形状一致）。 */
export function buildMockThemeState(input: {
  themes: MockTheme[];
  selected: string;
  system: Appearance;
  error: string | null;
  preference?: ThemeAppearance;
  styles?: Record<string, string>;
  reduce?: boolean;
}): ThemeState {
  const selected =
    input.themes.find((theme) => theme.id === input.selected) ?? input.themes[0];
  const entries: ThemeEntry[] = input.themes.map((theme) => ({
    id: theme.id,
    name: theme.name,
    version: theme.version,
    enabled: theme.enabled,
    selected: theme.id === selected.id,
    builtin: theme.builtin,
    appearance: theme.appearance,
    usable: true,
    error: null,
    legacy: !theme.styles,
    canDisable: theme.id !== "flashcast.theme.arc",
  }));
  const preference = selected.styles ? input.preference ?? "system" : selected.appearance;
  const appearance: Appearance = preference === "system" ? input.system : preference;
  const styles = selected.styles ?? [SOLID_STYLE];
  const style = styles.find(s => s.id === input.styles?.[selected.id]) ?? styles[0];
  const reduce = input.reduce ?? false;
  return {
    selected: selected.id,
    selectedName: selected.name,
    preference,
    appearance,
    systemAppearance: input.system,
    cssVars: Object.entries(selected.palettes[appearance]).map(([name,value]) => ({name,value})),
    styles, style: style.id, renderer: reduce ? "solid" : style.renderer,
    surface: reduce ? SOLID_STYLE.light : style[appearance], reduceTransparency: reduce,
    themes: entries,
    error: input.error,
  };
}
