import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useRef, useState } from "react";
import type { Platform } from "../types";
import { formatShortcut } from "./shortcut";

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

  return values.length >= 2 || (values.length === 1 && !MODIFIERS.has(event.key))
    ? values.join("+")
    : null;
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
    <div {...stylex.props(styles.root)}>
      <Button
        label={`${label}: ${recording ? "recording" : value}`}
        onClick={() => {
          setRecording(true);
          setInvalid(false);
          buttonRef.current?.focus();
        }}
        // Leaving the button disarms the recorder so a later keystroke
        // elsewhere is never swallowed by a forgotten armed state.
        onBlur={() => {
          setRecording(false);
          setInvalid(false);
        }}
        ref={buttonRef}
        size="sm"
        type="button"
        variant="secondary"
      >
        {recording ? (
          <span {...stylex.props(styles.prompt)}>Press shortcut</span>
        ) : (
          <span {...stylex.props(styles.keys)}>
            {formatShortcut(value, platform).map((key, index) => (
              <kbd key={`${key}-${index}`} {...stylex.props(styles.key)}>
                {key}
              </kbd>
            ))}
          </span>
        )}
      </Button>
      {invalid ? (
        <span role="alert" {...stylex.props(styles.error)}>
          Include a key with your modifiers.
        </span>
      ) : null}
    </div>
  );
}

const styles = stylex.create({
  root: {
    display: "grid",
    justifyItems: "end",
    gap: "5px",
    minWidth: "150px",
  },
  keys: {
    display: "flex",
    alignItems: "center",
    gap: "4px",
  },
  prompt: {
    color: "var(--color-accent)",
    fontSize: "11px",
  },
  key: {
    display: "grid",
    placeItems: "center",
    minWidth: "23px",
    height: "21px",
    paddingInline: "6px",
    fontSize: "11px",
    borderWidth: "1px",
    borderStyle: "solid",
    borderBottomWidth: "2px",
    borderColor: "var(--color-border-emphasized)",
    borderRadius: "4px",
    backgroundColor: "var(--color-background-muted)",
  },
  error: {
    color: "var(--color-error)",
    fontSize: "11px",
  },
});
