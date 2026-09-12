export type StyleFamily = 'material' | 'miuix';
export type ColorMode = 'system' | 'light' | 'dark';

export const STYLE_STORAGE_KEY = 'justlocation.style';
export const MODE_STORAGE_KEY = 'justlocation.theme';

export const styleFamilies = [
  { id: 'material', label: 'Material 3', hint: '圆角卡片、分组列表，标准安卓观感' },
  { id: 'miuix', label: '米 UI', hint: '大圆角、整行开关、扁平无重阴影' },
] as const satisfies readonly { id: StyleFamily; label: string; hint: string }[];

export const colorModes = [
  { id: 'system', label: '跟随系统' },
  { id: 'light', label: '浅色' },
  { id: 'dark', label: '深色' },
] as const satisfies readonly { id: ColorMode; label: string }[];

export function isStyleFamily(value: unknown): value is StyleFamily {
  return value === 'material' || value === 'miuix';
}

export function isColorMode(value: unknown): value is ColorMode {
  return value === 'system' || value === 'light' || value === 'dark';
}

export function readStyle(storage: Pick<Storage, 'getItem'> = localStorage): StyleFamily {
  try {
    const saved = storage.getItem(STYLE_STORAGE_KEY);
    return isStyleFamily(saved) ? saved : 'material';
  } catch {
    return 'material';
  }
}

export function readColorMode(storage: Pick<Storage, 'getItem'> = localStorage): ColorMode {
  try {
    const saved = storage.getItem(MODE_STORAGE_KEY);
    return isColorMode(saved) ? saved : 'system';
  } catch {
    return 'system';
  }
}

export function applyTheme(
  style: StyleFamily,
  mode: ColorMode,
  root: HTMLElement = document.documentElement,
) {
  root.dataset.style = style;
  root.dataset.theme = mode;
}

export function saveTheme(
  style: StyleFamily,
  mode: ColorMode,
  storage: Pick<Storage, 'setItem'> = localStorage,
) {
  try {
    storage.setItem(STYLE_STORAGE_KEY, style);
    storage.setItem(MODE_STORAGE_KEY, mode);
  } catch {
    /* Keep the current session usable when browser storage is unavailable. */
  }
}
