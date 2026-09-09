import { useEffect, useRef, useState } from "react";
import type { Platform } from "../types";

interface ShortcutRecorderProps {
  readonly label: string;
  readonly platform: Platform;
  readonly value: string;
  readonly onChange: (shortcut: string) => unknown;
}

const MODIFIERS = new Set(["Control", "Shift", "Alt", "Meta"]);

function displayKey(event: KeyboardEvent): string {
  if (event.code === "Space") return "Space";
  if (event.key.length === 1) return event.key.toUpperCase();
  return event.key;
}

function shortcutFromEvent(event: KeyboardEvent): string | null {
  const values: string[] = [];
  if (event.ctrlKey) values.push("Ctrl");
  if (event.altKey) values.push("Alt");
  if (event.shiftKey) values.push("Shift");
  if (event.metaKey) values.push("Meta");

  if (!MODIFIERS.has(event.key)) {
    values.push(displayKey(event));
  }

  return values.length >= 2 || (values.length === 1 && !MODIFIERS.has(event.key)) ? values.join("+") : null;
}

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

export function ShortcutRecorder({ label, platform, value, onChange }: ShortcutRecorderProps) {
  const [recording, setRecording] = useState(false);
  const [invalid, setInvalid] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!recording) return;

    function onKeyDown(event: KeyboardEvent) {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        setRecording(false);
        setInvalid(false);
        return;
      }
      const shortcut = shortcutFromEvent(event);
      if (!shortcut) {
        setInvalid(true);
        return;
      }
      setInvalid(false);
      setRecording(false);
      void onChange(shortcut);
    }

    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [onChange, recording]);

  return (
    <div className="shortcut-recorder">
      <button
        aria-label={`${label}: ${recording ? "recording" : value}`}
        className="shortcut-recorder__button"
        data-recording={recording}
        onClick={() => {
          setRecording(true);
          setInvalid(false);
          buttonRef.current?.focus();
        }}
        ref={buttonRef}
        type="button"
      >
        {recording ? (
          <span className="shortcut-recorder__prompt">Press shortcut</span>
        ) : (
          formatShortcut(value, platform).map((key, index) => <kbd key={`${key}-${index}`}>{key}</kbd>)
        )}
      </button>
      {invalid ? <span className="shortcut-recorder__error">Include a key with your modifiers.</span> : null}
    </div>
  );
}
