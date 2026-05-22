import { useMemo } from "react";
import type { SensitivityRow } from "../lib/sensitivityApi";

interface Props {
  rows: SensitivityRow[];
  baselineMean: number;
  metric: string;
  numSims: number;
}

function fmtPct(n: number | null | undefined, digits = 2): string {
  if (n == null || !Number.isFinite(n)) return "—";
  return `${(n * 100).toFixed(digits)}%`;
}

function fmtFloat(n: number, digits = 4): string {
  if (!Number.isFinite(n)) return "—";
  if (Math.abs(n) < 1e-6 && n !== 0) return n.toExponential(2);
  return n.toFixed(digits);
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
            <th style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}>
              δ applied
            </th>
            <th style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}>
              Δ metric
            </th>
            <th style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}>
              Δ relative
            </th>
            <th style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}>
              95% CI
            </th>
            <th style={{ textAlign: "center", padding: "0.45rem 0.5rem" }}>
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
