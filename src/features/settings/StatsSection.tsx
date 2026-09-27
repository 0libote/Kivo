import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { useCallback, useEffect, useState } from "react";
import { StatBarChart } from "../../components/charts/StatBarChart";
import { StatLineChart } from "../../components/charts/StatLineChart";
import { Switch } from "../../components/Switch";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import type { AiModelInfo, AppSettings, StatsDocument, StatsRow } from "../../types";

const empty: StatsDocument = { schemaVersion: 1, rows: [] };
type SaveSettings = (patch: Partial<AppSettings>) => Promise<void>;

function sum(
  rows: StatsRow[],
  key:
    | "count"
    | "ok"
    | "fail"
    | "charsIn"
    | "charsOut"
    | "msSum"
    | "tokensIn"
    | "tokensOut"
    | "costMicroUsd",
) {
  return rows.reduce((total, row) => total + (row[key] ?? 0), 0);
}
function dayKey(date: Date) {
  return date.toISOString().slice(0, 10);
}
function words(chars: number) {
  return Math.round(chars / 5);
}
function byDate(rows: StatsRow[], value: (row: StatsRow) => number, days: number, at: number) {
  const result: Array<{ date: string; value: number }> = [];
  for (let index = days - 1; index >= 0; index--) {
    const date = new Date(at);
    date.setUTCDate(date.getUTCDate() - index);
    const key = dayKey(date);
    result.push({
      date: key.slice(5),
      value: rows.filter((row) => row.date === key).reduce((total, row) => total + value(row), 0),
    });
  }
  return result;
}
function delta(current: number, previous: number) {
  if (previous === 0) return current ? "New this week" : "No change";
  const change = Math.round(((current - previous) / previous) * 100);
  return `${change > 0 ? "+" : ""}${change}% vs last week`;
}
function saveFile(name: string, type: string, content: string) {
  const url = URL.createObjectURL(new Blob([content], { type }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = name;
  anchor.click();
  URL.revokeObjectURL(url);
}

async function exportJson() {
  saveFile(
    "kivo-statistics.json",
    "application/json",
    JSON.stringify(await nativeBridge.exportStats(), null, 2),
  );
}
async function exportCsv() {
  const snapshot = await nativeBridge.exportStats();
  const header =
    "date,kind,action,provider,model,engine,errorCategory,count,ok,fail,cancel,failoversRescued,charsIn,charsOut,msSum,msMax,tokensIn,tokensOut,costMicroUsd";
  const lines = snapshot.rows.map((row) =>
    [
      row.date,
      row.kind,
      row.action,
      row.provider,
      row.model,
      row.engine,
      row.errorCategory,
      row.count,
      row.ok,
      row.fail,
      row.cancel,
      row.failoversRescued,
      row.charsIn,
      row.charsOut,
      row.msSum,
      row.msMax,
      row.tokensIn,
      row.tokensOut,
      row.costMicroUsd,
    ]
      .map((value) => JSON.stringify(value ?? ""))
      .join(","),
  );
  saveFile("kivo-statistics.csv", "text/csv", [header, ...lines].join("\n"));
}

export function StatsSection({ settings, save }: { settings: AppSettings; save: SaveSettings }) {
  const [stats, setStats] = useState(empty);
  const [now, setNow] = useState(0);
  const [goModels, setGoModels] = useState<AiModelInfo[]>([]);
  const [chartDays, setChartDays] = useState<14 | 30>(14);
  const refresh = useCallback(() => {
    void nativeBridge
      .getStatsSummary()
      .then(setStats)
      .catch(() => setStats(empty));
  }, []);
  useEffect(refresh, [refresh]);
  useEffect(() => {
    void nativeBridge
      .listAiModels("go")
      .then(setGoModels)
      .catch(() => setGoModels([]));
  }, []);
  useEffect(() => setNow(Date.now()), []);
  useNativeEvent("stats-changed", refresh);

  const rows = stats.rows;
  const today = dayKey(new Date(now));
  const todayRows = rows.filter((row) => row.date === today);
  const weekRows = rows.filter((row) => row.date >= dayKey(new Date(now - 6 * 86_400_000)));
  const previousRows = rows.filter(
    (row) =>
      row.date >= dayKey(new Date(now - 13 * 86_400_000)) &&
      row.date < dayKey(new Date(now - 6 * 86_400_000)),
  );
  const dictations = rows.filter((row) => row.kind === "dictation" && row.action === "dictate");
  const writing = rows.filter(
    (row) => row.kind === "writing" && row.action !== "applied" && row.action !== "kept",
  );
  const applied = sum(
    rows.filter((row) => row.action === "applied"),
    "count",
  );
  const kept = sum(
    rows.filter((row) => row.action === "kept"),
    "count",
  );
  const allChars = sum(dictations, "charsOut");
  const recentWords = words(
    sum(
      weekRows.filter((row) => row.kind === "dictation" && row.action === "dictate"),
      "charsOut",
    ),
  );
  const oldWords = words(
    sum(
      previousRows.filter((row) => row.kind === "dictation" && row.action === "dictate"),
      "charsOut",
    ),
  );
  const weekDictations = sum(
    weekRows.filter((row) => row.kind === "dictation" && row.action === "dictate"),
    "ok",
  );
  const previousDictations = sum(
    previousRows.filter((row) => row.kind === "dictation" && row.action === "dictate"),
    "ok",
  );
  const weekWriting = sum(
    weekRows.filter(
      (row) => row.kind === "writing" && row.action !== "applied" && row.action !== "kept",
    ),
    "count",
  );
  const previousWriting = sum(
    previousRows.filter(
      (row) => row.kind === "writing" && row.action !== "applied" && row.action !== "kept",
    ),
    "count",
  );
  const spend = sum(rows, "costMicroUsd") / 1_000_000;
  const monthStart = `${new Date(now).toISOString().slice(0, 7)}-01`;
  const goSpend =
    sum(
      rows.filter((row) => row.provider === "go" && row.date >= monthStart),
      "costMicroUsd",
    ) / 1_000_000;
  const goAllowance = Math.max(...goModels.map((model) => model.monthlyLimitUsd ?? 0), 0);
  const dailyWords = byDate(dictations, (row) => words(row.charsOut), chartDays, now);
  const dailyWriting = byDate(writing, (row) => row.ok, chartDays, now);
  const dailySpend = byDate(rows, (row) => row.costMicroUsd ?? 0, chartDays, now);
  const dailyLatency = byDate(
    writing,
    (row) => (row.count ? Math.round(row.msSum / row.count) : 0),
    chartDays,
    now,
  );

  const modelTotals = new Map<
    string,
    { count: number; ok: number; ms: number; cost: number; failovers: number }
  >();
  for (const row of writing) {
    const id = row.model ?? "Unknown model";
    const item = modelTotals.get(id) ?? { count: 0, ok: 0, ms: 0, cost: 0, failovers: 0 };
    item.count += row.count;
    item.ok += row.ok;
    item.ms += row.msSum;
    item.cost += row.costMicroUsd ?? 0;
    item.failovers += row.failoversRescued;
    modelTotals.set(id, item);
  }
  const models = [...modelTotals.entries()].sort((a, b) => b[1].count - a[1].count);
  const errors = new Map<string, number>();
  for (const row of rows) {
    if (row.errorCategory && row.fail > 0)
      errors.set(row.errorCategory, (errors.get(row.errorCategory) ?? 0) + row.fail);
  }

  return (
    <section {...stylex.props(styles.section)}>
      <header>
        <h2>Usage statistics</h2>
        <p>
          Private, text-free daily totals stored on this device. Token and cost values are
          estimates.
        </p>
      </header>
      <Switch
        checked={settings.statsEnabled}
        label="Track usage statistics on this device"
        onChange={(value) => void save({ statsEnabled: value })}
      />
      <label {...stylex.props(styles.control)}>
        Keep statistics
        <select
          aria-label="Statistics retention"
          disabled={!settings.statsEnabled}
          value={settings.statsRetentionDays}
          onChange={(event) => void save({ statsRetentionDays: Number(event.target.value) })}
        >
          <option value={0}>Forever</option>
          <option value={30}>30 days</option>
          <option value={90}>90 days</option>
          <option value={365}>1 year</option>
        </select>
      </label>
      {rows.length === 0 ? (
        <p>Nothing tracked yet — dictate something.</p>
      ) : (
        <>
          <div {...stylex.props(styles.metrics)}>
            <Metric
              title="Today"
              value={`≈ ${words(
                sum(
                  todayRows.filter((row) => row.kind === "dictation" && row.action === "dictate"),
                  "charsOut",
                ),
              ).toLocaleString()} words`}
              detail={`${delta(recentWords, oldWords)} · ${sum(todayRows, "ok")} completed today`}
            />
            <Metric
              title="This week"
              value={`≈ ${recentWords.toLocaleString()} words`}
              detail={delta(recentWords, oldWords)}
            />
            <Metric
              title="All time"
              value={`≈ ${words(allChars).toLocaleString()} words`}
              detail={`${delta(weekDictations, previousDictations)} · ${sum(dictations, "ok")} dictations`}
            />
            <Metric
              title="Typing time saved"
              value={`≈ ${Math.round(words(allChars) / 40)} min`}
              detail={`Estimated at 40 words per minute · ${delta(Math.round(recentWords / 40), Math.round(oldWords / 40))}`}
            />
          </div>
          {goAllowance > 0 && (
            <ChartPanel title="Go allowance">
              <p>
                ≈ ${goSpend.toFixed(4)} of ${goAllowance.toFixed(2)} monthly allowance used
              </p>
              <progress
                aria-label="Estimated Go monthly allowance used"
                max={goAllowance}
                value={Math.min(goSpend, goAllowance)}
              />
              <small>Estimate from recorded Go model usage this month.</small>
            </ChartPanel>
          )}
          <div {...stylex.props(styles.metrics)}>
            <Metric
              title="Dictations"
              value={sum(dictations, "ok").toLocaleString()}
              detail={`≈ ${words(allChars).toLocaleString()} words`}
            />
            <Metric
              title="Writing requests"
              value={sum(writing, "count").toLocaleString()}
              detail={`${sum(writing, "ok")} completed · ${delta(weekWriting, previousWriting)}`}
            />
            <Metric
              title="Estimated spend"
              value={`≈ $${spend.toFixed(4)}`}
              detail="Provider pricing estimate"
            />
            <Metric
              title="Tokens"
              value={`≈ ${(sum(rows, "tokensIn") + sum(rows, "tokensOut") || 0).toLocaleString()}`}
              detail="Input + output estimate"
            />
            <Metric
              title="Writing results applied"
              value={`${applied.toLocaleString()} / ${(applied + kept).toLocaleString()}`}
              detail="Applied vs kept in Writing Tools"
            />
          </div>
          <label {...stylex.props(styles.control)}>
            Chart range
            <select
              aria-label="Statistics chart range"
              value={chartDays}
              onChange={(event) => setChartDays(Number(event.target.value) as 14 | 30)}
            >
              <option value={14}>14 days</option>
              <option value={30}>30 days</option>
            </select>
          </label>
          <div {...stylex.props(styles.charts)}>
            <ChartPanel title={`Words dictated per day · ${chartDays} days`}>
              <StatBarChart
                data={dailyWords}
                label={`Estimated words dictated each day over the last ${chartDays} days`}
              />
            </ChartPanel>
            <ChartPanel title={`Writing requests per day · ${chartDays} days`}>
              <StatBarChart
                data={dailyWriting}
                label={`Writing requests each day over the last ${chartDays} days`}
              />
            </ChartPanel>
            <ChartPanel title={`Estimated spend per day · ${chartDays} days`}>
              <StatBarChart
                data={dailySpend.map((point) => ({
                  date: point.date,
                  value: point.value / 1_000_000,
                }))}
                label={`Estimated spend each day over the last ${chartDays} days`}
              />
            </ChartPanel>
            <ChartPanel title={`Average latency per day · ${chartDays} days`}>
              <StatLineChart
                data={dailyLatency}
                label={`Average writing request latency over the last ${chartDays} days`}
              />
            </ChartPanel>
          </div>
          <ChartPanel title="Recent activity">
            <Heatmap rows={rows} now={now} />
          </ChartPanel>
          <h3>By speech engine</h3>
          <p>
            System:{" "}
            {sum(
              dictations.filter((row) => row.engine === "system"),
              "ok",
            )}{" "}
            · On-device:{" "}
            {sum(
              dictations.filter((row) => row.engine === "local"),
              "ok",
            )}
          </p>
          <h3>Model usage</h3>
          <table {...stylex.props(styles.table)}>
            <thead>
              <tr>
                <th>Model</th>
                <th>Requests</th>
                <th>Success</th>
                <th>Avg ms</th>
                <th>Est. $ / request</th>
                <th>Failovers rescued</th>
              </tr>
            </thead>
            <tbody>
              {models.map(([model, item]) => (
                <tr key={model}>
                  <td>{model}</td>
                  <td>{item.count}</td>
                  <td>{item.count ? `${Math.round((item.ok / item.count) * 100)}%` : "—"}</td>
                  <td>{item.count ? Math.round(item.ms / item.count) : "—"}</td>
                  <td>{item.count ? `≈ $${item.cost / 1_000_000 / item.count}` : "—"}</td>
                  <td>{item.failovers}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {errors.size > 0 && (
            <>
              <h3>Errors by category</h3>
              <ul>
                {[...errors.entries()]
                  .sort((a, b) => b[1] - a[1])
                  .map(([category, count]) => (
                    <li key={category}>
                      {category}: {count}
                    </li>
                  ))}
              </ul>
            </>
          )}
        </>
      )}
      <div {...stylex.props(styles.actions)}>
        <Button
          label="Clear all statistics"
          onClick={() => {
            if (window.confirm("Clear all locally stored usage statistics?"))
              void nativeBridge.clearStats().then(refresh);
          }}
          size="sm"
          variant="secondary"
        />
        <Button
          label="Export statistics as JSON"
          onClick={() => void exportJson()}
          size="sm"
          variant="secondary"
        />
        <Button
          label="Export statistics as CSV"
          onClick={() => void exportCsv()}
          size="sm"
          variant="secondary"
        />
      </div>
    </section>
  );
}

function Metric({ title, value, detail }: { title: string; value: string; detail: string }) {
  return (
    <article {...stylex.props(styles.metric)}>
      <strong>{value}</strong>
      <span>{title}</span>
      <small>{detail}</small>
    </article>
  );
}
function ChartPanel({ title, children }: React.PropsWithChildren<{ title: string }>) {
  return (
    <article {...stylex.props(styles.panel)}>
      <h3>{title}</h3>
      {children}
    </article>
  );
}
function Heatmap({ rows, now }: { rows: StatsRow[]; now: number }) {
  const dates = Array.from({ length: 84 }, (_, index) => {
    const date = new Date(now);
    date.setUTCDate(date.getUTCDate() - (83 - index));
    return dayKey(date);
  });
  const totals = new Map<string, number>();
  for (const row of rows) totals.set(row.date, (totals.get(row.date) ?? 0) + row.count);
  return (
    <div aria-label="Daily activity over the last 12 weeks" {...stylex.props(styles.heatmap)}>
      {dates.map((date) => {
        const level = Math.min(4, Math.ceil((totals.get(date) ?? 0) / 2));
        return (
          <span
            key={date}
            title={`${date}: ${totals.get(date) ?? 0} requests`}
            {...stylex.props(
              styles.cell,
              level > 0 && styles.cellActive,
              level > 1 && styles.cellActiveMore,
            )}
          />
        );
      })}
    </div>
  );
}

const styles = stylex.create({
  section: { display: "flex", flexDirection: "column", gap: "var(--spacing-4)" },
  metrics: {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fit, minmax(0, 1fr))",
    gap: "var(--spacing-3)",
    "@media (max-width: 680px)": { gridTemplateColumns: "1fr" },
  },
  metric: {
    display: "flex",
    flexDirection: "column",
    gap: "var(--spacing-1)",
    padding: "var(--spacing-4)",
    borderRadius: "var(--radius-md)",
    backgroundColor: "var(--color-background-card)",
  },
  charts: {
    display: "grid",
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
    gap: "var(--spacing-4)",
    "@media (max-width: 680px)": { gridTemplateColumns: "1fr" },
  },
  panel: {
    minWidth: "0",
    padding: "var(--spacing-4)",
    borderRadius: "var(--radius-md)",
    backgroundColor: "var(--color-background-card)",
  },
  table: { width: "100%", borderCollapse: "collapse" },
  control: { display: "flex", alignItems: "center", gap: "var(--spacing-3)" },
  actions: { display: "flex", flexWrap: "wrap", gap: "var(--spacing-2)" },
  heatmap: {
    display: "grid",
    gridTemplateColumns: "repeat(12, minmax(0, 1fr))",
    gridAutoFlow: "column",
    gridTemplateRows: "repeat(7, 1fr)",
    gap: "var(--spacing-1)",
  },
  cell: {
    aspectRatio: "1",
    borderRadius: "var(--radius-inner)",
    backgroundColor: "var(--color-background-muted)",
  },
  cellActive: { backgroundColor: "var(--color-accent-muted)" },
  cellActiveMore: { backgroundColor: "var(--color-accent)" },
});
