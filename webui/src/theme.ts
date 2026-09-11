// 主题分两层，互相独立：
//   style = 'material' | 'miuix'  外观语言（圆角、层级、控件形态）
//   mode  = 'system' | 'light' | 'dark'  明暗
// 两者都写到 <html> 的 data 属性上，具体取值全部由 CSS 令牌承担。
// 变量命名沿用 KernelSU 管理器的 Miuix→CSS 桥接（MonetColorsProvider.kt），
// 这样 Miuix 一侧的语义与真实实现一致，Material 一侧只换值不换名字。

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

/** 读取已保存的外观语言。旧版本没有这个键，按 Material 处理。 */
export function readStyle(storage: Pick<Storage, 'getItem'> = localStorage): StyleFamily {
  try {
    const saved = storage.getItem(STYLE_STORAGE_KEY);
    return isStyleFamily(saved) ? saved : 'material';
  } catch {
    return 'material';
  }
}

/** 读取已保存的明暗模式。旧版本用 'system' | 'light' | 'dark'，语义一致。 */
export function readColorMode(storage: Pick<Storage, 'getItem'> = localStorage): ColorMode {
  try {
    const saved = storage.getItem(MODE_STORAGE_KEY);
    return isColorMode(saved) ? saved : 'system';
  } catch {
    return 'system';
  }
}

/** 把两层主题写到根元素上，并返回清理函数。 */
export function applyTheme(style: StyleFamily, mode: ColorMode, root: HTMLElement = document.documentElement) {
  root.dataset.style = style;
  root.dataset.theme = mode;
}

export function saveTheme(style: StyleFamily, mode: ColorMode, storage: Pick<Storage, 'setItem'> = localStorage) {
  try {
    storage.setItem(STYLE_STORAGE_KEY, style);
    storage.setItem(MODE_STORAGE_KEY, mode);
  } catch {
    /* 存储不可用时仅本次会话生效。 */
  }
}
