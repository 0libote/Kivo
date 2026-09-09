import { Fragment, type ReactNode } from "react";

function inlineMarkdown(text: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  let key = 0;
  let plain = "";
  const flushPlain = () => {
    if (plain) {
      nodes.push(<Fragment key={`text-${key++}-${plain.length}`}>{plain}</Fragment>);
      plain = "";
    }
  };

  let i = 0;
  while (i < text.length) {
    const codeEnd = matchCodeSpan(text, i);
    if (codeEnd !== null) {
      flushPlain();
      nodes.push(<code key={`code-${key++}`}>{text.slice(i + 1, codeEnd)}</code>);
      i = codeEnd + 1;
      continue;
    }
    const strongEnd = matchDelimitedRun(text, i, "**");
    if (strongEnd !== null) {
      flushPlain();
      nodes.push(<strong key={`strong-${key++}`}>{text.slice(i + 2, strongEnd)}</strong>);
      i = strongEnd + 2;
      continue;
    }
    const emEnd = matchDelimitedRun(text, i, "_");
    if (emEnd !== null) {
      flushPlain();
      nodes.push(<em key={`em-${key++}`}>{text.slice(i + 1, emEnd)}</em>);
      i = emEnd + 1;
      continue;
    }
    plain += text[i];
    i += 1;
  }
  flushPlain();
  return nodes;
}

function matchCodeSpan(text: string, start: number): number | null {
  if (text[start] !== "`") return null;
  const end = text.indexOf("`", start + 1);
  if (end > start + 1) return end;
  return null;
}

function matchDelimitedRun(text: string, start: number, delimiter: string): number | null {
  if (!text.startsWith(delimiter, start)) return null;
  const contentStart = start + delimiter.length;
  const end = text.indexOf(delimiter, contentStart);
  if (end > contentStart) return end;
  return null;
}

type Block =
  | { kind: "heading"; level: number; text: string }
  | { kind: "paragraph"; text: string }
  | { kind: "quote"; text: string }
  | { kind: "list"; ordered: boolean; items: string[] }
  | { kind: "code"; text: string };

export function parseMarkdown(markdown: string): Block[] {
  const lines = markdown.replaceAll("\r\n", "\n").split("\n");
  const blocks: Block[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];
    if (!line.trim()) {
      index += 1;
      continue;
    }
    const codeBlock = tryParseCodeBlock(lines, index);
    if (codeBlock) {
      blocks.push(codeBlock.block);
      index = codeBlock.nextIndex;
      continue;
    }
    const heading = tryParseHeading(line);
    if (heading) {
      blocks.push(heading);
      index += 1;
      continue;
    }
    const quote = tryParseQuote(line);
    if (quote) {
      blocks.push(quote);
      index += 1;
      continue;
    }
    const list = tryParseList(lines, index);
    if (list) {
      blocks.push(list.block);
      index = list.nextIndex;
      continue;
    }
    const paragraph = parseParagraph(lines, index);
    blocks.push(paragraph.block);
    index = paragraph.nextIndex;
  }
  return blocks;
}

function tryParseCodeBlock(lines: string[], start: number): { block: Block; nextIndex: number } | null {
  if (!lines[start].startsWith("```")) return null;
  const code: string[] = [];
  let index = start + 1;
  while (index < lines.length && !lines[index].startsWith("```")) {
    code.push(lines[index]);
    index += 1;
  }
  return { block: { kind: "code", text: code.join("\n") }, nextIndex: index + 1 };
}

function tryParseHeading(line: string): Block | null {
  for (const level of [3, 2, 1]) {
    const prefix = "#".repeat(level) + " ";
    if (line.startsWith(prefix)) {
      return { kind: "heading", level, text: line.slice(prefix.length) };
    }
  }
  return null;
}

function tryParseQuote(line: string): Block | null {
  if (line.startsWith("> ")) {
    return { kind: "quote", text: line.slice(2) };
  }
  return null;
}

function tryParseList(lines: string[], start: number): { block: Block; nextIndex: number } | null {
  const firstUnordered = parseUnorderedItem(lines[start]);
  const firstOrdered = parseOrderedItem(lines[start]);
  if (firstUnordered === null && firstOrdered === null) return null;
  const orderedList = firstOrdered !== null;
  const items: string[] = [];
  let index = start;
  while (index < lines.length) {
    const item = orderedList ? parseOrderedItem(lines[index]) : parseUnorderedItem(lines[index]);
    if (item === null) break;
    items.push(item);
    index += 1;
  }
  return { block: { kind: "list", ordered: orderedList, items }, nextIndex: index };
}

function parseUnorderedItem(line: string): string | null {
  if (line.startsWith("- ") || line.startsWith("* ")) {
    return line.slice(2);
  }
  return null;
}

function parseOrderedItem(line: string): string | null {
  let cursor = 0;
  while (cursor < line.length && isAsciiDigit(line[cursor])) {
    cursor += 1;
  }
  if (cursor === 0) return null;
  if (line.startsWith(". ", cursor)) {
    return line.slice(cursor + 2);
  }
  return null;
}

function isAsciiDigit(char: string): boolean {
  return char >= "0" && char <= "9";
}

function parseParagraph(lines: string[], start: number): { block: Block; nextIndex: number } {
  const paragraph = [lines[start]];
  let index = start + 1;
  while (index < lines.length && isParagraphContinuation(lines[index])) {
    paragraph.push(lines[index]);
    index += 1;
  }
  return { block: { kind: "paragraph", text: paragraph.join(" ") }, nextIndex: index };
}

function isParagraphContinuation(line: string): boolean {
  if (!line.trim()) return false;
  return !isBlockStart(line);
}

function isBlockStart(line: string): boolean {
  if (line.startsWith("```")) return true;
  if (line.startsWith("# ") || line.startsWith("## ") || line.startsWith("### ")) return true;
  if (line.startsWith("> ")) return true;
  if (line.startsWith("- ") || line.startsWith("* ")) return true;
  return parseOrderedItem(line) !== null;
}

function headingTag(level: number): "h2" | "h3" | "h4" {
  if (level === 1) return "h2";
  if (level === 2) return "h3";
  return "h4";
}

function blockKey(block: Block): string {
  switch (block.kind) {
    case "heading":
      return `heading-${block.level}-${block.text}`;
    case "paragraph":
      return `paragraph-${block.text}`;
    case "quote":
      return `quote-${block.text}`;
    case "code":
      return `code-${block.text}`;
    case "list":
      return `list-${block.ordered ? "ol" : "ul"}-${block.items.join("\n")}`;
  }
}

function occurrenceKey(content: string, occurrences: Map<string, number>): string {
  const occurrence = occurrences.get(content) ?? 0;
  occurrences.set(content, occurrence + 1);
  return JSON.stringify([content, occurrence]);
}

export function SafeMarkdown({ children }: { readonly children: string }) {
  const blocks = parseMarkdown(children);
  // Repeated paragraphs and list items are valid source content.
  const blockOccurrences = new Map<string, number>();
  return (
    <div className="markdown-result">
      {blocks.map((block) => {
        const key = occurrenceKey(blockKey(block), blockOccurrences);
        switch (block.kind) {
          case "heading": {
            const Heading = headingTag(block.level);
            return <Heading key={key}>{inlineMarkdown(block.text)}</Heading>;
          }
          case "paragraph":
            return <p key={key}>{inlineMarkdown(block.text)}</p>;
          case "quote":
            return <blockquote key={key}>{inlineMarkdown(block.text)}</blockquote>;
          case "code":
            return (
              <pre key={key}>
                <code>{block.text}</code>
              </pre>
            );
          case "list": {
            const List = block.ordered ? "ol" : "ul";
            const itemOccurrences = new Map<string, number>();
            return (
              <List key={key}>
                {block.items.map((item) => (
                  <li key={occurrenceKey(item, itemOccurrences)}>{inlineMarkdown(item)}</li>
                ))}
              </List>
            );
          }
        }
      })}
    </div>
  );
}
