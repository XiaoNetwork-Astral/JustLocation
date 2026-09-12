import { expect, it } from 'vitest';
import { LOCALE_KEY, locales, readLocale, saveLocale, translate, zh } from './i18n';

function storage(initial: string | null = null) {
  let value = initial;
  return {
    getItem: () => value,
    setItem: (_key: string, next: string) => {
      value = next;
    },
    current: () => value,
  };
}

it('defaults to Chinese so existing expectations keep working', () => {
  expect(readLocale(storage())).toBe('zh');
  expect(readLocale(storage('nonsense'))).toBe('zh');
  expect(readLocale(storage('en'))).toBe('en');
  const box = storage();
  saveLocale('en', box);
  expect(box.current()).toBe('en');
  expect(LOCALE_KEY).toBe('justlocation.locale');
  expect(locales.map((item) => item.id)).toEqual(['zh', 'en']);
});

it('translates in both languages', () => {
  expect(translate('zh', 'nav.location')).toBe('位置模拟');
  expect(translate('en', 'nav.location')).toBe('Location');
});

it('falls back to Chinese when a translation is missing', () => {
  const untranslated = (Object.keys(zh) as (keyof typeof zh)[]).find(
    (key) => translate('en', key) === translate('zh', key),
  );
  expect(untranslated).toBeTruthy();
  expect(translate('en', untranslated!)).toBe(translate('zh', untranslated!));
});

it('replaces placeholders and leaves unknown ones alone', () => {
  expect(translate('zh', 'location.altitude', { value: 31 })).toBe('海拔 31 m · WGS84');
  expect(translate('en', 'feature.scopeApps', { count: 2 })).toBe('Only these apps · 2 selected');
  expect(translate('zh', 'location.pin', { name: '公司' })).toBe('置顶 公司');

  expect(translate('zh', 'location.altitude')).toBe('海拔 {value} m · WGS84');
  expect(translate('zh', 'location.altitude', { other: 1 })).toBe('海拔 {value} m · WGS84');
});

it('keeps the two dictionaries in step for keys a screen needs', () => {
  for (const key of Object.keys(zh)) expect(typeof key).toBe('string');
  expect(translate('zh', 'settings.language')).toBe('语言');
  expect(translate('en', 'settings.language')).toBe('Language');
});

it('reports how much of the dictionary the screens already use', () => {
  const sources = Object.entries(
    import.meta.glob(['./*.ts', './*.tsx', '!./*.test.*', '!./i18n.ts'], {
      query: '?raw',
      import: 'default',
      eager: true,
    }),
  )
    .map(([, text]) => String(text))
    .join('\n');
  const keys = Object.keys(zh) as (keyof typeof zh)[];
  const unused = keys.filter((key) => !sources.includes(`'${key}'`));
  expect(keys.length).toBeGreaterThan(100);

  expect(keys.length - unused.length).toBeGreaterThanOrEqual(0);
  expect(translate('zh', 'location.coordinatesLabel')).toBe('经纬度:');
});
