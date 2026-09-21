import { useEffect, useMemo, useState } from "react";
import * as stylex from "@stylexjs/stylex";
import { Badge } from "@astryxdesign/core/Badge";
import { Card } from "@astryxdesign/core/Card";
import { Heading } from "@astryxdesign/core/Heading";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { barY, defineChart } from "@tanstack/charts";
import { scaleBand } from "@tanstack/charts/scales/band";
import { scaleLinear } from "@tanstack/charts/scales/linear";
import { tooltip } from "@tanstack/charts/tooltip";
import { Chart } from "@tanstack/charts/react";
import { Button } from "../../components/Button";
import { nativeBridge } from "../../platform/native";
import type { UsageGroup, UsageSummary } from "../../types";

/** Window shown by default; the ledger keeps more but the card stays recent. */
const DEFAULT_WINDOW_DAYS = 30;

const numberFormat = new Intl.NumberFormat();

function formatCost(value: number): string {
  if (value <= 0) return "$0.00";
  if (value < 0.01) return "<$0.01";
  return `$${value.toFixed(2)}`;
}

const KIND_LABELS: Record<string, string> = {
  writing: "Writing Tools",
  "dictation-cleanup": "Dictation cleanup",
  dictation: "Dictation",
  "link-summary": "Link summaries",
  "connection-test": "Connection tests",
};

/**
 * Local AI usage dashboard. Everything shown is derived from the on-device
 * ledger in Rust (counts and identifiers only); Kivo never stores or sends
 * prompt, response, or transcript text.
 */
export function UsagePanel() {
  const [summary, setSummary] = useState<UsageSummary | null>(null);
  const [error, setError] = useState("");
  const [clearing, setClearing] = useState(false);

  async function reload() {
    try {
      setSummary(await nativeBridge.getUsageStats(DEFAULT_WINDOW_DAYS));
      setError("");
    } catch {
      setError("Usage statistics could not be loaded.");
    }
  }

  useEffect(() => {
    void reload();
  }, []);

  const days = useMemo(
    () => (summary?.days ?? []).map((day) => ({ day: day.day.slice(5), requests: day.requests })),
    [summary],
  );

  const chart = useMemo(() => {
    if (days.length === 0) return null;
    return defineChart({
      marks: [barY(days, { x: "day", y: "requests" })],
      scales: {
        x: { scale: () => scaleBand().padding(0.22) },
        y: { scale: scaleLinear, nice: true, grid: true, axis: { label: "Requests" } },
      },
      tooltip,
    });
  }, [days]);

  async function clear() {
    setClearing(true);
    try {
      await nativeBridge.clearUsageStats();
      await reload();
    } catch {
      setError("Usage statistics could not be cleared.");
    } finally {
      setClearing(false);
    }
  }

  const monthPrefix = new Date().toISOString().slice(0, 7);
  const monthCost = (summary?.days ?? []).reduce(
    (total, day) => (day.day.startsWith(monthPrefix) ? total + day.costUsd : total),
    0,
  );

  return (
    <Card padding={4}>
      <Stack direction="vertical" gap={3}>
        <Stack direction="horizontal" gap={3} align="center">
          <Heading level={2}>AI usage</Heading>
          <Badge label="On this device" />
        </Stack>
        <Text color="secondary" size="sm">
          Counts, tokens, and estimated cost only. Nothing here leaves your computer.
        </Text>

        {error ? (
          <Text color="accent" size="sm" role="status">
            {error}
          </Text>
        ) : null}

        {summary == null ? (
          <Text color="secondary" size="sm">
            Loading usage…
          </Text>
        ) : summary.requests === 0 ? (
          <Text color="secondary" size="sm">
            No AI usage in the last {DEFAULT_WINDOW_DAYS} days yet.
          </Text>
        ) : (
          <>
            <div {...stylex.props(styles.metrics)}>
              <Metric label="Requests" value={numberFormat.format(summary.requests)} />
              <Metric label="Tokens" value={numberFormat.format(summary.inputTokens + summary.outputTokens)} />
              <Metric label="Est. cost" value={formatCost(summary.costUsd)} />
              <Metric label="This month" value={formatCost(monthCost)} />
              {summary.dictationWords > 0 ? (
                <Metric label="Words dictated" value={numberFormat.format(summary.dictationWords)} />
              ) : null}
            </div>
            <div {...stylex.props(styles.footnoteRow)}>
              <Text color="secondary" size="2xs">
                {numberFormat.format(summary.inputTokens)} in · {numberFormat.format(summary.outputTokens)} out
                {summary.failures > 0 ? ` · ${numberFormat.format(summary.failures)} failed` : ""}
              </Text>
              {summary.estimatedRequests > 0 ? (
                <Text color="secondary" size="2xs">
                  {numberFormat.format(summary.estimatedRequests)} estimated
                </Text>
              ) : null}
            </div>

            {chart ? (
              <Chart ariaLabel="Requests per day" definition={chart} height={180} />
            ) : null}

            <Breakdown title="By model" groups={summary.byModel} />
            <Breakdown
              title="By task"
              groups={summary.byKind.map((group) => ({
                ...group,
                key: KIND_LABELS[group.key] ?? group.key,
              }))}
            />
            <Breakdown title="By provider" groups={summary.byProvider} />

            <div {...stylex.props(styles.actions)}>
              <Button compact disabled={clearing} onClick={() => void clear()} tone="danger">
                {clearing ? "Clearing…" : "Clear usage history"}
              </Button>
            </div>
          </>
        )}
      </Stack>
    </Card>
  );
}

function Metric({ label, value }: { readonly label: string; readonly value: string }) {
  return (
    <div {...stylex.props(styles.metric)}>
      <Text color="secondary" size="2xs">
        {label}
      </Text>
      <span {...stylex.props(styles.metricValue)}>{value}</span>
    </div>
  );
}

function Breakdown({ title, groups }: { readonly title: string; readonly groups: UsageGroup[] }) {
  const visible = groups.slice(0, 4);
  if (visible.length === 0) return null;
  return (
    <Stack direction="vertical" gap={1}>
      <Text color="secondary" size="2xs" weight="semibold">
        {title}
      </Text>
      {visible.map((group) => (
        <div key={group.key} {...stylex.props(styles.row)}>
          <span {...stylex.props(styles.rowLabel)}>{group.key}</span>
          <span {...stylex.props(styles.muted)}>
            {numberFormat.format(group.requests)} · {formatCost(group.costUsd)}
          </span>
        </div>
      ))}
    </Stack>
  );
}

const styles = stylex.create({
  metrics: {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fit, minmax(120px, 1fr))",
    gap: "12px",
  },
  metric: {
    display: "grid",
    gap: "2px",
  },
  metricValue: {
    fontSize: "22px",
    fontWeight: 640,
    letterSpacing: "-0.02em",
  },
  footnoteRow: {
    display: "flex",
    justifyContent: "space-between",
    gap: "8px",
    flexWrap: "wrap",
  },
  row: {
    display: "flex",
    justifyContent: "space-between",
    gap: "8px",
  },
  rowLabel: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  muted: {
    color: "var(--text-secondary)",
    flexShrink: 0,
  },
  actions: {
    display: "flex",
    justifyContent: "flex-end",
  },
});
