import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { StatusDot } from "@astryxdesign/core/StatusDot";
import * as stylex from "@stylexjs/stylex";
import { type ReactNode, useCallback, useEffect, useState } from "react";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import type { AppContext, AppSettings, DictationSnapshot, PermissionStatus } from "../../types";

/**
 * Frontend-only dev bench (never a Tauri window, never IPC): one screen
 * that exercises every bit through the active bridge — MockBridge in
 * `bun dev`, the real Rust core under `bun tauri dev` on Linux.
 *
 * Read-only probes run on mount (permissions, mics, languages, models,
 * key status, recovery). Write paths are explicit buttons so nothing here
 * spends AI quota or overwrites a key by surprise.
 */
export function GalleryWindow({
  context,
  settings,
}: {
  readonly context: AppContext;
  readonly settings: AppSettings;
}) {
  const [permissions, setPermissions] = useState<PermissionStatus[]>([]);
  const [mics, setMics] = useState<string>("—");
  const [languages, setLanguages] = useState<string>("—");
  const [models, setModels] = useState<string>("—");
  const [keyStatus, setKeyStatus] = useState<string>("—");
  const [recovery, setRecovery] = useState<string | null>(null);
  const [dictation, setDictation] = useState<DictationSnapshot | null>(null);
  const [writing, setWriting] = useState<string>("Not run yet.");
  const [roundTrip, setRoundTrip] = useState<string>("Not run yet.");
  const [error, setError] = useState<string | null>(null);

  useNativeEvent<DictationSnapshot>("dictation-state", setDictation);

  const refresh = useCallback(() => {
    setError(null);
    void (async () => {
      try {
        const [perms, micList, langList, modelList, key, rec] = await Promise.all([
          nativeBridge.getPermissions(),
          nativeBridge.listMicrophones(),
          nativeBridge.listSpeechLanguages(),
          nativeBridge.listAiModels(settings.aiProvider),
          nativeBridge.getApiKeyStatus(),
          nativeBridge.getDictationRecovery(),
        ]);
        setPermissions(perms);
        setMics(micList.map((mic) => mic.name).join(", "));
        setLanguages(
          langList.map((lang) => `${lang.code}${lang.installed ? "" : " (missing)"}`).join(", "),
        );
        setModels(modelList.map((model) => model.id).join(", "));
        setKeyStatus(`configured=${key.configured ? "yes" : "no"} connection=${key.connection}`);
        setRecovery(rec);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "The probe failed.");
      }
    })();
  }, [settings.aiProvider]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function runWritingProbe() {
    setWriting("Running proofread on the sample text…");
    try {
      const response = await nativeBridge.runWritingAction({
        action: "proofread",
        text: "Hello, how are you?",
      });
      setWriting(
        response.kind === "replaced"
          ? "Replaced the text in place."
          : (response.text ?? "(empty result)"),
      );
    } catch (cause) {
      setWriting(cause instanceof Error ? `Failed: ${cause.message}` : "Failed.");
    }
  }

  async function runSettingsRoundTrip() {
    setRoundTrip("Writing soundFeedback=false then restoring…");
    try {
      const original = settings.soundFeedback;
      await nativeBridge.updateSettings({ soundFeedback: false });
      const restored = await nativeBridge.updateSettings({ soundFeedback: original });
      setRoundTrip(
        restored.soundFeedback === original
          ? "Round trip ok: write + restore both persisted."
          : "Round trip mismatch: restore did not persist.",
      );
      refresh();
    } catch (cause) {
      setRoundTrip(cause instanceof Error ? `Failed: ${cause.message}` : "Failed.");
      refresh();
    }
  }

  return (
    <main data-testid="gallery" {...stylex.props(styles.window)}>
      <aside {...stylex.props(styles.sidebar)}>
        <div {...stylex.props(styles.brand)}>
          <span aria-hidden="true" {...stylex.props(styles.mark)}>
            K
          </span>
          <span>Kivo bench</span>
        </div>
        <p {...stylex.props(styles.version)}>
          {context.platform} · {nativeBridge.isNative ? "native core" : "mock harness"} ·{" "}
          {context.version || "dev"}
        </p>
        <p {...stylex.props(styles.status)}>
          <StatusDot
            label={context.paused ? "Paused" : "Active"}
            variant={context.paused ? "neutral" : "success"}
          />
          {context.paused ? "Paused" : "Active"}
        </p>
      </aside>
      <section {...stylex.props(styles.main)}>
        <div {...stylex.props(styles.content)}>
          <h1 {...stylex.props(styles.title)}>Test bench</h1>
          <p {...stylex.props(styles.lead)}>
            One screen over the active bridge. Green here means the shared AppCore path works; only
            the thin per-OS adapter differs on macOS and Windows.
          </p>
          {error ? <Banner status="error" title={error} xstyle={styles.errorBanner} /> : null}

          <Group
            footer={
              <Button label="Re-run probes" onClick={refresh} size="sm" variant="secondary" />
            }
            title="Contract"
          >
            <Row label="Dictation shortcut">{settings.dictationShortcut}</Row>
            <Row label="Writing shortcut">{settings.writingShortcut}</Row>
            <Row label="AI provider / queue">
              {settings.aiProvider} · {settings.aiModels.join(", ")}
            </Row>
            <Row label="Permissions">
              {permissions.length === 0
                ? "—"
                : permissions
                    .map((p) => `${p.kind}=${p.state}${p.required ? "*" : ""}`)
                    .join(" · ")}
            </Row>
            <Row label="Microphones">{mics}</Row>
            <Row label="Speech languages">{languages}</Row>
            <Row label="AI models">{models}</Row>
            <Row label="API key">{keyStatus}</Row>
            <Row label="Recovery text">{recovery === null ? "(none)" : recovery}</Row>
            <Row label="Dictation event">
              {dictation === null ? "(no event yet)" : dictation.status}
            </Row>
          </Group>

          <Group
            footer={
              <>
                <Button
                  label="Start"
                  onClick={() =>
                    void nativeBridge
                      .startDictation()
                      .catch((cause: unknown) =>
                        setError(cause instanceof Error ? cause.message : "start failed"),
                      )
                  }
                  size="sm"
                  variant="secondary"
                />
                <Button
                  label="Stop"
                  onClick={() =>
                    void nativeBridge
                      .stopDictation()
                      .catch((cause: unknown) =>
                        setError(cause instanceof Error ? cause.message : "stop failed"),
                      )
                  }
                  size="sm"
                  variant="secondary"
                />
                <Button
                  label="Cancel"
                  onClick={() =>
                    void nativeBridge
                      .cancelDictation()
                      .catch((cause: unknown) =>
                        setError(cause instanceof Error ? cause.message : "cancel failed"),
                      )
                  }
                  size="sm"
                  variant="secondary"
                />
              </>
            }
            footerSplit
            note="Drives the real start/stop/cancel path. On Linux the simulated engine answers; in the mock harness the canned events answer."
            title="Dictation smoke"
          />

          <Group
            footer={
              <Button
                label="Run proofread"
                onClick={() => void runWritingProbe()}
                size="sm"
                variant="secondary"
              />
            }
            note="Runs proofread on the sample text. Uses AI quota when a key is configured against the live core."
            title="Writing smoke"
          >
            <Row label="Result">{writing}</Row>
          </Group>

          <Group
            footer={
              <Button
                label="Write + restore"
                onClick={() => void runSettingsRoundTrip()}
                size="sm"
                variant="secondary"
              />
            }
            title="Settings round trip"
          >
            <Row label="Result">{roundTrip}</Row>
          </Group>

          <Group
            footer={
              <>
                <Button
                  href="?surface=flow-bar&harness=1"
                  label="Flow Bar"
                  size="sm"
                  variant="secondary"
                />
                <Button
                  href="?surface=writing-tools&harness=1"
                  label="Writing Tools"
                  size="sm"
                  variant="secondary"
                />
                <Button
                  href="?surface=settings&harness=1"
                  label="Settings"
                  size="sm"
                  variant="secondary"
                />
                <Button
                  href="?surface=onboarding&harness=1"
                  label="Onboarding"
                  size="sm"
                  variant="secondary"
                />
              </>
            }
            footerSplit
            title="Surfaces"
          />
        </div>
      </section>
    </main>
  );
}

function Group({
  title,
  note,
  footer,
  footerSplit = false,
  children,
}: {
  readonly title: string;
  readonly note?: string;
  readonly footer?: ReactNode;
  readonly footerSplit?: boolean;
  readonly children?: ReactNode;
}) {
  return (
    <section {...stylex.props(styles.group)}>
      <h2 {...stylex.props(styles.groupTitle)}>{title}</h2>
      {note ? <p {...stylex.props(styles.note)}>{note}</p> : null}
      <div {...stylex.props(styles.groupBody)}>
        {children}
        {footer ? (
          <div {...stylex.props(styles.footer, footerSplit && styles.footerSplit)}>{footer}</div>
        ) : null}
      </div>
    </section>
  );
}

function Row({ label, children }: { readonly label: string; readonly children: ReactNode }) {
  return (
    <div {...stylex.props(styles.row)}>
      <span {...stylex.props(styles.rowLabel)}>{label}</span>
      <span {...stylex.props(styles.rowControl)}>{children}</span>
    </div>
  );
}

const styles = stylex.create({
  window: {
    display: "grid",
    gridTemplateColumns: "184px minmax(0, 1fr)",
    width: "100%",
    height: "100%",
    backgroundColor: "var(--color-background-body)",
    "@media (max-width: 680px)": {
      gridTemplateColumns: "148px minmax(0, 1fr)",
    },
  },
  sidebar: {
    display: "flex",
    flexDirection: "column",
    minWidth: 0,
    padding: "22px 10px 16px",
    color: "var(--color-text-primary)",
    backgroundColor: "var(--color-background-surface)",
    borderInlineEndWidth: "1px",
    borderInlineEndStyle: "solid",
    borderInlineEndColor: "var(--color-border)",
    "@media (max-width: 680px)": {
      paddingInline: "8px",
    },
  },
  brand: {
    display: "flex",
    alignItems: "center",
    gap: "9px",
    padding: "0 9px 23px",
    fontSize: "16px",
    letterSpacing: "-0.025em",
    color: "var(--color-text-primary)",
    "@media (max-width: 680px)": {
      paddingInline: "6px",
    },
  },
  mark: {
    display: "grid",
    placeItems: "center",
    width: "27px",
    height: "27px",
    color: "var(--color-on-accent)",
    backgroundColor: "var(--color-accent)",
    borderRadius: "7px",
  },
  version: {
    margin: 0,
    color: "var(--kivo-text-tertiary)",
    fontSize: "10px",
    fontVariantNumeric: "tabular-nums",
  },
  status: {
    display: "flex",
    alignItems: "center",
    gap: "9px",
    margin: "auto 8px 0",
    padding: "12px 0 0",
    color: "var(--kivo-text-tertiary)",
    fontSize: "10.5px",
    borderBlockStartWidth: "1px",
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--color-border)",
  },
  main: {
    minWidth: 0,
    overflow: "auto",
    scrollbarGutter: "stable",
    padding: "42px 48px 36px",
    backgroundColor: "var(--color-background-body)",
    outline: "none",
    "@media (max-width: 680px)": {
      padding: "28px 22px",
    },
  },
  content: {
    maxWidth: "620px",
    marginInline: "auto",
  },
  title: {
    margin: "0 0 4px",
    fontSize: "25px",
    fontWeight: 650,
    lineHeight: 1.2,
    letterSpacing: "-0.03em",
  },
  lead: {
    maxWidth: "560px",
    margin: "2px 0 14px",
    color: "var(--color-text-secondary)",
    fontSize: "13px",
  },
  errorBanner: {
    marginBlockEnd: "18px",
  },
  group: {
    marginBlockEnd: "22px",
  },
  groupTitle: {
    margin: "0 0 9px 2px",
    color: "var(--kivo-text-tertiary)",
    fontSize: "10px",
    fontWeight: 750,
    letterSpacing: "0.11em",
    textTransform: "uppercase",
  },
  note: {
    margin: "2px 0 14px",
    color: "var(--color-text-secondary)",
    fontSize: "12px",
    lineHeight: 1.4,
  },
  groupBody: {
    overflow: "hidden",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
  },
  row: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "18px",
    minHeight: "62px",
    padding: "12px 15px",
    borderBlockStartWidth: "1px",
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--color-border)",
    ":first-child": {
      borderBlockStartWidth: 0,
    },
    "@media (max-width: 680px)": {
      flexDirection: "column",
      alignItems: "flex-start",
      gap: "10px",
    },
  },
  rowLabel: {
    flexShrink: 0,
    color: "var(--color-text-primary)",
    fontSize: "13.5px",
    fontWeight: 620,
  },
  rowControl: {
    minWidth: 0,
    maxWidth: "58%",
    color: "var(--color-text-secondary)",
    fontSize: "13px",
    textAlign: "right",
    overflowWrap: "anywhere",
    "@media (max-width: 680px)": {
      maxWidth: "100%",
      textAlign: "left",
    },
  },
  footer: {
    display: "flex",
    gap: "8px",
    padding: "10px 12px",
    borderBlockStartWidth: "1px",
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--color-border)",
    // Footer-only groups (dictation smoke, surfaces) have no row to separate
    // from, so the first-child footer drops the divider line.
    ":first-child": {
      borderBlockStartWidth: 0,
    },
  },
  footerSplit: {
    flexWrap: "wrap",
    justifyContent: "space-between",
  },
});
