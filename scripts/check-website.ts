import { readFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { JSDOM } from "jsdom";

const root = resolve("website");
const pages = [...new Bun.Glob("*.html").scanSync(root)];
const documents = new Map(
  pages.map((page) => [page, new JSDOM(readFileSync(resolve(root, page), "utf8")).window.document]),
);
let failures = 0;
for (const [page, document] of documents) {
  for (const node of document.querySelectorAll("[href], [src]")) {
    const value = node.getAttribute("href") ?? node.getAttribute("src");
    if (!value || /^(?:https?:|mailto:|data:)/.test(value)) continue;
    const [file, anchor] = value.split("#");
    const target = resolve(dirname(resolve(root, page)), decodeURIComponent(file || page));
    if (relative(root, target).startsWith("..") || !(await Bun.file(target).exists())) {
      console.error(`${page}: missing local resource ${value}`);
      failures++;
      continue;
    }
    if (anchor && target.endsWith(".html")) {
      const targetDocument = documents.get(relative(root, target));
      if (!targetDocument?.getElementById(decodeURIComponent(anchor))) {
        console.error(`${page}: missing anchor ${value}`);
        failures++;
      }
    }
  }
  const ids = [...document.querySelectorAll("[id]")].map((node) => node.id);
  if (new Set(ids).size !== ids.length) {
    console.error(`${page}: duplicate IDs`);
    failures++;
  }
  if (!document.title || !document.documentElement.lang) {
    console.error(`${page}: title/language missing`);
    failures++;
  }
}
const lint = Bun.spawn(
  [process.execPath, "--bun", "oxlint", "--deny-warnings", "--no-ignore", "website/site.js"],
  { stdout: "inherit", stderr: "inherit" },
);
if ((await lint.exited) !== 0) failures++;
if (failures) process.exit(1);
console.info(`Website: ${pages.length} pages, local resources/anchors and JavaScript verified.`);
