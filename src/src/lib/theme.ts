export type ThemeMode = 'system' | 'dark' | 'light';
export type ResolvedTheme = 'dark' | 'light';

const THEME_STORAGE_KEY = 'grpcui.theme.mode';
const SYSTEM_THEME_MEDIA_QUERY = '(prefers-color-scheme: dark)';
const THEME_DATA_ATTRIBUTE = 'data-theme';

/**
 * isThemeMode 用于校验本地存储读取的字符串是否为受支持的主题模式。
 * 通过显式类型守卫，确保后续主题流程只处理 system/dark/light 三种合法值。
 */
function isThemeMode(value: string | null): value is ThemeMode {
  return value === 'system' || value === 'dark' || value === 'light';
}

/**
 * getStoredThemeMode 从 localStorage 读取用户上次选择的主题模式。
 * 若浏览器不支持 localStorage 或读取到非法值，则回退到 system，保持行为可预期。
 */
export function getStoredThemeMode(): ThemeMode {
  if (typeof window === 'undefined') {
    return 'system';
  }

  try {
    const storedThemeMode = window.localStorage.getItem(THEME_STORAGE_KEY);
    return isThemeMode(storedThemeMode) ? storedThemeMode : 'system';
  } catch {
    return 'system';
  }
}

/**
 * getSystemPrefersDark 基于媒体查询读取当前系统主题偏好。
 * 在不支持 matchMedia 的运行环境中统一返回 false，避免抛错影响页面初始化。
 */
export function getSystemPrefersDark(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return false;
  }

  return window.matchMedia(SYSTEM_THEME_MEDIA_QUERY).matches;
}

/**
 * resolveThemeMode 将“用户选择模式”解析为最终可渲染主题（dark/light）。
 * 当模式为 system 时，使用系统偏好；当模式为显式 dark/light 时直接返回对应值。
 */
export function resolveThemeMode(mode: ThemeMode, systemPrefersDark: boolean): ResolvedTheme {
  if (mode === 'system') {
    return systemPrefersDark ? 'dark' : 'light';
  }

  return mode;
}

/**
 * applyResolvedTheme 把最终主题写入 html 根节点，并同步 color-scheme。
 * 这样浏览器原生控件（滚动条、输入框等）也能与页面主题保持一致。
 */
export function applyResolvedTheme(theme: ResolvedTheme): void {
  if (typeof document === 'undefined') {
    return;
  }

  const htmlElement = document.documentElement;
  htmlElement.setAttribute(THEME_DATA_ATTRIBUTE, theme);
  htmlElement.style.colorScheme = theme;
}

/**
 * applyThemeMode 统一执行主题应用流程：解析最终主题、更新 DOM、持久化用户选择。
 * 返回解析后的主题值，便于调用方在需要时进行后续逻辑判断。
 */
export function applyThemeMode(mode: ThemeMode): ResolvedTheme {
  const resolvedTheme = resolveThemeMode(mode, getSystemPrefersDark());
  applyResolvedTheme(resolvedTheme);

  if (typeof window !== 'undefined') {
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, mode);
    } catch {
      // ignore localStorage write failures
    }
  }

  return resolvedTheme;
}

/**
 * initializeTheme 在应用启动时恢复主题设置并立即应用。
 * 提前执行可减少首次渲染闪烁，同时返回恢复出的主题模式供上层状态初始化复用。
 */
export function initializeTheme(): ThemeMode {
  const themeMode = getStoredThemeMode();
  const resolvedTheme = resolveThemeMode(themeMode, getSystemPrefersDark());
  applyResolvedTheme(resolvedTheme);
  return themeMode;
}

/**
 * subscribeSystemThemeChange 监听系统主题变化事件。
 * 仅在 system 模式下需要订阅，返回清理函数以避免组件卸载后残留监听器。
 */
export function subscribeSystemThemeChange(onChange: () => void): () => void {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return () => {};
  }

  const mediaQueryList = window.matchMedia(SYSTEM_THEME_MEDIA_QUERY);
  const handleChange = () => {
    onChange();
  };

  if (typeof mediaQueryList.addEventListener === 'function') {
    mediaQueryList.addEventListener('change', handleChange);
    return () => {
      mediaQueryList.removeEventListener('change', handleChange);
    };
  }

  mediaQueryList.addListener(handleChange);
  return () => {
    mediaQueryList.removeListener(handleChange);
  };
}
