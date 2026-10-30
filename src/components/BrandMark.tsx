/** 品牌标记：一片叶子，叶柄处被一根浅绿丝线绕过并打结。 */
export function BrandMark({ size = 28 }: { size?: number }) {
  const glyph = Math.round(size * 0.66);
  return (
    <div
      className="flex shrink-0 items-center justify-center rounded-[30%] bg-teal shadow-[0_2px_6px_rgba(31,68,48,0.28)]"
      style={{ width: size, height: size }}
    >
      <svg width={glyph} height={glyph} viewBox="0 0 24 24" fill="none" aria-hidden>
        <path d="M6.2 18.4C5.6 10.6 10.4 5.2 19.2 4.6c.4 8.6-4.6 13.6-13 13.8Z" fill="#E0ECD9" />
        <path d="M6.2 18.4C9.4 14 12.6 10.6 16.6 7.4" stroke="#2F5E3F" strokeWidth="1.2" strokeLinecap="round" />
        <path
          d="M2.6 15.2c2.4.4 3.8 1.6 4.4 3.4.5 1.6-.8 2.6-2 1.8-1.4-.9.2-3 2.6-3.6 2.6-.6 5.6.2 8.6 2.4"
          stroke="#9CC79A"
          strokeWidth="1.2"
          strokeLinecap="round"
        />
      </svg>
    </div>
  );
}
