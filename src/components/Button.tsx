import type { ButtonHTMLAttributes, ReactNode } from "react";
import { Icon, type IconName } from "./Icon";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  readonly children: ReactNode;
  readonly icon?: IconName;
  readonly tone?: "default" | "primary" | "danger";
  readonly compact?: boolean;
}

export function Button({ children, icon, tone = "default", compact = false, className = "", ...props }: ButtonProps) {
  return (
    <button
      className={`button button--${tone}${compact ? " button--compact" : ""} ${className}`.trim()}
      type="button"
      {...props}
    >
      {icon ? <Icon name={icon} size={compact ? 14 : 16} /> : null}
      <span>{children}</span>
    </button>
  );
}
