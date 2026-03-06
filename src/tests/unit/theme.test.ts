import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  applyResolvedTheme,
  applyThemeMode,
  getStoredThemeMode,
  resolveThemeMode,
  subscribeSystemThemeChange,
} from '../../src/lib/theme';

interface MatchMediaMockController {
  trigger: () => void;
}

interface LocalStorageMockController {
  clear: () => void;
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

// mockLocalStorage 负责提供可预测的内存存储实现，避免运行环境下原生 localStorage 不完整导致测试波动。
function mockLocalStorage(): LocalStorageMockController {
  const memoryStore = new Map<string, string>();

  const storage = {
    getItem: (key: string): string | null => memoryStore.get(key) ?? null,
    setItem: (key: string, value: string) => {
      memoryStore.set(key, value);
    },
    removeItem: (key: string) => {
      memoryStore.delete(key);
    },
    clear: () => {
      memoryStore.clear();
    },
  };

  Object.defineProperty(window, 'localStorage', {
    configurable: true,
    writable: true,
    value: storage,
  });

  return storage;
}

// mockMatchMedia 负责模拟系统主题媒体查询，并暴露触发 change 的控制器给测试用例。
function mockMatchMedia(matches: boolean): MatchMediaMockController {
  const listeners = new Set<() => void>();

  const mediaQueryList = {
    matches,
    media: '(prefers-color-scheme: dark)',
    onchange: null,
    addEventListener: vi.fn((event: string, listener: EventListenerOrEventListenerObject) => {
      if (event === 'change') {
        const callback = listener as () => void;
        listeners.add(callback);
      }
    }),
    removeEventListener: vi.fn((event: string, listener: EventListenerOrEventListenerObject) => {
      if (event === 'change') {
        const callback = listener as () => void;
        listeners.delete(callback);
      }
    }),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  };

  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    writable: true,
    value: vi.fn(() => mediaQueryList),
  });

  return {
    trigger: () => {
      listeners.forEach((listener) => listener());
    },
  };
}

describe('theme helpers', () => {
  beforeEach(() => {
    mockLocalStorage();
    window.localStorage.clear();
    document.documentElement.removeAttribute('data-theme');
    document.documentElement.style.colorScheme = '';
    mockMatchMedia(false);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('读取到非法主题配置时回退到 system', () => {
    window.localStorage.setItem('grpcui.theme.mode', 'unexpected-mode');
    expect(getStoredThemeMode()).toBe('system');
  });

  it('读取到合法主题配置时返回对应模式', () => {
    window.localStorage.setItem('grpcui.theme.mode', 'dark');
    expect(getStoredThemeMode()).toBe('dark');
  });

  it('system 模式按系统偏好解析主题', () => {
    expect(resolveThemeMode('system', true)).toBe('dark');
    expect(resolveThemeMode('system', false)).toBe('light');
  });

  it('applyResolvedTheme 会同步 data-theme 与 color-scheme', () => {
    applyResolvedTheme('light');

    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    expect(document.documentElement.style.colorScheme).toBe('light');
  });

  it('applyThemeMode 在 system 模式下会根据系统偏好应用暗色并持久化', () => {
    mockMatchMedia(true);

    const resolvedTheme = applyThemeMode('system');

    expect(resolvedTheme).toBe('dark');
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    expect(window.localStorage.getItem('grpcui.theme.mode')).toBe('system');
  });

  it('subscribeSystemThemeChange 会在系统主题变化时触发回调并支持取消订阅', () => {
    const controller = mockMatchMedia(false);
    const handleThemeChange = vi.fn();

    const unsubscribe = subscribeSystemThemeChange(handleThemeChange);

    controller.trigger();
    expect(handleThemeChange).toHaveBeenCalledTimes(1);

    unsubscribe();
    controller.trigger();
    expect(handleThemeChange).toHaveBeenCalledTimes(1);
  });
});
