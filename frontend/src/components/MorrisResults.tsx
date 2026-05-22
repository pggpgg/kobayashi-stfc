import { useMemo, useState } from "react";
import type { MorrisRow } from "../lib/sensitivityApi";

interface Props {
  rows: MorrisRow[];
  metric: string;
  rTrajectories: number;
  numSimsPerPoint: number;
  totalSims: number;
  baseSeed: number;
}

type SortKey = "mu_star" | "sigma" | "mu";

function fmtFloat(n: number, digits = 4): string {
  if (!Number.isFinite(n)) return "—";
  if (Math.abs(n) < 1e-6 && n !== 0) return n.toExponential(2);
  return n.toFixed(digits);
}

export default function MorrisResults({
  rows,
  metric,
  rTrajectories,
  numSimsPerPoint,
  totalSims,
  baseSeed,
}: Props) {
  const [sortBy, setSortBy] = useState<SortKey>("mu_star");

  const sorted = useMemo(() => {
    const copy = [...rows];
    copy.sort((a, b) => {
      if (sortBy === "mu_star") return b.mu_star - a.mu_star;
      if (sortBy === "sigma") return b.sigma - a.sigma;
      return Math.abs(b.mu) - Math.abs(a.mu);
    });
    return copy;
  }, [rows, sortBy]);

  // Interactive heuristic: σ > 0.5 × μ* and μ* > 0 → flag as "interacts".
  // Tight visual cue; not statistical (Sobol pairwise gives the real answer).
  const interactsThreshold = 0.5;

  if (rows.length === 0) {
    return (
      <p style={{ color: "var(--text-muted)" }}>
        No rows yet. Configure the scenario and run Morris screening.
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
        <strong>{metric}</strong> · <strong>r={rTrajectories}</strong>{" "}
        trajectories · {numSimsPerPoint} sims/point ·{" "}
        {totalSims.toLocaleString()} total sims · seed {baseSeed}
      </div>
      <div
        style={{
          marginBottom: "0.5rem",
          fontSize: "0.85rem",
          color: "var(--text-muted)",
        }}
      >
        Sort by:{" "}
        {(["mu_star", "sigma", "mu"] as const).map((key) => (
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
            {key === "mu_star"
              ? "μ* (importance)"
              : key === "sigma"
                ? "σ (interaction)"
                : "|μ| (direction)"}
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
              δ applied
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="Mean of |elementary effects| across trajectories. Importance."
            >
              μ*
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="μ* 95% CI (normal approx on the |EE| std)."
            >
              μ* 95% CI
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="Mean signed EE. Sign indicates direction."
            >
              μ
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="Std of EE across trajectories. High σ relative to μ* suggests interaction with other stats."
            >
              σ
            </th>
            <th
              style={{ textAlign: "center", padding: "0.45rem 0.5rem" }}
              title="Heuristic: σ > 0.5 × μ* hints the stat's effect varies with other stats. Not a pairwise test — Sobol pairwise indices are tracked separately."
            >
              Interacts?
            </th>
          </tr>
        </thead>
        <tbody>
          {sorted.map((row, i) => {
            const bg = i % 2 === 1 ? "rgba(255,255,255,0.03)" : undefined;
            const interacts =
              row.mu_star > 0 && row.sigma > interactsThreshold * row.mu_star;
            return (
              <tr key={row.stat} style={{ background: bg }}>
                <td style={{ padding: "0.45rem 0.5rem" }}>{row.stat}</td>
                <td style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}>
                  {fmtFloat(row.delta_applied, 3)}
                </td>
                <td
                  style={{
                    textAlign: "right",
                    padding: "0.45rem 0.5rem",
                    fontWeight: 600,
                  }}
                >
                  {fmtFloat(row.mu_star)}
                </td>
                <td
                  style={{
                    textAlign: "right",
                    padding: "0.45rem 0.5rem",
                    color: "var(--text-muted)",
                    fontSize: "0.85rem",
                  }}
                >
                  [{fmtFloat(row.mu_star_ci95_low)},{" "}
                  {fmtFloat(row.mu_star_ci95_high)}]
                </td>
                <td style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}>
                  {fmtFloat(row.mu)}
                </td>
                <td style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}>
                  {fmtFloat(row.sigma)}
                </td>
                <td style={{ textAlign: "center", padding: "0.45rem 0.5rem" }}>
                  {interacts ? "•" : ""}
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
        Morris is a <strong>screening</strong> method. σ flags stats whose
        effect depends on other previously-perturbed stats, but doesn't identify
        the specific pairs that interact. For pairwise interaction
        decomposition, see the Sobol entry on the roadmap.
      </p>
    </div>
  );
}
