import * as stylex from "@stylexjs/stylex";
import type { Surface } from "../types";

const surfaces: Array<{ id: Surface; label: string }> = [
  { id: "gallery", label: "Bench" },
  { id: "flow-bar", label: "Flow Bar" },
  { id: "writing-tools", label: "Writing Tools" },
  { id: "settings", label: "Settings" },
  { id: "onboarding", label: "Onboarding" },
];

export function DeveloperSurfaceMenu({ current }: { readonly current: Surface }) {
  if (!import.meta.env.DEV || new URLSearchParams(window.location.search).get("harness") !== "1")
    return null;
  return (
    <nav aria-label="Development surfaces" {...stylex.props(styles.menu)}>
      {surfaces.map((surface) => (
        <a
          aria-current={surface.id === current ? "page" : undefined}
          href={`?surface=${surface.id}&harness=1`}
          key={surface.id}
          {...stylex.props(styles.link, surface.id === current && styles.current)}
        >
          {surface.label}
        </a>
      ))}
      {current === "flow-bar" ? (
        <span {...stylex.props(styles.states)}>
          <a href="?surface=flow-bar&state=listening&harness=1" {...stylex.props(styles.link)}>
            Listen
          </a>
          <a href="?surface=flow-bar&state=processing&harness=1" {...stylex.props(styles.link)}>
            Process
          </a>
          <a href="?surface=flow-bar&state=error&harness=1" {...stylex.props(styles.link)}>
            Error
          </a>
        </span>
      ) : null}
    </nav>
  );
}

const styles = stylex.create({
  menu: {
    position: "fixed",
    zIndex: 1000,
    right: "12px",
    top: "10px",
    display: "flex",
    gap: "3px",
    padding: "4px",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "7px",
    boxShadow: "var(--shadow-med)",
  },
  link: {
    padding: "4px 7px",
    color: "var(--color-text-secondary)",
    textDecoration: "none",
    fontSize: "11px",
    borderRadius: "4px",
    ":hover": {
      backgroundColor: "var(--color-background-muted)",
      color: "var(--color-text-primary)",
    },
  },
  current: {
    backgroundColor: "var(--color-background-muted)",
    color: "var(--color-text-primary)",
  },
  states: {
    display: "flex",
    paddingInlineStart: "3px",
    borderInlineStartWidth: "1px",
    borderInlineStartStyle: "solid",
    borderInlineStartColor: "var(--color-border)",
  },
});
