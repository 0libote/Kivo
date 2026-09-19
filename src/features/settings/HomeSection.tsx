import { useEffect, useState } from "react";
import { Button } from "../../components/Button";
import { DictationPractice } from "../../components/DictationPractice";
import { Icon } from "../../components/Icon";
import { formatShortcut } from "../../components/ShortcutRecorder";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import type { AppContext, AppSettings } from "../../types";

export function HomeSection({ context, settings, onWriting, onDictation }: { readonly context: AppContext; readonly settings: AppSettings; readonly onWriting: () => void; readonly onDictation: () => void }) {
  const [recovery, setRecovery] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const paused = context.paused;
  const refresh = () => { void nativeBridge.getDictationRecovery().then(setRecovery).catch(() => setNotice("Your last dictation could not be loaded.")); };
  useEffect(refresh, []);
  useNativeEvent("recovery-changed", refresh);

  return <section className="settings-content home-content">
    <header className="home-header"><span className="home-eyebrow"><i />Workspace ready</span><h1>Home</h1><p>Write with your voice. Keep your train of thought.</p></header>
    {paused ? <div className="home-pause" role="status"><span>Kivo is paused</span><Button compact onClick={() => void nativeBridge.setPaused(false).catch(() => setNotice("Kivo could not resume. Try again."))}>Resume Kivo</Button></div> : null}
    <div className="home-shortcuts" aria-label="Your shortcuts">
      <button className="home-shortcut" type="button" onClick={onDictation} aria-label="Change dictation shortcut"><Icon name="microphone" size={19} /><div className="home-shortcut__copy"><strong>Dictation</strong><span>Hold to speak, release to finish</span></div><div className="home-shortcut__tail"><div className="shortcut-summary">{formatShortcut(settings.dictationShortcut, context.platform).map((key, index) => <kbd key={index}>{key}</kbd>)}</div><span className="home-shortcut__edit">Edit</span></div></button>
      <button className="home-shortcut" type="button" onClick={onWriting} aria-label="Change Writing Tools shortcut"><Icon name="pencil" size={19} /><div className="home-shortcut__copy"><strong>Writing Tools</strong><span>Select text, then press your shortcut</span></div><div className="home-shortcut__tail"><div className="shortcut-summary">{formatShortcut(settings.writingShortcut, context.platform).map((key, index) => <kbd key={index}>{key}</kbd>)}</div><span className="home-shortcut__edit">Edit</span></div></button>
    </div>
    <DictationPractice platform={context.platform} shortcut={settings.dictationShortcut} />
    {recovery ? <section className="dictation-recovery" aria-label="Last dictation"><div className="dictation-practice__header"><strong>Your last dictation</strong><span>Kept for this session</span></div><p>{recovery}</p><div className="recovery-actions"><Button compact onClick={() => void nativeBridge.copyText(recovery).then(() => setNotice("Copied to clipboard.")).catch(() => setNotice("The clipboard is unavailable. Try again."))}>Copy text</Button><Button compact onClick={() => void nativeBridge.clearDictationRecovery().then(() => setRecovery(null)).catch(() => setNotice("The dictation could not be cleared."))}>Clear</Button></div></section> : null}
    <p className="home-footnote">Close this window to keep Kivo in {context.platform === "windows" ? "the system tray" : "the menu bar"}.</p>
    {notice ? <p role="status" className="section-feedback">{notice}</p> : null}
  </section>;
}
