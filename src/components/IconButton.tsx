import * as stylex from "@stylexjs/stylex";
import type { ButtonHTMLAttributes, ReactNode } from "react";

const styles = stylex.create({
  iconButton: {
    display: "grid",
    placeItems: "center",
    width: "26px",
    height: "26px",
    padding: 0,
    borderWidth: 0,
    borderRadius: "6px",
    backgroundColor: "transparent",
    color: "var(--text-secondary)",
    cursor: "pointer",
    transitionProperty: "color, background-color, transform",
    transitionDuration: ".12s",
    ":hover": { color: "var(--text)", backgroundColor: "var(--hover)" },
    ":active": { transform: "scale(.94)" },
    ":disabled": { opacity: ".48", cursor: "default" },
  },
});

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  readonly label: string;
  readonly children: ReactNode;
}

/** Compact icon-only control (close, back, dismiss). */
export function IconButton({ label, children, className, ...props }: IconButtonProps) {
  const sx = stylex.props(styles.iconButton);
  return (
    <button
      {...sx}
      aria-label={label}
      className={[sx.className, "kv-icon-button", className].filter(Boolean).join(" ") || undefined}
      type="button"
      {...props}
    >
      {children}
    </button>
  );
}
