import * as stylex from "@stylexjs/stylex";
import type { PermissionState } from "../types";

const pulse = stylex.keyframes({
  "50%": { opacity: ".35" },
});

const styles = stylex.create({
  indicator: {
    display: "inline-flex",
    alignItems: "center",
    gap: "7px",
    whiteSpace: "nowrap",
    color: "var(--text-secondary)",
  },
  dot: {
    width: "7px",
    height: "7px",
    borderRadius: "50%",
    backgroundColor: "var(--text-tertiary)",
  },
  positive: {
    backgroundColor: "var(--success)",
  },
  negative: {
    backgroundColor: "var(--danger)",
  },
  testing: {
    backgroundColor: "var(--accent)",
    animationName: pulse,
    animationDuration: "1s",
    animationTimingFunction: "ease-in-out",
    animationIterationCount: "infinite",
  },
});

type StatusState =
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
  readonly state: StatusState;
}

function indicatorTone(state: StatusState): "positive" | "neutral" | "negative" {
  if (state === "granted" || state === "connected") return "positive";
  if (state === "not-determined" || state === "untested" || state === "testing") return "neutral";
  return "negative";
}

export function StatusIndicator({ label, state }: StatusIndicatorProps) {
  const tone = indicatorTone(state);
  const container = stylex.props(styles.indicator);
  const dot = stylex.props(
    styles.dot,
    tone === "positive" && styles.positive,
    tone === "negative" && styles.negative,
    state === "testing" && styles.testing,
  );
  return (
    <span {...container}>
      <span aria-hidden="true" {...dot} />
      {label}
      <span className="sr-only">({humanState(state)})</span>
    </span>
  );
}

function humanState(state: StatusState): string {
  switch (state) {
    case "granted": return "allowed";
    case "denied": return "denied";
    case "not-determined": return "not determined";
    case "unavailable": return "not required";
    case "connected": return "connected";
    case "testing": return "testing";
    case "invalid": return "key not accepted";
    case "untested": return "not tested";
    case "offline": return "offline";
    case "rate-limited": return "rate limited";
    case "model": return "model unavailable";
    case "blocked": return "provider error";
  }
}
