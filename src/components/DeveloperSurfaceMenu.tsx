import type { Surface } from "../types";

const surfaces: Array<{ id: Surface; label: string }> = [
  { id: "flow-bar", label: "Flow Bar" },
  { id: "writing-tools", label: "Writing Tools" },
  { id: "settings", label: "Settings" },
  { id: "onboarding", label: "Onboarding" },
];

export function DeveloperSurfaceMenu({ current }: { readonly current: Surface }) {
  if (!import.meta.env.DEV || new URLSearchParams(window.location.search).get("harness") !== "1") return null;
  return (
    <nav aria-label="Development surfaces" className="developer-menu">
      {surfaces.map((surface) => (
        <a aria-current={surface.id === current ? "page" : undefined} href={`?surface=${surface.id}&harness=1`} key={surface.id}>{surface.label}</a>
      ))}
      {current === "flow-bar" ? (
        <span className="developer-menu__states">
          <a href="?surface=flow-bar&state=listening&harness=1">Listen</a>
          <a href="?surface=flow-bar&state=processing&harness=1">Process</a>
          <a href="?surface=flow-bar&state=error&harness=1">Error</a>
        </span>
      ) : null}
    </nav>
  );
}
