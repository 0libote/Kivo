import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { useRef, useState } from "react";
import { useNativeEvent } from "../hooks/useNativeEvent";
import { nativeBridge } from "../platform/native";
import type { DictationSnapshot, Platform } from "../types";
import { formatShortcut } from "./shortcut";

export function DictationPractice({
  shortcut,
  platform,
}: {
  readonly shortcut: string;
  readonly platform: Platform;
}) {
  const field = useRef<HTMLTextAreaElement>(null);
  const [status, setStatus] = useState("");
  useNativeEvent<DictationSnapshot>("dictation-state", (snapshot) => {
    setStatus(dictationStatusMessage(snapshot));
  });
  // Backend microphone failures all mention the microphone ("Microphone access
  // is required…", "…could not start. Check microphone…"); other errors
  // (speech engine, shortcuts) link nowhere.
  const showMicSettings = /could\s?not|couldn’t|microphone/i.test(status);
  return (
    <div {...stylex.props(styles.practice)}>
      <div {...stylex.props(styles.header)}>
        <strong {...stylex.props(styles.title)}>Practice dictation</strong>
        <Button
          label="Try dictation"
          onClick={() => field.current?.focus()}
          size="sm"
          variant="ghost"
        />
      </div>
      <p {...stylex.props(styles.copy)}>
        Click the field, then hold {formatShortcut(shortcut, platform).join(" + ")} and speak.
      </p>
      <textarea
        aria-label="Dictation practice"
        placeholder="What’s on your mind?"
        ref={field}
        rows={3}
        spellCheck
        {...stylex.props(styles.field)}
      />
      <p aria-live="polite" {...stylex.props(styles.status)}>
        {status || "Only your latest dictation is kept, until you clear it or quit."}
      </p>
      {showMicSettings ? (
        <Button
          label="Open microphone settings"
          onClick={() =>
            void nativeBridge
              .openPermissionSettings("microphone")
              .catch(() => setStatus("Open your system microphone settings to check access."))
          }
          size="sm"
          variant="ghost"
        />
      ) : null}
    </div>
  );
}

function dictationStatusMessage(snapshot: DictationSnapshot): string {
  switch (snapshot.status) {
    case "error":
      return snapshot.message ?? "Dictation could not start.";
    case "listening":
      return "Listening. Release your shortcut to finish.";
    case "processing":
      return "Finishing your sentence…";
    case "success":
      return "Your sentence is ready.";
    default:
      return "";
  }
}

const styles = stylex.create({
  practice: {
    width: "100%",
    padding: "17px",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
  },
  header: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "12px",
  },
  title: {
    fontSize: "14px",
    fontWeight: 600,
  },
  copy: {
    margin: "6px 0 0",
    color: "var(--color-text-secondary)",
    fontSize: "12px",
  },
  field: {
    width: "100%",
    minHeight: "120px",
    marginTop: "14px",
    padding: "14px",
    resize: "vertical",
    userSelect: "text",
    color: "var(--color-text-primary)",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    borderRadius: "var(--radius-element)",
    lineHeight: 1.6,
    ":focus": {
      borderColor: "color-mix(in srgb, var(--color-accent) 55%, transparent)",
      outline: "none",
    },
    "::placeholder": {
      color: "var(--color-text-disabled)",
    },
  },
  status: {
    minHeight: "18px",
    color: "var(--color-text-disabled)",
    fontSize: "11px",
  },
});
