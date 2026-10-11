import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { Icon } from "../../components/Icon";
import { nativeBridge, type UpdateResult } from "../../platform/native";
import { type AppContext, NativeError } from "../../types";
import { SettingRow, SettingsGroup } from "./settings-layout";
import { styles } from "./settings-styles";
export function AboutPreferences({
  busy,
  context,
  setBusy,
  setNotice,
  setUpdateResult,
  updateResult,
}: {
  readonly busy: string | null;
  readonly context: AppContext;
  readonly setBusy: (value: string | null) => void;
  readonly setNotice: (value: string | null) => void;
  readonly setUpdateResult: (value: UpdateResult) => void;
  readonly updateResult: UpdateResult | null;
}) {
  const [installed, setInstalled] = useState(false);
  // Older builds without an updater key cannot install signed updates.
  const [installUnsupported, setInstallUnsupported] = useState(false);
  const updateAvailable = updateResult?.available === true;

  async function installUpdate() {
    setBusy("install");
    setNotice(null);
    try {
      await nativeBridge.installUpdate();
      setInstalled(true);
      setNotice("Update installed. Restarting…");
      setBusy("restart");
      try {
        await nativeBridge.restartApp();
        setNotice("Update installed. Restart Kivo to finish.");
      } catch {
        setNotice("Kivo couldn’t restart. Quit and reopen it manually.");
      } finally {
        setBusy(null);
      }
    } catch (error: unknown) {
      const code = error instanceof NativeError ? error.code : "";
      if (code === "update_install_unavailable" || code === "update_not_available") {
        setInstallUnsupported(true);
      }
      setNotice(
        error instanceof NativeError
          ? error.message
          : "The update couldn’t be installed. Use the download link instead.",
      );
      setBusy(null);
    }
  }

  return (
    <>
      <div {...stylex.props(styles.lockup)}>
        <div {...stylex.props(styles.lockupMark)}>
          <Icon name="audio" size={27} />
        </div>
        <div>
          <h2 {...stylex.props(styles.lockupTitle)}>Kivo</h2>
          <p {...stylex.props(styles.lockupVersion)}>Version {context.version}</p>
        </div>
      </div>
      <SettingsGroup header="About">
        <SettingRow
          label="Software updates"
          description={updateStatusDescription(installed, busy, updateResult)}
        >
          {installed ? (
            <Button
              isDisabled={busy !== null}
              label={busy === "restart" ? "Restarting…" : "Restart now"}
              onClick={() => {
                setBusy("restart");
                void nativeBridge
                  .restartApp()
                  .catch(() => setNotice("Kivo couldn’t restart. Quit and reopen it manually."))
                  .finally(() => setBusy(null));
              }}
              size="sm"
              variant="primary"
            />
          ) : (
            <Button
              isDisabled={busy !== null}
              label={busy === "updates" ? "Checking…" : "Check now"}
              onClick={() => {
                setBusy("updates");
                setNotice(null);
                setInstalled(false);
                void nativeBridge
                  .checkForUpdates()
                  .then(setUpdateResult)
                  .catch((error: unknown) =>
                    setNotice(
                      error instanceof NativeError
                        ? error.message
                        : "Kivo couldn’t check for updates right now.",
                    ),
                  )
                  .finally(() => setBusy(null));
              }}
              size="sm"
              variant="secondary"
            />
          )}
        </SettingRow>
        {updateAvailable && !installed && !installUnsupported ? (
          <SettingRow label="Install update" description={installUpdateDescription(updateResult)}>
            <Button
              isDisabled={busy !== null}
              label={installButtonLabel(busy)}
              onClick={() => void installUpdate()}
              size="sm"
              variant="primary"
            />
          </SettingRow>
        ) : null}
      </SettingsGroup>
      {updateResult?.available && !installed ? (
        <button
          onClick={() =>
            void nativeBridge
              .openExternal(updateResult.downloadUrl ?? "https://github.com/0libote/Kivo/releases")
              .catch(() => setNotice("The releases page couldn’t be opened."))
          }
          type="button"
          {...stylex.props(styles.textLink, styles.linkStart)}
        >
          {updateResult.channel === "beta"
            ? "Download the latest beta build from GitHub"
            : "Download the latest release from GitHub"}
        </button>
      ) : null}
      <div aria-label="Project links" {...stylex.props(styles.aboutLinks)}>
        <button
          onClick={() =>
            void nativeBridge
              .openExternal("https://github.com/0libote/Kivo")
              .catch(() => setNotice("The repository could not be opened."))
          }
          type="button"
          {...stylex.props(styles.textLink, styles.linkStart)}
        >
          Source code on GitHub
        </button>
        <button
          onClick={() =>
            void nativeBridge
              .openExternal("https://github.com/0libote/Kivo/releases")
              .catch(() => setNotice("The releases page could not be opened."))
          }
          type="button"
          {...stylex.props(styles.textLink, styles.linkStart)}
        >
          Release notes
        </button>
      </div>
      <p {...stylex.props(styles.privacyNote)}>
        No accounts, hosted analytics, or telemetry; your text is processed only when you invoke
        Kivo.
      </p>
    </>
  );
}

function updateStatusDescription(
  installed: boolean,
  busy: string | null,
  updateResult: UpdateResult | null,
): string {
  if (!installed) return updateDescription(updateResult);
  return busy === "restart"
    ? "Update installed. Restarting…"
    : "Update installed. Restart Kivo to finish.";
}

function installUpdateDescription(updateResult: UpdateResult): string {
  const version =
    updateResult.channel === "beta"
      ? "The latest beta build"
      : `Version ${updateResult.availableVersion}`;
  return `${version} can be installed without leaving Kivo.`;
}

function installButtonLabel(busy: string | null): string {
  if (busy === "install") return "Installing…";
  if (busy === "restart") return "Restarting…";
  return "Download and Install";
}

function updateDescription(updateResult: UpdateResult | null): string {
  if (updateResult === null) return "Check manually for a newer version.";
  if (updateResult.available) {
    if (updateResult.channel === "beta") {
      const sha = updateResult.availableSha?.slice(0, 7);
      return sha ? `A newer beta build is available (${sha}).` : "A newer beta build is available.";
    }
    return `Version ${updateResult.availableVersion} is available.`;
  }
  return "Kivo is up to date.";
}
