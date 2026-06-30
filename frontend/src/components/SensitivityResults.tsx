import { useMemo } from "react";
import type { SensitivityRow } from "../lib/sensitivityApi";
import { fmtFloat, fmtPct } from "../lib/sensitivityFormat";
import ExplainerPanel from "./ExplainerPanel";

interface Props {
  rows: SensitivityRow[];
  baselineMean: number;
  metric: string;
  numSims: number;
}

export default function SensitivityResults({
  rows,
  baselineMean,
  metric,
  numSims,
}: Props) {
  const sorted = useMemo(() => {
    const copy = [...rows];
    copy.sort((a, b) => {
      const av = Math.abs(a.mean_diff_relative ?? 0);
      const bv = Math.abs(b.mean_diff_relative ?? 0);
      return bv - av;
    });
    return copy;
  }, [rows]);

  if (rows.length === 0) {
    return (
      <p style={{ color: "var(--text-muted)" }}>
        No rows yet. Configure the scenario and run a sensitivity analysis.
      </p>
    );
  }

  return (
    <div>
      <ExplainerPanel storageKey="oat" title="How to read this (one-at-a-time)">
        <p>
          <strong>What's the question?</strong> "If I bump exactly one stat by a
          small amount, how much does the outcome shift?"
        </p>
        <p>
          <strong>Reading a row:</strong>
        </p>
        <ul style={{ marginTop: 0 }}>
          <li>
            <strong>δ applied</strong> — the size of the bump for that stat (a
            "one realistic step of investment").
          </li>
          <li>
            <strong>Δ metric</strong> — how much the outcome changed in raw
            units (e.g. hull remaining).
          </li>
          <li>
            <strong>Δ relative</strong> — same change as a percentage of the
            baseline. Easier to compare across stats with different units.
          </li>
          <li>
            <strong>95% CI</strong> — the confidence interval on Δ. If it
            includes zero, the change might just be noise.
          </li>
          <li>
            <strong>Significant ✓</strong> — the CI doesn't cross zero, so this
            stat measurably moves the outcome at this sample size. Rows that
            aren't significant are dimmed.
          </li>
        </ul>
        <p>
          <strong>What this method does well:</strong> tight, easy-to-interpret
          numbers per stat. Good first look at "which stats matter at all in
          this scenario."
        </p>
        <p style={{ marginBottom: 0 }}>
          <strong>What it misses:</strong> it changes <em>one</em> stat at a
          time. If two stats only matter when invested together (e.g. armor +
          accuracy), this method underestimates them. For that, switch to Morris
          or Sobol.
        </p>
      </ExplainerPanel>
      <div
        style={{
          marginBottom: "0.75rem",
          color: "var(--text-muted)",
          fontSize: "0.85rem",
        }}
      >
        <strong>{metric}</strong> · baseline mean ={" "}
        <span style={{ fontVariantNumeric: "tabular-nums" }}>
          {fmtFloat(baselineMean)}
        </span>{" "}
        · {numSims} paired sims per stat · sorted by |Δ relative|
      </div>
      <table
        style={{
          width: "100%",
          borderCollapse: "collapse",
          fontSize: "0.9rem",
          fontVariantNumeric: "tabular-nums",
        }}
      >
        <thead>
          <tr style={{ borderBottom: "1px solid var(--border)" }}>
            <th style={{ textAlign: "left", padding: "0.45rem 0.5rem" }}>
              Stat
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="The size of the perturbation applied to this stat — 'one realistic step of investment'."
            >
              δ applied
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="How much the outcome metric changed in raw units (e.g. hull remaining)."
            >
              Δ metric
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="Same change as a percentage of the baseline. Easier to compare across stats."
            >
              Δ relative
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="95% confidence interval on Δ. If it includes zero, the change might just be noise."
            >
              95% CI
            </th>
            <th
              style={{ textAlign: "center", padding: "0.45rem 0.5rem" }}
              title="✓ means the CI doesn't cross zero — this stat measurably moves the outcome at this sample size."
            >
              Significant
            </th>
          </tr>
        </thead>
        <tbody>
          {sorted.map((row, i) => {
            const opacity = row.significant ? 1 : 0.5;
            const bg = i % 2 === 1 ? "rgba(255,255,255,0.03)" : undefined;
            return (
              <tr
                key={row.stat}
                style={{ background: bg, opacity }}
                title={
                  row.significant
                    ? undefined
                    : "95% CI crosses zero — no measurable effect at this N. Increase sims, or accept that this stat doesn't move the outcome in this scenario."
                }
              >
                <td style={{ padding: "0.45rem 0.5rem" }}>{row.stat}</td>
                <td style={{ textAlign: "right", padding: "0.45rem 0.4rem" }}>
                  {fmtFloat(row.delta_applied, 4)}
                </td>
                <td style={{ textAlign: "right", padding: "0.45rem 0.4rem" }}>
                  {fmtFloat(row.mean_diff, 6)}
                </td>
                <td style={{ textAlign: "right", padding: "0.45rem 0.4rem" }}>
                  {fmtPct(row.mean_diff_relative)}
                </td>
                <td
                  style={{
                    textAlign: "right",
                    padding: "0.45rem 0.4rem",
                    color: "var(--text-muted)",
                  }}
                >
                  [{fmtFloat(row.ci95_low, 6)}, {fmtFloat(row.ci95_high, 6)}]
                </td>
                <td style={{ textAlign: "center", padding: "0.45rem 0.5rem" }}>
                  {row.significant ? "✓" : "·"}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
