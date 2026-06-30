import { useMemo, useState } from "react";
import type { MorrisRow } from "../lib/sensitivityApi";
import { fmtFloat } from "../lib/sensitivityFormat";
import ExplainerPanel from "./ExplainerPanel";
import SortableStatTable, { type StatTableColumn } from "./SortableStatTable";

interface Props {
  rows: MorrisRow[];
  metric: string;
  rTrajectories: number;
  numSimsPerPoint: number;
  totalSims: number;
  baseSeed: number;
}

type SortKey = "mu_star" | "sigma" | "mu";

// Interactive heuristic: σ > 0.5 × μ* and μ* > 0 → flag as "interacts".
// Tight visual cue; not statistical (Sobol pairwise gives the real answer).
const INTERACTS_THRESHOLD = 0.5;

const SORT_KEYS = [
  { key: "mu_star" as const, label: "μ* (importance)" },
  { key: "sigma" as const, label: "σ (interaction)" },
  { key: "mu" as const, label: "|μ| (direction)" },
];

const columns: StatTableColumn<MorrisRow>[] = [
  { key: "stat", header: "Stat", align: "left", render: (row) => row.stat },
  {
    key: "delta_applied",
    header: "δ applied",
    render: (row) => fmtFloat(row.delta_applied, 3),
  },
  {
    key: "mu_star",
    header: "μ* (importance)",
    headerTitle:
      "Importance: how much this stat moves the outcome on average, ignoring whether it goes up or down. Sort by this first.",
    variant: "headline",
    render: (row) => fmtFloat(row.mu_star),
  },
  {
    key: "mu_star_ci",
    header: "95% CI",
    headerTitle:
      "95% confidence interval on the importance. Wider = noisier; raise sims per point or trajectory count to tighten.",
    variant: "ci",
    render: (row) =>
      `[${fmtFloat(row.mu_star_ci95_low)}, ${fmtFloat(row.mu_star_ci95_high)}]`,
  },
  {
    key: "mu",
    header: "μ (direction)",
    headerTitle:
      "Direction: average signed effect. Positive = investing helps; negative = hurts. Small μ but large μ* means 'helps in some setups, hurts in others' — an interaction hint.",
    render: (row) => fmtFloat(row.mu),
  },
  {
    key: "sigma",
    header: "σ (interaction)",
    headerTitle:
      "Interaction signal: how much the effect varies across random paths. High σ vs. μ* means the stat's effect depends on what other stats are doing.",
    render: (row) => fmtFloat(row.sigma),
  },
  {
    key: "interacts",
    header: "Interacts?",
    headerTitle:
      "Quick flag: σ > 0.5 × μ* — this stat probably interacts with others. To find out WHICH others, run Sobol with pairwise interactions enabled.",
    align: "center",
    render: (row) =>
      row.mu_star > 0 && row.sigma > INTERACTS_THRESHOLD * row.mu_star
        ? "•"
        : "",
  },
];

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
      <SortableStatTable
        rows={sorted}
        rowKey={(row) => row.stat}
        columns={columns}
        sortKeys={SORT_KEYS}
        sortBy={sortBy}
        onSortByChange={setSortBy}
        summary={
          <>
            <strong>{metric}</strong> · <strong>r={rTrajectories}</strong>{" "}
            trajectories · {numSimsPerPoint} sims/point ·{" "}
            {totalSims.toLocaleString()} total sims · seed {baseSeed}
          </>
        }
      />
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
