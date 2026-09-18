import { useMemo } from "react";
import DOMPurify from "dompurify";
import { Marked, type Tokens } from "marked";

/**
 * AI result body renderer. Block markdown (headings, paragraphs, quotes,
 * lists, fenced code) plus inline code/emphasis render as HTML; everything
 * that could navigate or execute renders as literal text, exactly like the
 * previous hand-rolled renderer:
 *
 * - links and images stay literal (`[text](url)`), so tapping a result can
 *   never navigate the popup webview away;
 * - raw HTML stays literal (`<b>` shows as text), so model output can never
 *   inject elements;
 * - headings demote one level (`#` renders as `h2`) because the result card
 *   already owns the `h1`.
 *
 * Parsing is `marked`; safety is layered: overrides neutralize the dangerous
 * token kinds before serialization, then DOMPurify sanitizes the final HTML
 * as defense-in-depth against parser bugs or future renderer changes.
 */
const kivoMarked = new Marked({
  // Single newlines stay soft breaks inside a paragraph (matches the old
  // paragraph joining); fenced code, tables, and strikethrough stay on.
  breaks: false,
  gfm: true,
});
kivoMarked.use({
  renderer: {
    heading({ tokens, depth }: Tokens.Heading): string {
      // The result card owns the h1, so body headings start at h2.
      let tag = "h4";
      if (depth <= 1) tag = "h2";
      else if (depth === 2) tag = "h3";
      return `<${tag}>${this.parser.parseInline(tokens)}</${tag}>`;
    },
    link(token: Tokens.Link): string {
      return escapeHtml(token.raw);
    },
    image(token: Tokens.Image): string {
      return escapeHtml(token.raw);
    },
    html(token: Tokens.HTML | Tokens.Tag): string {
      return escapeHtml(token.raw);
    },
  },
});

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

/** Markdown source to sanitized HTML. Exported for unit tests. */
export function renderMarkdown(markdown: string): string {
  const parsed = kivoMarked.parse(markdown, { async: false });
  return DOMPurify.sanitize(parsed);
}

export function SafeMarkdown({ children }: { readonly children: string }) {
  const html = useMemo(() => renderMarkdown(children), [children]);
  return <div className="markdown-result" dangerouslySetInnerHTML={{ __html: html }} />;
}
