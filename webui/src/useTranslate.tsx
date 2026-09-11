import { createContext, useContext, type ReactNode } from 'react';
import { translate, type Locale, type MessageKey } from './i18n';

/** 语言放在 Context 里，组件只取 `t`，不各自读存储。默认中文，未包 Provider 时也能工作。 */
const LocaleContext = createContext<Locale>('zh');

export function LocaleProvider({ locale, children }: { locale: Locale; children: ReactNode }) {
  return <LocaleContext.Provider value={locale}>{children}</LocaleContext.Provider>;
}

export type Translate = (key: MessageKey, params?: Record<string, string | number>) => string;

export function useTranslate(): Translate {
  const locale = useContext(LocaleContext);
  return (key, params) => translate(locale, key, params);
}
