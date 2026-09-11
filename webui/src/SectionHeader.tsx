import type { ReactNode } from 'react';

type Props = {
  /** 区块标题，例如「历史位置」 */
  title: string;
  /** 标题后的计数；原版这里是纯文字，这里只保留一个轻量计数 */
  count?: number;
  /** 右侧入口的点击行为；不传则只显示标题（没有箭头） */
  onAction?(): void;
  /** 右侧入口的无障碍名称，默认「查看全部」 */
  actionLabel?: string;
  /** 自定义右侧内容，用于放与箭头不同的入口 */
  action?: ReactNode;
};

/**
 * 区块头（原版 `ci.xml` 模板里的 [B] 段）。
 *
 * 原版结构是：灰色 14dp 标题 + 弹簧 View + 32dp 箭头 ImageView（`#no`，
 * tint `@color/l7` = #a1a1a1）。箭头表示"进入列表/查看全部"，
 * 中文原文取自 `values-zh-rCN` 的 `@string/f5`「查看全部」。
 *
 * 我们用按钮而不是 ImageView：这里真的可以点，点了会滚到列表并聚焦搜索框，
 * 所以在保持原版形态的同时给它一个真实行为。
 */
export function SectionHeader({ title, count, onAction, actionLabel = '查看全部', action }: Props) {
  return <div className="section-header">
    <h2>{title}{count !== undefined && <span className="section-count">{count}</span>}</h2>
    <span className="spacer" />
    {action ?? (onAction && <button type="button" className="section-arrow" aria-label={actionLabel} title={actionLabel} onClick={onAction}>
      <svg viewBox="0 0 24 24" width="32" height="32" aria-hidden="true" focusable="false">
        <path d="M9 6l6 6-6 6" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </button>)}
  </div>;
}
