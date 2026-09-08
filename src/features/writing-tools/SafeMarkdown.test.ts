import { describe, expect, it } from "vitest";
import { parseMarkdown } from "./SafeMarkdown";

describe("parseMarkdown", () => {
  it("parses structured informational results without interpreting HTML", () => {
    expect(parseMarkdown("## Summary\n\n- One\n- <script>unsafe</script>")).toEqual([
      { kind: "heading", level: 2, text: "Summary" },
      { kind: "list", ordered: false, items: ["One", "<script>unsafe</script>"] },
    ]);
  });
});
