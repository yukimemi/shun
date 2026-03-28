/**
 * Shiki-based syntax highlighter
 * - Lazy initialization (first use only)
 * - Common languages pre-loaded at init time
 * - LRU-style cache keyed by "path:theme"
 */

// shun プリセット → Shiki テーマ名。bundledThemes に存在するものだけ使い、ないものは fallback。
const PRESET_TO_SHIKI_THEME = {
  'catppuccin-mocha':  'catppuccin-mocha',
  'catppuccin-latte':  'catppuccin-latte',
  'nord':              'nord',
  'dracula':           'dracula',
  'tokyo-night':       'tokyo-night',
  'one-half-dark':     'one-dark-pro',      // Shiki v4 に one-half-dark がないため代替
  'solarized-dark':    'solarized-dark',
  'solarized-light':   'solarized-light',
};

const FALLBACK_THEME = 'catppuccin-mocha';

export async function shikiTheme(preset) {
  const { bundledThemes } = await import('shiki');
  const candidate = PRESET_TO_SHIKI_THEME[preset] ?? FALLBACK_THEME;
  return candidate in bundledThemes ? candidate : FALLBACK_THEME;
}

const EXT_TO_LANG = {
  js: 'javascript', mjs: 'javascript', cjs: 'javascript',
  ts: 'typescript', mts: 'typescript', cts: 'typescript',
  svelte: 'svelte',
  rs: 'rust',
  py: 'python',
  sh: 'bash', bash: 'bash', zsh: 'bash', fish: 'fish',
  ps1: 'powershell', psm1: 'powershell', psd1: 'powershell',
  json: 'json', jsonc: 'jsonc',
  toml: 'toml',
  yaml: 'yaml', yml: 'yaml',
  md: 'markdown',
  html: 'html', htm: 'html',
  css: 'css', scss: 'scss', less: 'less',
  go: 'go',
  java: 'java',
  c: 'c', h: 'c',
  cpp: 'cpp', cc: 'cpp', cxx: 'cpp', hpp: 'cpp',
  cs: 'csharp',
  fs: 'fsharp', fsi: 'fsharp', fsx: 'fsharp',
  rb: 'ruby',
  php: 'php',
  lua: 'lua',
  kt: 'kotlin', kts: 'kotlin',
  swift: 'swift',
  vim: 'viml', vimrc: 'viml',
  xml: 'xml', plist: 'xml',
  sql: 'sql',
  tf: 'hcl', hcl: 'hcl',
  ini: 'ini', cfg: 'ini', conf: 'ini',
};

// 重複排除した言語リスト
const BUNDLED_LANGS = [...new Set(Object.values(EXT_TO_LANG))];

function extOf(path) {
  const base = path.replace(/\\/g, '/').split('/').pop() ?? '';
  const lower = base.toLowerCase();
  if (lower === 'dockerfile') return 'dockerfile';
  const dot = lower.lastIndexOf('.');
  return dot >= 0 ? lower.slice(dot + 1) : '';
}

// キャッシュ (最大 100 エントリ)
const CACHE_MAX = 100;
const cache = new Map();

function cacheSet(key, html) {
  if (cache.size >= CACHE_MAX) {
    cache.delete(cache.keys().next().value);
  }
  cache.set(key, html);
}

// Shiki highlighter シングルトン（初回プレビュー時に遅延初期化）
let _highlighterPromise = null;

async function getHighlighter() {
  if (!_highlighterPromise) {
    _highlighterPromise = (async () => {
      const { createHighlighter, bundledThemes } = await import('shiki');
      // bundledThemes に存在するテーマのみ渡す
      const validThemes = [...new Set(Object.values(PRESET_TO_SHIKI_THEME))]
        .filter(t => t in bundledThemes);
      return createHighlighter({ themes: validThemes, langs: BUNDLED_LANGS });
    })();
  }
  return _highlighterPromise;
}

/**
 * ファイルをハイライト。
 * - 言語不明 / エラー時は null（呼び出し側でプレーンテキストにフォールバック）
 */
export async function highlight(path, content, theme) {
  const lang = EXT_TO_LANG[extOf(path)];
  if (!lang || !content) return null;

  const cacheKey = `${path}:${theme}`;
  if (cache.has(cacheKey)) return cache.get(cacheKey);

  try {
    const hl = await getHighlighter();
    const html = hl.codeToHtml(content, { lang, theme });
    cacheSet(cacheKey, html);
    return html;
  } catch (e) {
    console.error('[highlight]', e);
    return null;
  }
}
