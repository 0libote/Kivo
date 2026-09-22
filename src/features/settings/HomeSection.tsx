import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { motion, useReducedMotion } from "motion/react";
import { useEffect, useState } from "react";
import { DictationPractice } from "../../components/DictationPractice";
import { Icon, type IconName } from "../../components/Icon";
import { formatShortcut } from "../../components/shortcut";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import type { AppContext, AppSettings } from "../../types";

export function HomeSection({
  context,
  settings,
  onWriting,
  onDictation,
}: {
  readonly context: AppContext;
  readonly settings: AppSettings;
  readonly onWriting: () => void;
  readonly onDictation: () => void;
}) {
  const [recovery, setRecovery] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const paused = context.paused;
  const refresh = () => {
    void nativeBridge
      .getDictationRecovery()
      .then(setRecovery)
      .catch(() => setNotice("Your last dictation could not be loaded."));
  };
  useEffect(refresh, []);
  useNativeEvent("recovery-changed", refresh);

  return (
    <section {...stylex.props(styles.content)}>
      <header {...stylex.props(styles.header)}>
        <h1 {...stylex.props(styles.title)}>Home</h1>
        <p {...stylex.props(styles.subtitle)}>Write with your voice. Keep your train of thought.</p>
      </header>
      {paused ? (
        <output {...stylex.props(styles.pause)}>
          <span>Kivo is paused</span>
          <Button
            label="Resume Kivo"
            onClick={() =>
              void nativeBridge
                .setPaused(false)
                .catch(() => setNotice("Kivo could not resume. Try again."))
            }
            size="sm"
          />
        </output>
      ) : null}
      <div
        aria-label="Your shortcuts"
        data-testid="home-shortcuts"
        {...stylex.props(styles.shortcuts)}
      >
        <ShortcutRow
          ariaLabel="Change dictation shortcut"
          blurb="Hold to speak, release to finish"
          icon="microphone"
          keys={formatShortcut(settings.dictationShortcut, context.platform)}
          onClick={onDictation}
          title="Dictation"
        />
        <ShortcutRow
          ariaLabel="Change Writing Tools shortcut"
          blurb="Select text, then press your shortcut"
          icon="pencil"
          keys={formatShortcut(settings.writingShortcut, context.platform)}
          onClick={onWriting}
          title="Writing Tools"
        />
      </div>
      <DictationPractice platform={context.platform} shortcut={settings.dictationShortcut} />
      {recovery ? (
        <section aria-label="Last dictation" {...stylex.props(styles.recovery)}>
          <div {...stylex.props(styles.recoveryHeader)}>
            <strong {...stylex.props(styles.recoveryTitle)}>Your last dictation</strong>
            <span {...stylex.props(styles.recoveryMeta)}>Kept for this session</span>
          </div>
          <p {...stylex.props(styles.recoveryText)}>{recovery}</p>
          <div {...stylex.props(styles.recoveryActions)}>
            <Button
              label="Copy text"
              onClick={() =>
                void nativeBridge
                  .copyText(recovery)
                  .then(() => setNotice("Copied to clipboard."))
                  .catch(() => setNotice("The clipboard is unavailable. Try again."))
              }
              size="sm"
            />
            <Button
              label="Clear"
              onClick={() =>
                void nativeBridge
                  .clearDictationRecovery()
                  .then(() => setRecovery(null))
                  .catch(() => setNotice("The dictation could not be cleared."))
              }
              size="sm"
            />
          </div>
        </section>
      ) : null}
      <p {...stylex.props(styles.footnote)}>
        Close this window to keep Kivo in{" "}
        {context.platform === "windows" ? "the system tray" : "the menu bar"}.
      </p>
      {notice ? <output {...stylex.props(styles.feedback)}>{notice}</output> : null}
    </section>
  );
}

function ShortcutRow({
  ariaLabel,
  blurb,
  icon,
  keys,
  onClick,
  title,
}: {
  readonly ariaLabel: string;
  readonly blurb: string;
  readonly icon: IconName;
  readonly keys: string[];
  readonly onClick: () => void;
  readonly title: string;
}) {
  // The old row transitioned transform; keep the lift tiny and honor the
  // OS reduced-motion preference that CSS alone can no longer cover here.
  const reduceMotion = useReducedMotion();
  return (
    <motion.button
      aria-label={ariaLabel}
      data-testid="home-shortcut"
      onClick={onClick}
      transition={{ duration: 0.12, ease: "easeOut" }}
      type="button"
      whileHover={reduceMotion ? undefined : { y: -1 }}
      {...stylex.props(styles.shortcut)}
    >
      <Icon name={icon} size={19} {...stylex.props(styles.shortcutIcon)} />
      <div {...stylex.props(styles.shortcutCopy)}>
        <strong {...stylex.props(styles.shortcutTitle)}>{title}</strong>
        <span {...stylex.props(styles.shortcutBlurb)}>{blurb}</span>
      </div>
      <div {...stylex.props(styles.shortcutTail)}>
        <div {...stylex.props(styles.keyRow)}>
          {keys.map((key) => (
            <kbd key={key} {...stylex.props(styles.key)}>
              {key}
            </kbd>
          ))}
        </div>
        <span {...stylex.props(styles.shortcutEdit)}>Edit</span>
      </div>
    </motion.button>
  );
}

const styles = stylex.create({
  content: {
    maxWidth: "620px",
    marginInline: "auto",
  },
  header: {
    marginBottom: "27px",
  },
  title: {
    margin: "0 0 4px",
    fontSize: "25px",
    fontWeight: 650,
    lineHeight: 1.2,
    letterSpacing: "-0.03em",
  },
  subtitle: {
    maxWidth: "560px",
    margin: 0,
    color: "var(--color-text-secondary)",
    fontSize: "13px",
  },
  pause: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "12px",
    marginBottom: "12px",
    padding: "10px 12px",
    backgroundColor: "var(--color-background-muted)",
    borderRadius: "var(--radius-element)",
  },
  shortcuts: {
    display: "flex",
    flexDirection: "column",
    marginBlockEnd: "22px",
    overflow: "hidden",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
  },
  shortcut: {
    position: "relative",
    display: "flex",
    alignItems: "center",
    gap: "12px",
    width: "100%",
    minHeight: "68px",
    padding: "13px 15px",
    color: "var(--color-text-primary)",
    textAlign: "left",
    cursor: "pointer",
    backgroundColor: "transparent",
    borderBlockStartWidth: "1px",
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--color-border)",
    ":first-child": {
      borderBlockStartWidth: 0,
    },
    ":hover": {
      backgroundColor: "var(--kivo-hover)",
    },
    ":focus-visible": {
      outline: "2px solid var(--color-accent)",
      outlineOffset: "-2px",
    },
    "@media (max-width: 680px)": {
      flexWrap: "wrap",
      minHeight: "72px",
      alignItems: "flex-start",
    },
  },
  shortcutIcon: {
    display: "block",
    flexShrink: 0,
    color: "var(--color-text-secondary)",
  },
  shortcutCopy: {
    flex: 1,
    minWidth: 0,
  },
  shortcutTitle: {
    display: "block",
    fontSize: "13.5px",
    fontWeight: 600,
  },
  shortcutBlurb: {
    display: "block",
    marginTop: "2px",
    color: "var(--color-text-secondary)",
    fontSize: "12px",
  },
  shortcutTail: {
    display: "flex",
    alignItems: "center",
    gap: "14px",
    marginInlineStart: "auto",
    "@media (max-width: 680px)": {
      justifyContent: "space-between",
    },
  },
  keyRow: {
    display: "flex",
    gap: "5px",
  },
  key: {
    display: "grid",
    placeItems: "center",
    minWidth: "25px",
    height: "23px",
    padding: "0 6px",
    fontSize: "10.5px",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "4px",
    backgroundColor: "var(--color-background-muted)",
  },
  shortcutEdit: {
    display: "none",
  },
  recovery: {
    marginTop: "22px",
    padding: "18px",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-container)",
    boxShadow: "var(--shadow-med)",
  },
  recoveryHeader: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "12px",
    "@media (max-width: 680px)": {
      flexWrap: "wrap",
    },
  },
  recoveryTitle: {
    fontSize: "14px",
    fontWeight: 600,
  },
  recoveryMeta: {
    color: "var(--color-text-secondary)",
    fontSize: "11px",
  },
  recoveryText: {
    maxHeight: "150px",
    margin: "14px 0 0",
    overflow: "auto",
    color: "var(--color-text-primary)",
    whiteSpace: "pre-wrap",
    overflowWrap: "anywhere",
    userSelect: "text",
  },
  recoveryActions: {
    display: "flex",
    gap: "8px",
    marginTop: "12px",
  },
  footnote: {
    margin: "28px 0 0",
    paddingInlineStart: "2px",
    color: "var(--kivo-text-tertiary)",
    fontSize: "11px",
  },
  feedback: {
    margin: "12px 0 0",
    color: "var(--color-text-secondary)",
    fontSize: "12px",
  },
});
