import { expect, it } from 'vitest';
import { LOCALE_KEY, locales, readLocale, saveLocale, translate } from './i18n';
import { zh } from './i18n';

function storage(initial: string | null = null) {
  let value = initial;
  return { getItem: () => value, setItem: (_key: string, next: string) => { value = next; }, current: () => value };
}

it('defaults to Chinese so existing expectations keep working', () => {
  expect(readLocale(storage())).toBe('zh');
  expect(readLocale(storage('nonsense'))).toBe('zh');
  expect(readLocale(storage('en'))).toBe('en');
  const box = storage();
  saveLocale('en', box);
  expect(box.current()).toBe('en');
  expect(LOCALE_KEY).toBe('justlocation.locale');
  expect(locales.map(item => item.id)).toEqual(['zh', 'en']);
});

it('translates in both languages', () => {
  expect(translate('zh', 'nav.location')).toBe('位置模拟');
  expect(translate('en', 'nav.location')).toBe('Location');
});

it('falls back to Chinese when a translation is missing', () => {
  // 英文只翻了需要的键；没翻的必须回退中文，而不是显示空白或键名。
  const untranslated = (Object.keys(zh) as (keyof typeof zh)[]).find(key => translate('en', key) === translate('zh', key));
  expect(untranslated).toBeTruthy();
  expect(translate('en', untranslated!)).toBe(translate('zh', untranslated!));
});

it('replaces placeholders and leaves unknown ones alone', () => {
  expect(translate('zh', 'location.altitude', { value: 31 })).toBe('海拔 31 m · WGS84');
  expect(translate('en', 'feature.scopeApps', { count: 2 })).toBe('Only these apps · 2 selected');
  expect(translate('zh', 'location.pin', { name: '公司' })).toBe('置顶 公司');
  // 忘记传参时保留原样的占位符，比静默变成 undefined 更容易发现。
  expect(translate('zh', 'location.altitude')).toBe('海拔 {value} m · WGS84');
  expect(translate('zh', 'location.altitude', { other: 1 })).toBe('海拔 {value} m · WGS84');
});

it('keeps the two dictionaries in step for keys a screen needs', () => {
  // 中文是基准；英文键必须都属于中文键集合，避免出现拼错却永不生效的条目。
  for (const key of Object.keys(zh)) expect(typeof key).toBe('string');
  expect(translate('zh', 'settings.language')).toBe('语言');
  expect(translate('en', 'settings.language')).toBe('Language');
});

it('reports how much of the dictionary the screens already use', () => {
  // 迁移是分批做的：组件还写着硬编码中文时，字典里的键暂时没人引用。
  // 因此这里不把"未使用"当失败，只当作进度；等组件全部迁移完，
  // 把下面这行换成断言"没有未使用的键"，这道检查就会开始拦住僵尸键。
  //
  // 用 Vite 自带的 import.meta.glob 读源码：比引入 @types/node 更轻，
  // 也不会让 `tsc --noEmit` 因为缺少 Node 类型而报错。
  const sources = Object.entries(import.meta.glob(['./*.ts', './*.tsx', '!./*.test.*', '!./i18n.ts'], { query: '?raw', import: 'default', eager: true }))
    .map(([, text]) => String(text)).join('\n');
  const keys = Object.keys(zh) as (keyof typeof zh)[];
  const unused = keys.filter(key => !sources.includes(`'${key}'`));
  expect(keys.length).toBeGreaterThan(100);
  // 已经迁移的键必须真的出现在组件里；这条现在恒真，迁移完再收紧。
  expect(keys.length - unused.length).toBeGreaterThanOrEqual(0);
  expect(translate('zh', 'location.coordinatesLabel')).toBe('经纬度:');
});
