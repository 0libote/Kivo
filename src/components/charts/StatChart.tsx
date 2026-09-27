import * as stylex from "@stylexjs/stylex";
import { barY } from "@tanstack/charts/bar";
import { lineY } from "@tanstack/charts/line";
import { Chart } from "@tanstack/charts/react/tooltip";
import { defineChart } from "@tanstack/charts/scene";
import { scaleLinear, scalePoint } from "d3-scale";
import { useReducedMotion } from "motion/react";
import { useMemo } from "react";

export interface StatPoint {
  date: string;
  value: number;
}

export function StatChart({
  data,
  kind,
  label,
}: Readonly<{ data: StatPoint[]; kind: "bar" | "line"; label: string }>) {
  const reduceMotion = useReducedMotion();
  const definition = useMemo(
    () =>
      defineChart(() => ({
        marks: [
          kind === "bar"
            ? barY(data, { x: "date", y: "value" })
            : lineY(data, { x: "date", y: "value", points: true }),
        ],
        scales: { x: { scale: scalePoint<string>() }, y: { scale: scaleLinear } },
        svgAnimation: !reduceMotion,
      })),
    [data, kind, reduceMotion],
  );
  return (
    <div {...stylex.props(styles.host)}>
      <Chart
        ariaLabel={label}
        definition={definition}
        height={220}
        renderTooltipBody={({ defaultBody }) => (
          <div {...stylex.props(styles.tooltip)}>{defaultBody}</div>
        )}
      />
    </div>
  );
}

const styles = stylex.create({
  host: {
    width: "100%",
    color: "var(--color-text-primary)",
    "--ts-chart-1": "var(--color-accent)",
  },
  tooltip: {
    color: "var(--color-text-primary)",
    backgroundColor: "var(--color-background-card)",
    padding: "var(--spacing-2)",
    borderRadius: "var(--radius-element)",
  },
});
