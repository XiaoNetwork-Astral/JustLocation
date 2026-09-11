type Props = {
  /** 主提示，默认取自原版 `h0.xml` 的「点击右上角 "+" 按钮进行添加」 */
  text?: string;
  /** 补充说明；原版没有这一行，留给"搜不到结果"这类需要多解释一句的情况 */
  hint?: string;
};

/**
 * 空状态（原版 `h0.xml`）。
 *
 * 原版是 80dp 的单色插画（`@mipmap/a0`，tint `@color/l7` = #a1a1a1）
 * 加一行「点击右上角 "+" 按钮进行添加」。原版那张位图画的是：开盖的盒子、
 * 从盒中飞出的纸飞机、一条虚线轨迹和周围几个星点。这里用内联 SVG 画同样的东西：
 * 位图进不了我们的构建，而矢量还能跟着主题色走。
 *
 * 文案里的"右上角"指原版位于右上角的 FAB；我们的 FAB 在右下角（见 .fab-stack），
 * 所以默认文案用"右下角"，避免指错位置。
 */
export function EmptyBox({ text = '点击右下角 "+" 按钮进行添加', hint }: Props) {
  return <div className="empty-box">
    <svg className="empty-art" viewBox="0 0 96 96" width="80" height="80" aria-hidden="true" focusable="false">
      <g fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
        {/* 盒子：盒身 + 两片向外翻开的盒盖 */}
        <path d="M28 58h40v18a3 3 0 0 1-3 3H31a3 3 0 0 1-3-3V58z" />
        <path d="M28 58L18 50l7-9 11 6" />
        <path d="M68 58l10-8-7-9-11 6" />
        {/* 虚线飞行轨迹，从盒口斜向飞出 */}
        <path d="M36 46c8-12 18-20 30-24" strokeWidth="1.5" strokeDasharray="3 5" />
        {/* 纸飞机：一个箭头形机身 + 一道折痕 */}
        <path d="M78 20L48 34l12 4 4 12z" />
        <path d="M60 38l18-18" strokeWidth="1.5" />
      </g>
      <g fill="currentColor">
        {/* 星点 */}
        <circle cx="20" cy="32" r="1.6" />
        <circle cx="30" cy="18" r="1.1" />
        <circle cx="84" cy="44" r="1.4" />
        <circle cx="70" cy="70" r="1.2" />
      </g>
    </svg>
    <p className="empty-text">{text}</p>
    {hint && <p className="empty-hint">{hint}</p>}
  </div>;
}
