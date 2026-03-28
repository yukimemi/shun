export const THEMES = {
  "catppuccin-mocha": {
    "--color-bg":      "#1e1e2e",
    "--color-surface": "#313244",
    "--color-overlay": "#45475a",
    "--color-muted":   "#585b70",
    "--color-text":    "#cdd6f4",
    "--color-blue":    "#89b4fa",
    "--color-purple":  "#cba6f7",
    "--color-green":   "#a6e3a1",
    "--color-red":     "#f38ba8",
  },
  "catppuccin-latte": {
    "--color-bg":      "#eff1f5",
    "--color-surface": "#ccd0da",
    "--color-overlay": "#acb0be",
    "--color-muted":   "#9ca0b0",
    "--color-text":    "#4c4f69",
    "--color-blue":    "#1e66f5",
    "--color-purple":  "#8839ef",
    "--color-green":   "#40a02b",
    "--color-red":     "#d20f39",
  },
  "nord": {
    "--color-bg":      "#2e3440",
    "--color-surface": "#3b4252",
    "--color-overlay": "#434c5e",
    "--color-muted":   "#4c566a",
    "--color-text":    "#d8dee9",
    "--color-blue":    "#88c0d0",
    "--color-purple":  "#b48ead",
    "--color-green":   "#a3be8c",
    "--color-red":     "#bf616a",
  },
  "dracula": {
    "--color-bg":      "#282a36",
    "--color-surface": "#44475a",
    "--color-overlay": "#6272a4",
    "--color-muted":   "#6272a4",
    "--color-text":    "#f8f8f2",
    "--color-blue":    "#8be9fd",
    "--color-purple":  "#bd93f9",
    "--color-green":   "#50fa7b",
    "--color-red":     "#ff5555",
  },
  "tokyo-night": {
    "--color-bg":      "#1a1b26",
    "--color-surface": "#24283b",
    "--color-overlay": "#414868",
    "--color-muted":   "#565f89",
    "--color-text":    "#c0caf5",
    "--color-blue":    "#7aa2f7",
    "--color-purple":  "#bb9af7",
    "--color-green":   "#9ece6a",
    "--color-red":     "#f7768e",
  },
  "one-half-dark": {
    "--color-bg":      "#282c34",
    "--color-surface": "#3b4048",
    "--color-overlay": "#4b5263",
    "--color-muted":   "#5c6370",
    "--color-text":    "#dcdfe4",
    "--color-blue":    "#61afef",
    "--color-purple":  "#c678dd",
    "--color-green":   "#98c379",
    "--color-red":     "#e06c75",
  },
  "solarized-dark": {
    "--color-bg":      "#002b36",
    "--color-surface": "#073642",
    "--color-overlay": "#586e75",
    "--color-muted":   "#657b83",
    "--color-text":    "#839496",
    "--color-blue":    "#268bd2",
    "--color-purple":  "#6c71c4",
    "--color-green":   "#859900",
    "--color-red":     "#dc322f",
  },
  "solarized-light": {
    "--color-bg":      "#fdf6e3",
    "--color-surface": "#eee8d5",
    "--color-overlay": "#93a1a1",
    "--color-muted":   "#839496",
    "--color-text":    "#657b83",
    "--color-blue":    "#268bd2",
    "--color-purple":  "#6c71c4",
    "--color-green":   "#859900",
    "--color-red":     "#dc322f",
  },
};

export const THEME_PRESETS = Object.keys(THEMES);

const COLOR_MAP = {
  bg: "--color-bg", surface: "--color-surface", overlay: "--color-overlay",
  muted: "--color-muted", text: "--color-text", blue: "--color-blue",
  purple: "--color-purple", green: "--color-green", red: "--color-red",
};

/**
 * Applies theme CSS variables to document.documentElement.
 * Returns the resolved preset name.
 */
export function applyThemeCssVars(themeConfig) {
  const preset = themeConfig?.preset || "catppuccin-mocha";
  const base = THEMES[preset] ?? THEMES["catppuccin-mocha"];
  const root = document.documentElement;
  for (const [key, val] of Object.entries(base)) {
    root.style.setProperty(key, val);
  }
  for (const [field, cssVar] of Object.entries(COLOR_MAP)) {
    if (themeConfig?.[field]) root.style.setProperty(cssVar, themeConfig[field]);
  }
  return preset;
}
