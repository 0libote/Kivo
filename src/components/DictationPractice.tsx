import { useRef, useState } from "react";
import { useNativeEvent } from "../hooks/useNativeEvent";
import { nativeBridge } from "../platform/native";
import type { DictationSnapshot, Platform } from "../types";
import { formatShortcut } from "./ShortcutRecorder";

export function DictationPractice({ shortcut, platform }: { readonly shortcut: string; readonly platform: Platform }) {
  const field = useRef<HTMLTextAreaElement>(null);
  const [status, setStatus] = useState("");
  useNativeEvent<DictationSnapshot>("dictation-state", (snapshot) => {
    setStatus(snapshot.status === "error" ? snapshot.message ?? "Dictation could not start." : snapshot.status === "listening" ? "Listening. Release your shortcut to finish." : snapshot.status === "processing" ? "Finishing your sentence…" : snapshot.status === "success" ? "Your sentence is ready." : "");
  });
  return <div className="dictation-practice">
    <div className="dictation-practice__header"><strong>Try a sentence</strong><button className="text-link" onClick={() => field.current?.focus()} type="button">Try dictation</button></div>
    <p>Click the field, then hold {formatShortcut(shortcut, platform).join(" + ")} and speak.</p>
    <textarea aria-label="Dictation practice" placeholder="What’s on your mind?" ref={field} rows={3} spellCheck />
    <p className="dictation-practice__status" aria-live="polite">{status || "Only your latest dictation is kept, until you clear it or quit."}</p>
    {status.includes("could not") || status.includes("couldn’t") ? <button className="text-link" onClick={() => void nativeBridge.openPermissionSettings("microphone").catch(() => setStatus("Open your system microphone settings to check access."))} type="button">Open microphone settings</button> : null}
  </div>;
}
