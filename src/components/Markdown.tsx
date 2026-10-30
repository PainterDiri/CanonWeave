import MarkdownToJsx from "markdown-to-jsx";

/**
 * 统一的 Markdown 渲染：Agent 回答、人物与知识条目等长文本都用它。
 * 不解析原始 HTML，链接不跳出应用；样式见 styles.css 的 .md。
 */
export function Markdown({ children, className = "" }: { children: string; className?: string }) {
  return (
    <div className={`md ${className}`}>
      <MarkdownToJsx
        options={{
          disableParsingRawHTML: true,
          forceBlock: true,
          overrides: {
            a: {
              component: ({ children: label }: { children: React.ReactNode }) => (
                <span className="md-link">{label}</span>
              ),
            },
            h1: { component: "h3" },
            h2: { component: "h3" },
          },
        }}
      >
        {children ?? ""}
      </MarkdownToJsx>
    </div>
  );
}
