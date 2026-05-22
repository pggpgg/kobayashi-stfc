import { useMemo, useState } from "react";
import type { SobolRow } from "../lib/sensitivityApi";

interface Props {
  rows: SobolRow[];
  metric: string;
  nSamples: number;
  totalSims: number;
  outputVariance: number;
  baseSeed: number;
}

type SortKey = "st" | "s1" | "interaction";

function fmtFloat(n: number, digits = 4): string {
  if (!Number.isFinite(n)) return "—";
  if (Math.abs(n) < 1e-6 && n !== 0) return n.toExponential(2);
  return n.toFixed(digits);
}

export default function SobolResults({
  rows,
  metric,
  nSamples,
  totalSims,
  outputVariance,
  baseSeed,
}: Props) {
  const [sortBy, setSortBy] = useState<SortKey>("st");

  const sorted = useMemo(() => {
    const copy = [...rows];
    copy.sort((a, b) => {
      if (sortBy === "st") return b.st - a.st;
      if (sortBy === "s1") return b.s1 - a.s1;
      return b.interaction - a.interaction;
    });
    return copy;
  }, [rows, sortBy]);

  if (rows.length === 0) {
    return (
      <p style={{ color: "var(--text-muted)" }}>
        No rows yet. Configure the scenario and run Sobol analysis.
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
        <strong>{metric}</strong> · <strong>N={nSamples}</strong> samples ·{" "}
        {totalSims.toLocaleString()} total sims · V(Y) ={" "}
        {fmtFloat(outputVariance)} · seed {baseSeed}
      </div>
      <div
        style={{
          marginBottom: "0.5rem",
          fontSize: "0.85rem",
          color: "var(--text-muted)",
        }}
      >
        Sort by:{" "}
        {(["st", "s1", "interaction"] as const).map((key) => (
          <button
            key={key}
            type="button"
            onClick={() => setSortBy(key)}
            style={{
              marginRight: "0.5rem",
              padding: "0.15rem 0.5rem",
              border: "1px solid var(--border)",
              background: sortBy === key ? "var(--accent)" : "transparent",
              color: sortBy === key ? "var(--bg)" : "inherit",
              borderRadius: 3,
              cursor: "pointer",
              fontSize: "0.8rem",
            }}
          >
            {key === "st"
              ? "S_T (total)"
              : key === "s1"
                ? "S_1 (main)"
                : "interaction"}
          </button>
        ))}
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
              base δ
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="First-order Sobol index: fraction of Var(Y) explained by this stat alone."
            >
              S_1
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="95% bootstrap CI on S_1."
            >
              S_1 95% CI
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="Total-order Sobol index: fraction of Var(Y) involving this stat in any interaction."
            >
              S_T
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="95% bootstrap CI on S_T."
            >
              S_T 95% CI
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="S_T − S_1: variance share from interactions between this stat and any other. Per-pair S_ij is not estimated in v1."
            >
              interaction
            </th>
          </tr>
        </thead>
        <tbody>
          {sorted.map((row, i) => {
            const bg = i % 2 === 1 ? "rgba(255,255,255,0.03)" : undefined;
            return (
              <tr key={row.stat} style={{ background: bg }}>
                <td style={{ padding: "0.45rem 0.5rem" }}>{row.stat}</td>
                <td style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}>
                  {fmtFloat(row.base_delta, 3)}
                </td>
                <td
                  style={{
                    textAlign: "right",
                    padding: "0.45rem 0.5rem",
                    fontWeight: 600,
                  }}
                >
                  {fmtFloat(row.s1)}
                </td>
                <td
                  style={{
                    textAlign: "right",
                    padding: "0.45rem 0.5rem",
                    color: "var(--text-muted)",
                    fontSize: "0.85rem",
                  }}
                >
                  [{fmtFloat(row.s1_ci95_low)}, {fmtFloat(row.s1_ci95_high)}]
                </td>
                <td
                  style={{
                    textAlign: "right",
                    padding: "0.45rem 0.5rem",
                    fontWeight: 600,
                  }}
                >
                  {fmtFloat(row.st)}
                </td>
                <td
                  style={{
                    textAlign: "right",
                    padding: "0.45rem 0.5rem",
                    color: "var(--text-muted)",
                    fontSize: "0.85rem",
                  }}
                >
                  [{fmtFloat(row.st_ci95_low)}, {fmtFloat(row.st_ci95_high)}]
                </td>
                <td style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}>
                  {fmtFloat(row.interaction)}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <p
        style={{
          marginTop: "0.75rem",
          fontSize: "0.8rem",
          color: "var(--text-muted)",
        }}
      >
        Sobol decomposes Var(Y) into first-order (S_1) and total-order (S_T)
        contributions. Σ S_T − Σ S_1 quantifies total interaction strength; v1
        does not estimate per-pair S_ij (planned). Indices may slightly exceed 1
        in finite samples due to estimator noise; bootstrap CIs reflect this.
      </p>
    </div>
  );
}
