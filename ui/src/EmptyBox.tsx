type Props = {
  text?: string;

  hint?: string;
};

export function EmptyBox({ text = '点击右下角 "+" 按钮进行添加', hint }: Props) {
  return (
    <div className="empty-box">
      <svg
        className="empty-art"
        viewBox="0 0 96 96"
        width="80"
        height="80"
        aria-hidden="true"
        focusable="false"
      >
        <g
          fill="none"
          stroke="currentColor"
          strokeWidth="2.2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M28 58h40v18a3 3 0 0 1-3 3H31a3 3 0 0 1-3-3V58z" />
          <path d="M28 58L18 50l7-9 11 6" />
          <path d="M68 58l10-8-7-9-11 6" />

          <path d="M36 46c8-12 18-20 30-24" strokeWidth="1.5" strokeDasharray="3 5" />

          <path d="M78 20L48 34l12 4 4 12z" />
          <path d="M60 38l18-18" strokeWidth="1.5" />
        </g>
        <g fill="currentColor">
          <circle cx="20" cy="32" r="1.6" />
          <circle cx="30" cy="18" r="1.1" />
          <circle cx="84" cy="44" r="1.4" />
          <circle cx="70" cy="70" r="1.2" />
        </g>
      </svg>
      <p className="empty-text">{text}</p>
      {hint && <p className="empty-hint">{hint}</p>}
    </div>
  );
}
