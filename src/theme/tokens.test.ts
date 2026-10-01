import { describe, expect, it } from "bun:test";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      if (entry === "node_modules" || entry === "dist") continue;
      walk(path, out);
    } else if (/\.(tsx?|css)$/.test(entry)) {
      out.push(path);
    }
  }
  return out;
}

/** Every var(--token) used in src must resolve: Astryx base, Kivo build, or app.css. */
describe("design tokens", () => {
  it("uses only defined CSS custom properties", () => {
    const defined = new Set<string>();
    for (const css of [
      join(root, "src/theme/built/kivo.css"),
      join(root, "node_modules/@astryxdesign/core/dist/astryx.css"),
      join(root, "src/styles/app.css"),
    ]) {
      const text = readFileSync(css, "utf8");
      for (const match of text.matchAll(/(--[a-zA-Z0-9-_]+)\s*:/g)) defined.add(match[1]);
    }
    expect(defined.size).toBeGreaterThan(50);

    const used = new Map<string, string[]>();
    for (const file of walk(join(root, "src"))) {
      if (file.endsWith(".test.ts")) continue;
      const text = readFileSync(file, "utf8");
      for (const match of text.matchAll(/var\(\s*(--[a-zA-Z0-9-_]+)/g)) {
        const list = used.get(match[1]) ?? [];
        list.push(file.replace(`${root}/`, ""));
        used.set(match[1], list);
      }
    }

    const unknown = [...used.entries()].filter(([token]) => !defined.has(token));
    expect(
      unknown.map(([token, files]) => `${token} (${[...new Set(files)].join(", ")})`),
      "undefined CSS variables (add a fallback or define the token)",
    ).toEqual([]);
  });

  it("uses no hard-coded hex colors outside the theme", () => {
    const offenders: string[] = [];
    for (const file of walk(join(root, "src"))) {
      // The theme source owns the palette; translucent rgba borders and
      // hairline shadows elsewhere are deliberate overlay/shadow values.
      if (file.endsWith(".test.ts") || file.endsWith(".css") || file.includes("/theme/")) continue;
      const text = readFileSync(file, "utf8");
      for (const [index, line] of text.split("\n").entries()) {
        if (/#[0-9a-fA-F]{3,8}/.test(line)) {
          offenders.push(`${file.replace(`${root}/`, "")}:${index + 1}: ${line.trim()}`);
        }
      }
    }
    expect(offenders, "hard-coded hex colors (use a var(--color-*|--kivo-*) token)").toEqual([]);
  });
});
