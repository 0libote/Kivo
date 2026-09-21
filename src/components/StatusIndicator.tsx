import type { PermissionState } from "../types";

interface StatusIndicatorProps {
  readonly label: string;
  readonly state: PermissionState | "connected" | "testing" | "invalid" | "untested" | "offline" | "rate-limited" | "model" | "blocked";
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
      <span className="sr-only">({humanState(state)})</span>
    </span>
  );
}

function humanState(state: StatusIndicatorProps["state"]): string {
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
    case "blocked": return "provider rejected";
  }
}
