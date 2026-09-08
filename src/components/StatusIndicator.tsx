import type { PermissionState } from "../types";

interface StatusIndicatorProps {
  label: string;
  state: PermissionState | "connected" | "testing" | "invalid" | "untested" | "offline" | "rate-limited";
}

export function StatusIndicator({ label, state }: StatusIndicatorProps) {
  const positive = state === "granted" || state === "connected";
  const pending = state === "not-determined" || state === "untested" || state === "testing";
  return (
    <span className="status-indicator" data-state={positive ? "positive" : pending ? "neutral" : "negative"}>
      <span aria-hidden="true" className="status-indicator__dot" />
      {label}
    </span>
  );
}
