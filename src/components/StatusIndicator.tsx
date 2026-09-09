import type { PermissionState } from "../types";

interface StatusIndicatorProps {
  readonly label: string;
  readonly state: PermissionState | "connected" | "testing" | "invalid" | "untested" | "offline" | "rate-limited";
}

function indicatorTone(state: StatusIndicatorProps["state"]): "positive" | "neutral" | "negative" {
  if (state === "granted" || state === "connected") return "positive";
  if (state === "not-determined" || state === "untested" || state === "testing") return "neutral";
  return "negative";
}

export function StatusIndicator({ label, state }: StatusIndicatorProps) {
  return (
    <span className="status-indicator" data-state={indicatorTone(state)}>
      <span aria-hidden="true" className="status-indicator__dot" />
      {label}
    </span>
  );
}
