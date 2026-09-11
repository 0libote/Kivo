import { describe, expect, it } from "vitest";
import { formatShortcut } from "./components/ShortcutRecorder";
import { defaultSettings } from "./types";

// Cross-platform default parity. The Rust side owns the same defaults
// (ShortcutBinding::dictation_default / writing_tools_default in
// src-tauri/src/config/mod.rs, normalized to Control+Super in shell.rs).
// These tests run on every OS in CI, so a default changed on one platform
// without its counterpart fails fast instead of surfacing weeks later.
describe("platform defaults parity", () => {
  it("uses the native hold shortcut per platform", () => {
    expect(defaultSettings("macos").dictationShortcut).toBe("Fn");
    expect(defaultSettings("windows").dictationShortcut).toBe("Ctrl+Meta");
  });

  it("uses the portable writing shortcut per platform", () => {
    expect(defaultSettings("macos").writingShortcut).toBe("Ctrl+Shift+Space");
    expect(defaultSettings("windows").writingShortcut).toBe("Ctrl+Space");
  });

  it("labels the Windows key as Win, not Meta", () => {
    expect(formatShortcut("Ctrl+Meta", "windows")).toContain("Win");
    expect(formatShortcut("Ctrl+Meta", "windows")).not.toContain("Meta");
  });

  it("renders macOS modifiers as symbols", () => {
    expect(formatShortcut("Ctrl+Shift+Space", "macos")).toEqual(["⌃", "⇧", "Space"]);
  });
});
