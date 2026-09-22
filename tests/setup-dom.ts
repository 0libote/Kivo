import { JSDOM } from "jsdom";

/**
 * Bun test runs without a browser environment. The surfaces and the markdown
 * sanitizer expect real DOM semantics (and DOMPurify only sanitizes correctly
 * against a spec-compliant DOM), so every test file gets a jsdom global. This
 * is intentionally jsdom, not happy-dom: DOMPurify drops block elements under
 * happy-dom, which would silently weaken the SafeMarkdown safety tests.
 */
const dom = new JSDOM("<!doctype html><html><head></head><body></body></html>", {
  url: "http://localhost/",
  pretendToBeVisual: true,
});

const window = dom.window as unknown as Window & typeof globalThis;

// Copy every browser global jsdom provides (HTMLElement, Node, Event, …)
// without clobbering Bun's own globals, then attach the core objects the
// modules under test reach for directly.
for (const key of Object.getOwnPropertyNames(window)) {
  if (key === "window" || key === "document" || key === "navigator" || key === "localStorage") {
    continue;
  }
  if (key in globalThis) continue;
  const descriptor = Object.getOwnPropertyDescriptor(window, key);
  if (descriptor) Object.defineProperty(globalThis, key, descriptor);
}

const globals = globalThis as typeof globalThis & {
  window: Window & typeof globalThis;
  document: Document;
  navigator: Navigator;
  localStorage: Storage;
};
globals.window = window;
globals.document = window.document;
globals.navigator = window.navigator;
globals.localStorage = window.localStorage;
