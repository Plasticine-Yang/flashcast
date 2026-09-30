// 浏览器模拟宿主的主题数据。
//
// 真实的主题解析在 `flashcast-core::theme`：声明式 JSON → 语义 token → CSS 自定义
// 属性。这里只复刻 UI 需要的那一层（CSS 变量与列表语义），供 `pnpm dev` 与
// `pnpm ui-check` 使用。**模拟宿主不是产品行为**，真实行为由
// `crates/flashcast-core/tests/themes.rs` 的集成测试覆盖。
//
// 变量名称与 `ThemeTokens::css_vars` 一一对应；`pnpm ui-check` 会断言这份映射没有
// 缺口（数量与必需变量名都要对得上）。

import type { Appearance, CssVar, ThemeAppearance, ThemeEntry, ThemeState } from "./types";

/** 浏览器模拟宿主认得的主题。 */
export interface MockTheme {
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
  "--fc-page-bg": "rgba(250, 250, 251, 0.98)",
  "--fc-surface": "#ffffff",
  "--fc-hover-bg": "rgba(15, 23, 42, 0.04)",
  "--fc-border": "rgba(15, 23, 42, 0.12)",
  "--fc-border-strong": "rgba(15, 23, 42, 0.24)",
  "--fc-text": "#16181d",
  "--fc-text-muted": "#6b7280",
  "--fc-text-disabled": "#8b919c",
  "--fc-accent": "#2563eb",
  "--fc-info-bg": "rgba(15, 23, 42, 0.05)",
  "--fc-warning-bg": "rgba(217, 119, 6, 0.12)",
  "--fc-warning-border": "rgba(217, 119, 6, 0.45)",
  "--fc-warning-text": "#92400e",
  "--fc-icon-fallback-bg": "rgba(15, 23, 42, 0.08)",
  "--fc-selection-bg": "rgba(37, 99, 235, 0.12)",
  "--fc-selection-border": "rgba(37, 99, 235, 0.55)",
  "--fc-focus-ring": "rgba(37, 99, 235, 0.65)",
  "--fc-error-bg": "rgba(220, 38, 38, 0.1)",
  "--fc-error-border": "rgba(220, 38, 38, 0.5)",
  "--fc-error-text": "#991b1b",
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
  "--fc-shadow": "0 8px 28px rgba(15, 23, 42, 0.16)",
  "--fc-shadow-overlay": "0 12px 32px rgba(15, 23, 42, 0.22)",
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

/** 三个默认主题：浅色、深色、跟随系统。 */
export const MOCK_THEMES: MockTheme[] = [
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
  name: "Solarized 深色",
  version: "2.1.0",
  appearance: "dark",
  builtin: false,
  enabled: true,
  palettes: {
    light: { ...LIGHT_VARS, "--fc-surface": "#fdf6e3", "--fc-text": "#073642" },
    dark: {
      ...DARK_VARS,
      "--fc-page-bg": "rgba(0, 43, 54, 0.98)",
      "--fc-surface": "#002b36",
      "--fc-text": "#eee8d5",
      "--fc-text-muted": "#93a1a1",
      "--fc-accent": "#b58900",
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
  }));
  return {
    selected: selected.id,
    selectedName: selected.name,
    preference: selected.appearance,
    appearance: resolvedAppearance(selected, input.system),
    systemAppearance: input.system,
    cssVars: mockThemeCssVars(selected, input.system),
    themes: entries,
    error: input.error,
  };
}
