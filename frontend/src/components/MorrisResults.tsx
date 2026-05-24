import { useMemo, useState } from "react";
import type { MorrisRow } from "../lib/sensitivityApi";
import ExplainerPanel from "./ExplainerPanel";

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
      <ExplainerPanel
        storageKey="morris"
        title="How to read this (Morris screening)"
      >
        <p>
          <strong>What's the question?</strong> "Which stats are worth a closer
          look — and which ones might only matter in combination with others?"
        </p>
        <p>
          <strong>How it works:</strong> instead of bumping one stat at a time,
          Morris walks <em>r</em> random paths through stat-space, perturbing
          stats in a different random order on each path. For each path, it
          measures how much each stat changed the outcome at the moment it was
          perturbed. That gives <em>r</em> "elementary effects" per stat, which
          collapse into three numbers:
        </p>
        <ul style={{ marginTop: 0 }}>
          <li>
            <strong>μ* (importance)</strong> — the average size of each stat's
            elementary effect, ignoring sign. The headline number: higher means
            this stat moves the outcome more on average. Sort by this first.
          </li>
          <li>
            <strong>μ (direction)</strong> — average <em>signed</em> effect.
            Positive means investing in the stat improves the outcome; negative
            means it hurts. When μ ≈ 0 but μ* is large, the effect is sometimes
            positive and sometimes negative depending on other stats — a strong
            interaction hint.
          </li>
          <li>
            <strong>σ (interaction signal)</strong> — how much the elementary
            effects vary across paths. High σ relative to μ* means the stat's
            value depends on what other stats are doing.
          </li>
        </ul>
        <p>
          <strong>Reading the "Interacts?" dot:</strong> a heuristic flag for σ
          &gt; 0.5 × μ*. A dot means "this stat's effect depends on others" —
          but Morris won't tell you <em>which</em> others. For that, run Sobol
          with pairwise interactions enabled.
        </p>
        <p style={{ marginBottom: 0 }}>
          <strong>When to use Morris vs the others:</strong> Morris is the
          middle-ground choice. Cheaper than Sobol, more informative than OAT.
          Use it to filter the stat list down to the ones worth a careful Sobol
          run.
        </p>
      </ExplainerPanel>
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
              title="Importance: how much this stat moves the outcome on average, ignoring whether it goes up or down. Sort by this first."
            >
              μ* (importance)
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="95% confidence interval on the importance. Wider = noisier; raise sims per point or trajectory count to tighten."
            >
              95% CI
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="Direction: average signed effect. Positive = investing helps; negative = hurts. Small μ but large μ* means 'helps in some setups, hurts in others' — an interaction hint."
            >
              μ (direction)
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="Interaction signal: how much the effect varies across random paths. High σ vs. μ* means the stat's effect depends on what other stats are doing."
            >
              σ (interaction)
            </th>
            <th
              style={{ textAlign: "center", padding: "0.45rem 0.5rem" }}
              title="Quick flag: σ > 0.5 × μ* — this stat probably interacts with others. To find out WHICH others, run Sobol with pairwise interactions enabled."
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
        Morris is a <strong>screening</strong> method — quick triage to find
        what's worth a deeper look. σ flags stats whose effect depends on
        others, but won't tell you which other stat is the partner. Switch to
        Sobol and enable "Also compute pairwise interactions" to see the
        specific pairs.
      </p>
    </div>
  );
}
