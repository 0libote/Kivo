import type { StatPoint } from "./StatChart";
import { StatChart } from "./StatChart";

export function StatLineChart(props: Readonly<{ data: StatPoint[]; label: string }>) {
  return <StatChart {...props} kind="line" />;
}
