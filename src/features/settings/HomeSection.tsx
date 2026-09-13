import { useEffect, useState } from "react";
import { Button } from "../../components/Button";
import { DictationPractice } from "../../components/DictationPractice";
import { formatShortcut } from "../../components/ShortcutRecorder";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import type { AppContext, AppSettings } from "../../types";

export function HomeSection({ context, settings, onWriting }: { readonly context: AppContext; readonly settings: AppSettings; readonly onWriting: () => void }) {
  const [recovery, setRecovery] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const paused = context.paused;
  const refresh = () => { void nativeBridge.getDictationRecovery().then(setRecovery).catch(() => setNotice("Your last dictation could not be loaded.")); };
  useEffect(refresh, []);
  useNativeEvent("recovery-changed", refresh);

  return <section className="settings-content home-content">
    <p className="home-eyebrow">{paused ? "Kivo is paused" : "Your writing space"}</p>
    {paused ? <Button compact onClick={() => void nativeBridge.setPaused(false).catch(() => setNotice("Kivo could not resume. Try again."))}>Resume Kivo</Button> : null}
    <h1>A thought.<br />A few spoken words.</h1>
    <p className="home-intro">Write in the app you’re already using.<br />Kivo takes care of the typing.</p>
    <div className="home-shortcut"><div><strong>Hold. Speak. Release.</strong><span>Your dictation shortcut</span></div><div className="shortcut-summary">{formatShortcut(settings.dictationShortcut, context.platform).map((key, index) => <kbd key={index}>{key}</kbd>)}</div></div>
    <DictationPractice platform={context.platform} shortcut={settings.dictationShortcut} />
    {recovery ? <section className="dictation-recovery" aria-label="Last dictation"><div className="dictation-practice__header"><strong>Your last dictation</strong><span>Kept for this session</span></div><p>{recovery}</p><div className="recovery-actions"><Button compact onClick={() => void nativeBridge.copyText(recovery).then(() => setNotice("Copied to clipboard.")).catch(() => setNotice("The clipboard is unavailable. Try again."))}>Copy text</Button><Button compact onClick={() => void nativeBridge.clearDictationRecovery().then(() => setRecovery(null)).catch(() => setNotice("The dictation could not be cleared."))}>Clear</Button></div></section> : null}
    <div className="home-writing"><p>Need to refine something?<br />Select text and press <strong>{formatShortcut(settings.writingShortcut, context.platform).join(" + ")}</strong>.</p><button className="text-link" onClick={onWriting} type="button">Writing Tools →</button></div>
    {notice ? <p role="status" className="section-feedback">{notice}</p> : null}
  </section>;
}
