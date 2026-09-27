import * as stylex from "@stylexjs/stylex";
import { lineY } from "@tanstack/charts/line";
import { Chart } from "@tanstack/charts/react/tooltip";
import { defineChart } from "@tanstack/charts/scene";
import { scaleLinear, scalePoint } from "d3-scale";
import { useReducedMotion } from "motion/react";
import { useMemo } from "react";
import type { StatPoint } from "./StatBarChart";

export function StatLineChart({ data, label }: { data: StatPoint[]; label: string }) {
  const reduceMotion = useReducedMotion();
  const definition = useMemo(
    () =>
      defineChart(() => ({
        marks: [lineY(data, { x: "date", y: "value", points: true })],
        scales: { x: { scale: scalePoint<string>() }, y: { scale: scaleLinear } },
        svgAnimation: !reduceMotion,
      })),
    [data, reduceMotion],
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
