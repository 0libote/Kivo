import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { useCallback, useEffect, useState } from "react";
import { Icon } from "../../components/Icon";
import { SegmentedControl } from "../../components/SegmentedControl";
import { StatusIndicator } from "../../components/StatusIndicator";
import { Switch } from "../../components/Switch";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge, type UpdateResult } from "../../platform/native";
import {
  type AppContext,
  type AppSettings,
  NativeError,
  type PermissionKind,
  type PermissionStatus,
} from "../../types";
import { AboutPreferences } from "./AboutPreferences";
import { type SaveSettings, SettingRow, SettingsContent, SettingsGroup } from "./settings-layout";
import { styles } from "./settings-styles";
export function SystemSettingsSection({
  busy,
  context,
  setBusy,
  setNotice,
  setUpdateResult,
  settings,
  save,
  updateResult,
}: {
  readonly busy: string | null;
  readonly context: AppContext;
  readonly setBusy: (value: string | null) => void;
  readonly setNotice: (value: string | null) => void;
  readonly setUpdateResult: (value: UpdateResult) => void;
  readonly settings: AppSettings;
  readonly save: SaveSettings;
  readonly updateResult: UpdateResult | null;
}) {
  return (
    <SettingsContent title="Settings" subtitle="Appearance, startup, permissions, and updates.">
      <GeneralPreferences settings={settings} save={save} />
      <PermissionsPreferences context={context} setNotice={setNotice} />
      <AboutPreferences
        busy={busy}
        context={context}
        setBusy={setBusy}
        setNotice={setNotice}
        setUpdateResult={setUpdateResult}
        updateResult={updateResult}
      />
    </SettingsContent>
  );
}

function PermissionsPreferences({
  context,
  setNotice,
}: {
  readonly context: AppContext;
  readonly setNotice: (value: string | null) => void;
}) {
  const [permissions, setPermissions] = useState<PermissionStatus[]>([]);
  const [busy, setBusy] = useState<PermissionKind | null>(null);
  const [loaded, setLoaded] = useState(false);
  useNativeEvent<PermissionStatus[]>("permission-status-changed", setPermissions);

  const refresh = useCallback(() => {
    void nativeBridge
      .getPermissions()
      .then((next) => {
        setPermissions(next);
      })
      .catch(() => setNotice("Permission status isn’t available right now."))
      .finally(() => setLoaded(true));
  }, [setNotice]);

  useEffect(refresh, [refresh]);

  // Re-read microphone access when returning from Windows Settings.
  useEffect(() => {
    const poll = () => {
      void nativeBridge
        .getPermissions()
        .then(setPermissions)
        .catch(() => {});
    };
    const interval = window.setInterval(poll, 2500);
    window.addEventListener("focus", poll);
    return () => {
      window.clearInterval(interval);
      window.removeEventListener("focus", poll);
    };
  }, []);

  async function request(kind: PermissionKind) {
    setBusy(kind);
    setNotice(null);
    try {
      // Windows has no in-app prompt: open the Settings page, then re-read
      // the (possibly changed) state instead of firing a no-op request.
      if (context.platform === "windows") {
        await nativeBridge.openPermissionSettings(kind);
        setPermissions(await nativeBridge.getPermissions());
      } else {
        setPermissions(await nativeBridge.requestPermission(kind));
      }
    } catch (error) {
      setNotice(
        error instanceof NativeError ? error.message : "Permission wasn’t granted. Try again.",
      );
      refresh();
    } finally {
      setBusy(null);
    }
  }

  async function openSettings(kind: PermissionKind) {
    setBusy(kind);
    setNotice(null);
    try {
      await nativeBridge.openPermissionSettings(kind);
      setPermissions(await nativeBridge.getPermissions());
    } catch {
      setNotice("The system settings page couldn’t be opened.");
    } finally {
      setBusy(null);
    }
  }

  let subtitle: string;
  if (context.platform === "windows") {
    subtitle =
      "Microphone access is managed in Windows Settings. Text access needs no extra prompt on Windows.";
  } else {
    subtitle =
      "Linux test bench: microphone and speech are simulated, text access needs no extra prompt.";
  }
  const order: PermissionKind[] = [
    "accessibility",
    "input-monitoring",
    "microphone",
    "speech-recognition",
  ];
  const byKind = new Map(permissions.map((permission) => [permission.kind, permission]));
  return (
    <>
      <p {...stylex.props(styles.note)}>{subtitle}</p>
      <SettingsGroup header="Permissions">
        {order.map((kind) => {
          const status = byKind.get(kind);
          const state = status?.state ?? "not-determined";
          if (state === "unavailable") {
            return (
              <SettingRow
                key={kind}
                label={permissionLabel(kind)}
                description={status?.explanation ?? "Not required on this system."}
              >
                <span {...stylex.props(styles.granted)}>Not required</span>
              </SettingRow>
            );
          }
          const granted = state === "granted";
          const denied = state === "denied";
          return (
            <SettingRow
              key={kind}
              label={permissionLabel(kind)}
              description={status?.explanation ?? permissionBlurb(kind)}
            >
              <span {...stylex.props(styles.permissionControl)}>
                <StatusIndicator label={permissionLabel(kind)} state={state} />
                {granted ? (
                  <span {...stylex.props(styles.granted)}>
                    <Icon name="check" size={15} />
                    Allowed
                  </span>
                ) : (
                  <Button
                    isDisabled={busy !== null || !loaded}
                    label={permissionActionLabel(busy === kind, denied)}
                    onClick={() => void (denied ? openSettings(kind) : request(kind))}
                    size="sm"
                    variant="secondary"
                  />
                )}
              </span>
            </SettingRow>
          );
        })}
        <div {...stylex.props(styles.groupFooter, styles.groupFooterSplit)}>
          <Button
            isDisabled={busy !== null}
            label="Refresh status"
            onClick={refresh}
            size="sm"
            variant="secondary"
          />
        </div>
      </SettingsGroup>
    </>
  );
}

function permissionActionLabel(waiting: boolean, denied: boolean): string {
  if (waiting) return "Waiting…";
  if (denied) return "Open Settings";
  return "Allow";
}

function permissionLabel(kind: PermissionKind): string {
  switch (kind) {
    case "accessibility":
      return "Accessibility";
    case "input-monitoring":
      return "Shortcut monitoring";
    case "microphone":
      return "Microphone";
    case "speech-recognition":
      return "Speech Recognition";
  }
}

function permissionBlurb(kind: PermissionKind): string {
  switch (kind) {
    case "accessibility":
      return "Work with the selected text and cursor in your active app.";
    case "input-monitoring":
      return "Detect the dictation hold shortcut.";
    case "microphone":
      return "Listen only while dictation is active.";
    case "speech-recognition":
      return "Transcribe speech using the operating system.";
  }
}

function GeneralPreferences({
  settings,
  save,
}: {
  readonly settings: AppSettings;
  readonly save: SaveSettings;
}) {
  return (
    <SettingsGroup header="General">
      <SettingRow label="Launch at login" description="Start Kivo automatically after you sign in.">
        <Switch
          checked={settings.launchAtLogin}
          label="Launch at login"
          onChange={(value) => void save({ launchAtLogin: value })}
        />
      </SettingRow>
      <SettingRow label="Appearance">
        <SegmentedControl
          ariaLabel="Appearance"
          onChange={(theme) => void save({ theme })}
          options={[
            { label: "System", value: "system" },
            { label: "Light", value: "light" },
            { label: "Dark", value: "dark" },
          ]}
          value={settings.theme}
        />
      </SettingRow>
      <SettingRow
        label="Show Flow Bar while idle"
        description="Keep a quiet indicator visible between dictations."
      >
        <Switch
          checked={settings.showIdleFlowBar}
          label="Show Flow Bar while idle"
          onChange={(value) => void save({ showIdleFlowBar: value })}
        />
      </SettingRow>
      <SettingRow
        label="Start in background"
        description="Keep the main window closed when Kivo starts."
      >
        <Switch
          checked={settings.startInBackground}
          label="Start in background"
          onChange={(value) => void save({ startInBackground: value })}
        />
      </SettingRow>
    </SettingsGroup>
  );
}
