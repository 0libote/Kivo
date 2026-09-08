import { Fragment, type ReactNode } from "react";

function inlineMarkdown(text: string): ReactNode[] {
  const tokens = text.split(/(`[^`]+`|\*\*[^*]+\*\*|_[^_]+_)/g).filter(Boolean);
  return tokens.map((token, index) => {
    if (token.startsWith("`") && token.endsWith("`")) return <code key={index}>{token.slice(1, -1)}</code>;
    if (token.startsWith("**") && token.endsWith("**")) return <strong key={index}>{token.slice(2, -2)}</strong>;
    if (token.startsWith("_") && token.endsWith("_")) return <em key={index}>{token.slice(1, -1)}</em>;
    return <Fragment key={index}>{token}</Fragment>;
  });
}

type Block =
  | { kind: "heading"; level: number; text: string }
  | { kind: "paragraph"; text: string }
  | { kind: "quote"; text: string }
  | { kind: "list"; ordered: boolean; items: string[] }
  | { kind: "code"; text: string };

export function parseMarkdown(markdown: string): Block[] {
  const lines = markdown.replace(/\r\n/g, "\n").split("\n");
  const blocks: Block[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];
    if (!line.trim()) {
      index += 1;
      continue;
    }
    if (line.startsWith("```")) {
      const code: string[] = [];
      index += 1;
      while (index < lines.length && !lines[index].startsWith("```")) code.push(lines[index++]);
      index += 1;
      blocks.push({ kind: "code", text: code.join("\n") });
      continue;
    }
    const heading = /^(#{1,3})\s+(.+)$/.exec(line);
    if (heading) {
      blocks.push({ kind: "heading", level: heading[1].length, text: heading[2] });
      index += 1;
      continue;
    }
    if (line.startsWith("> ")) {
      blocks.push({ kind: "quote", text: line.slice(2) });
      index += 1;
      continue;
    }
    const unordered = /^[-*]\s+(.+)$/.exec(line);
    const ordered = /^\d+\.\s+(.+)$/.exec(line);
    if (unordered || ordered) {
      const orderedList = Boolean(ordered);
      const items: string[] = [];
      while (index < lines.length) {
        const match = orderedList ? /^\d+\.\s+(.+)$/.exec(lines[index]) : /^[-*]\s+(.+)$/.exec(lines[index]);
        if (!match) break;
        items.push(match[1]);
        index += 1;
      }
      blocks.push({ kind: "list", ordered: orderedList, items });
      continue;
    }
    const paragraph = [line];
    index += 1;
    while (index < lines.length && lines[index].trim() && !/^(#{1,3})\s|^[-*]\s|^\d+\.\s|^>\s|^```/.test(lines[index])) {
      paragraph.push(lines[index++]);
    }
    blocks.push({ kind: "paragraph", text: paragraph.join(" ") });
  }
  return blocks;
}

export function SafeMarkdown({ children }: { children: string }) {
  return (
    <div className="markdown-result">
      {parseMarkdown(children).map((block, index) => {
        switch (block.kind) {
          case "heading": {
            const Heading = block.level === 1 ? "h2" : block.level === 2 ? "h3" : "h4";
            return <Heading key={index}>{inlineMarkdown(block.text)}</Heading>;
          }
          case "paragraph": return <p key={index}>{inlineMarkdown(block.text)}</p>;
          case "quote": return <blockquote key={index}>{inlineMarkdown(block.text)}</blockquote>;
          case "code": return <pre key={index}><code>{block.text}</code></pre>;
          case "list": {
            const List = block.ordered ? "ol" : "ul";
            return <List key={index}>{block.items.map((item, itemIndex) => <li key={itemIndex}>{inlineMarkdown(item)}</li>)}</List>;
          }
        }
      })}
    </div>
  );
}
