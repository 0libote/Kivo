// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { formatShortcut } from "../components/ShortcutRecorder";
import { defaultSettings } from "../types";
import { detectedPlatform, surfaceFromLabel } from "./native";

// Runs on every CI OS (ubuntu + windows) and covers both platform branches
// regardless of the host runner: the harness derives its platform from
// navigator.userAgent, so spoofing the UA exercises the Windows branches
// even on Linux/macOS runners (mirrored in e2e by windows-branches.pw.ts).

const WINDOWS_UA =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const MACOS_UA =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const LINUX_UA =
  "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

function setUserAgent(userAgent: string) {
  Object.defineProperty(window.navigator, "userAgent", {
    value: userAgent,
    configurable: true,
  });
}

afterEach(() => {
  window.history.replaceState({}, "", "/");
});

describe("detectedPlatform", () => {
  it("detects Windows from the user agent", () => {
    setUserAgent(WINDOWS_UA);
    expect(detectedPlatform()).toBe("windows");
  });

  it("defaults to macOS for macOS and Linux agents", () => {
    setUserAgent(MACOS_UA);
    expect(detectedPlatform()).toBe("macos");
    // Linux WebViews do not exist; the dev-server fallback is the macOS UI.
    setUserAgent(LINUX_UA);
    expect(detectedPlatform()).toBe("macos");
  });
});

describe("surfaceFromLabel", () => {
  it("maps every native window label", () => {
    for (const surface of [
      "flow-bar",
      "writing-tools",
      "settings",
      "onboarding",
    ] as const) {
      window.history.replaceState({}, "", "/");
      expect(surfaceFromLabel(surface)).toBe(surface);
    }
  });

  it("prefers the query surface and falls back to settings", () => {
    window.history.replaceState({}, "", "/?surface=writing-tools");
    expect(surfaceFromLabel("settings")).toBe("writing-tools");
    window.history.replaceState({}, "", "/?surface=bogus");
    expect(surfaceFromLabel(undefined)).toBe("settings");
    window.history.replaceState({}, "", "/");
    expect(surfaceFromLabel(undefined)).toBe("settings");
  });
});

describe("shortcut labels cover both platforms", () => {
  it("renders every macOS modifier as a symbol", () => {
    expect(formatShortcut("Meta+Alt+Shift+Ctrl+X", "macos")).toEqual([
      "⌘",
      "⌥",
      "⇧",
      "⌃",
      "X",
    ]);
  });

  it("labels Win on Windows and never leaks Meta", () => {
    const rendered = formatShortcut("Ctrl+Meta+Space", "windows");
    expect(rendered).toContain("Win");
    expect(rendered.join("+")).not.toContain("Meta");
  });

  it("keeps the native defaults distinct per platform", () => {
    expect(defaultSettings("macos").dictationShortcut).not.toBe(
      defaultSettings("windows").dictationShortcut,
    );
    expect(defaultSettings("macos").writingShortcut).not.toBe(
      defaultSettings("windows").writingShortcut,
    );
  });
});
