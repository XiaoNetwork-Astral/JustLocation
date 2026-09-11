import type { ReactNode } from 'react';

type Props = {
  /** 行首图标；放在 40×40 的绿色圆底里 */
  icon: ReactNode;
  /** 主文字，例如位置名称 */
  title: string;
  /** 次要行，例如坐标 */
  detail?: string;
  /** 点击整行；不传时这一行不可点（例如正在忙） */
  onClick?(): void;
  /** 行尾操作，例如置顶 / 编辑 / 删除 */
  actions?: ReactNode;
  /** 无障碍名称，默认用主文字 */
  label?: string;
};

/**
 * 列表行（原版 `cv.xml`）。
 *
 * 原版结构：40×40dp 的绿色圆形底（`@color/ix`）+ 图标 + 名称（bold 16dp）
 * + 下一行「经纬度:」+ 值，右侧留一个默认 GONE 的 24dp ImageView。
 *
 * 这里把主次两行做成可点区域，行尾留给我们的置顶/编辑/删除三个按钮——
 * 原版没有这三个操作，属于我们自己的功能，保持在同一行的右侧而不是收进菜单，
 * 免得把常用操作藏起来。
 */
export function RowCard({ icon, title, detail, onClick, actions, label }: Props) {
  const content = <>
    <span className="row-symbol">{icon}</span>
    <span className="row-text-block">
      <strong>{title}</strong>
      {detail && <small>{detail}</small>}
    </span>
  </>;
  return <div className="row-card">
    {onClick
      ? <button type="button" className="row-card-main" aria-label={label ?? title} onClick={onClick}>{content}</button>
      : <div className="row-card-main" aria-label={label ?? title}>{content}</div>}
    {actions && <span className="row-card-actions">{actions}</span>}
  </div>;
}
