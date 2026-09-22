import { StatusDot } from "@astryxdesign/core/StatusDot";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import * as stylex from "@stylexjs/stylex";
import type { PermissionState } from "../types";

type IndicatorState =
  | PermissionState
  | "connected"
  | "testing"
  | "invalid"
  | "untested"
  | "offline"
  | "rate-limited"
  | "model"
  | "blocked";

interface StatusIndicatorProps {
  readonly label: string;
  readonly state: IndicatorState;
}

interface IndicatorStyle {
  readonly variant: "success" | "error" | "accent" | "neutral";
  readonly isPulsing: boolean;
}

function indicatorStyle(state: IndicatorState): IndicatorStyle {
  switch (state) {
    case "granted":
    case "connected":
      return { variant: "success", isPulsing: false };
    case "testing":
      return { variant: "accent", isPulsing: true };
    case "not-determined":
    case "untested":
    case "unavailable":
      return { variant: "neutral", isPulsing: false };
    default:
      return { variant: "error", isPulsing: false };
  }
}

export function StatusIndicator({ label, state }: StatusIndicatorProps) {
  const { variant, isPulsing } = indicatorStyle(state);
  return (
    <span {...stylex.props(styles.root)}>
      <StatusDot isPulsing={isPulsing} label={label} variant={variant} />
      {label}
      <VisuallyHidden>({humanState(state)})</VisuallyHidden>
    </span>
  );
}

function humanState(state: IndicatorState): string {
  switch (state) {
    case "granted":
      return "allowed";
    case "denied":
      return "denied";
    case "not-determined":
      return "not determined";
    case "unavailable":
      return "not required";
    case "connected":
      return "connected";
    case "testing":
      return "testing";
    case "invalid":
      return "key not accepted";
    case "untested":
      return "not tested";
    case "offline":
      return "offline";
    case "rate-limited":
      return "rate limited";
    case "model":
      return "model unavailable";
    case "blocked":
      return "provider error";
  }
}

const styles = stylex.create({
  root: {
    display: "inline-flex",
    alignItems: "center",
    gap: "7px",
  },
});
