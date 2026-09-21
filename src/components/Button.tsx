import * as stylex from "@stylexjs/stylex";
import type { AnchorHTMLAttributes, ButtonHTMLAttributes, ReactNode } from "react";
import { Icon, type IconName } from "./Icon";

const styles = stylex.create({
  base: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    gap: "7px",
    minHeight: "32px",
    paddingBlock: 0,
    paddingInline: "13px",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--border-strong)",
    borderRadius: "var(--radius-control)",
    backgroundColor: "var(--surface-strong)",
    color: "inherit",
    font: "inherit",
    fontWeight: 620,
    letterSpacing: "-0.01em",
    textDecoration: "none",
    cursor: "pointer",
    transitionProperty: "background-color, border-color, transform, box-shadow",
    transitionDuration: ".12s",
    ":hover": { backgroundColor: "var(--surface-muted)" },
    ":active": { transform: "scale(.98)" },
    ":disabled": { opacity: ".48", cursor: "default" },
  },
  compact: {
    minHeight: "28px",
    paddingInline: "10px",
    fontSize: "12px",
  },
  primary: {
    backgroundColor: "var(--accent)",
    color: "var(--accent-text)",
    borderColor: "transparent",
    ":hover": { backgroundColor: "color-mix(in srgb, var(--accent) 88%, var(--text))" },
  },
  danger: {
    color: "var(--danger)",
    backgroundColor: "transparent",
    borderColor: "transparent",
    ":hover": { backgroundColor: "var(--hover)" },
  },
});

type ButtonTone = "default" | "primary" | "danger";

function toneStyles(tone: ButtonTone) {
  if (tone === "primary") return styles.primary;
  if (tone === "danger") return styles.danger;
  return null;
}

function joinClass(stylexClass: string | undefined, ...rest: (string | undefined | false)[]) {
  return [stylexClass, ...rest].filter(Boolean).join(" ") || undefined;
}

/** Stable, unhashed classes so contextual CSS can still target the button. */
function stableClasses(tone: ButtonTone, compact: boolean) {
  return joinClass("kv-button", compact && "kv-button--compact", tone !== "default" && `kv-button--${tone}`);
}

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  readonly children?: ReactNode;
  readonly icon?: IconName;
  readonly tone?: ButtonTone;
  readonly compact?: boolean;
}

export function Button({ children, icon, tone = "default", compact = false, className, ...props }: ButtonProps) {
  const sx = stylex.props(styles.base, compact && styles.compact, toneStyles(tone));
  return (
    <button
      {...sx}
      className={joinClass(sx.className, stableClasses(tone, compact), className)}
      type="button"
      {...props}
    >
      {icon ? <Icon name={icon} size={compact ? 14 : 16} /> : null}
      {children == null ? null : <span>{children}</span>}
    </button>
  );
}

interface LinkButtonProps extends AnchorHTMLAttributes<HTMLAnchorElement> {
  readonly children?: ReactNode;
  readonly icon?: IconName;
  readonly compact?: boolean;
}

/** Anchor styled as a button (navigation, not an action). */
export function LinkButton({ children, icon, compact = false, className, ...props }: LinkButtonProps) {
  const sx = stylex.props(styles.base, compact && styles.compact);
  return (
    <a
      {...sx}
      className={joinClass(sx.className, compact && "kv-button--compact", "kv-button", className)}
      {...props}
    >
      {icon ? <Icon name={icon} size={compact ? 14 : 16} /> : null}
      {children == null ? null : <span>{children}</span>}
    </a>
  );
}
