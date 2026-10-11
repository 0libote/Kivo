import * as stylex from "@stylexjs/stylex";
import { type ReactNode } from "react";
import { type AppSettings } from "../../types";
import { styles } from "./settings-styles";
export type SaveSettings = (patch: Partial<AppSettings>) => Promise<void>;
export function SettingsContent({
  title,
  subtitle,
  children,
}: {
  readonly title: string;
  readonly subtitle: string;
  readonly children: ReactNode;
}) {
  return (
    <section {...stylex.props(styles.content)}>
      <header {...stylex.props(styles.contentHeader)}>
        <h1 {...stylex.props(styles.contentTitle)}>{title}</h1>
        <p {...stylex.props(styles.contentSubtitle)}>{subtitle}</p>
      </header>
      {children}
    </section>
  );
}

export function SettingsGroup({
  header,
  children,
}: {
  readonly header?: string;
  readonly children: ReactNode;
}) {
  return (
    <section {...stylex.props(styles.group)}>
      {header ? <h2 {...stylex.props(styles.groupHeader)}>{header}</h2> : null}
      <div {...stylex.props(styles.groupBody)}>{children}</div>
    </section>
  );
}

export function SettingRow({
  label,
  description,
  children,
  stacked = false,
}: {
  readonly label: string;
  readonly description?: string;
  readonly children: ReactNode;
  readonly stacked?: boolean;
}) {
  return (
    <div data-stacked={stacked} {...stylex.props(styles.row, stacked && styles.rowStacked)}>
      <div {...stylex.props(styles.rowLabel)}>
        <strong {...stylex.props(styles.rowTitle)}>{label}</strong>
        {description ? <span {...stylex.props(styles.rowDescription)}>{description}</span> : null}
      </div>
      <div {...stylex.props(styles.rowControl, stacked && styles.rowControlStacked)}>
        {children}
      </div>
    </div>
  );
}
