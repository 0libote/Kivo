import type { Platform } from "../types";

/**
 * Render a stored shortcut as platform-appropriate key labels. Kept free of
 * React/StyleX so unit tests can import it without the StyleX compiler.
 */
export function formatShortcut(value: string, platform: Platform) {
  return value.split("+").map((part) => {
    if (platform === "macos") {
      if (part === "Meta") return "⌘";
      if (part === "Alt") return "⌥";
      if (part === "Shift") return "⇧";
      if (part === "Ctrl") return "⌃";
    }
    if (platform === "windows" && part === "Meta") return "Win";
    return part;
  });
}
