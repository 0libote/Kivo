import { describe, expect, it } from "bun:test";
import { MARKDOWN_ALLOWED_ATTR, MARKDOWN_ALLOWED_TAGS, renderMarkdown } from "./SafeMarkdown";

/** Parse sanitized output in a real DOM so element queries are meaningful. */
function rendered(source: string): HTMLElement {
  const host = document.createElement("div");
  host.innerHTML = renderMarkdown(source);
  return host;
}

describe("renderMarkdown structure", () => {
  it("renders headings demoted one level, lists, and paragraphs", () => {
    const host = rendered("## Summary\n\n- One\n- Two");
    expect(host.querySelector("h3")?.textContent).toBe("Summary");
    expect(host.querySelector("h1")).toBeNull();
    expect([...host.querySelectorAll("li")].map((item) => item.textContent)).toEqual([
      "One",
      "Two",
    ]);
  });

  it("demotes top-level headings because the result card owns the h1", () => {
    const host = rendered("# Title");
    expect(host.querySelector("h1")).toBeNull();
    expect(host.querySelector("h2")?.textContent).toBe("Title");
  });

  it("renders inline code, emphasis, quotes, and fenced code blocks", () => {
    const host = rendered(
      "Use `code` and **strong** and *em*.\n\n> quoted\n\n```\nplain <div>\n```",
    );
    expect(host.querySelector("code")?.textContent).toBe("code");
    expect(host.querySelector("strong")?.textContent).toBe("strong");
    expect(host.querySelector("em")?.textContent).toBe("em");
    expect(host.querySelector("blockquote")?.textContent?.trim()).toBe("quoted");
    expect(host.querySelector("pre code")?.textContent).toContain("plain <div>");
  });
});

describe("renderMarkdown safety", () => {
  it("keeps raw HTML literal and creates no elements for it", () => {
    const host = rendered("- One\n- <script>unsafe</script>");
    expect(host.querySelector("script")).toBeNull();
    expect(host.textContent).toContain("<script>unsafe</script>");
  });

  it("keeps links literal so results can never navigate the popup", () => {
    const host = rendered("[OpenAI](https://openai.com)");
    expect(host.querySelector("a")).toBeNull();
    expect(host.textContent).toContain("[OpenAI](https://openai.com)");
  });

  it("keeps javascript: links and images literal", () => {
    const host = rendered("[x](javascript:alert(1))\n\n![alt](https://example.com/y.png)");
    expect(host.querySelector("a")).toBeNull();
    expect(host.querySelector("img")).toBeNull();
    expect(host.textContent).toContain("javascript:alert(1)");
  });

  it("neutralizes a hostile fenced-code language tag", () => {
    const host = rendered('```"><img src=x onerror=alert(1)>\ncode\n```');
    expect(host.querySelector("img")).toBeNull();
    expect(host.querySelector("pre code")?.textContent?.trim()).toBe("code");
  });

  it("strips event-handler attributes even if parsing ever emits them", () => {
    const host = rendered('<b onmouseover="alert(1)">bold</b>');
    expect(host.querySelector("[onmouseover]")).toBeNull();
  });

  it("exposes an allowlist with no navigation, media, form, or style elements", () => {
    for (const tag of [
      "a",
      "img",
      "script",
      "iframe",
      "form",
      "input",
      "button",
      "style",
      "video",
      "audio",
      "h1",
    ]) {
      expect(MARKDOWN_ALLOWED_TAGS).not.toContain(tag);
    }
    expect(MARKDOWN_ALLOWED_ATTR).toEqual([]);
  });

  it("keeps tables and strikethrough, which the allowlist covers", () => {
    const host = rendered("| a | b |\n|---|---|\n| 1 | 2 |\n\n~~gone~~");
    expect(host.querySelector("table")).not.toBeNull();
    expect(host.querySelector("del")?.textContent).toBe("gone");
  });

  it("strips task-list checkbox inputs, which the allowlist does not cover", () => {
    const host = rendered("- [ ] todo\n- [x] done");
    expect(host.querySelector("input")).toBeNull();
    expect(host.textContent).toContain("todo");
  });

  it("strips element attributes such as fenced-code language classes", () => {
    const host = rendered("```ts\ncode\n```");
    const code = host.querySelector("pre code");
    expect(code?.textContent?.trim()).toBe("code");
    expect(code?.getAttribute("class")).toBeNull();
  });
});
