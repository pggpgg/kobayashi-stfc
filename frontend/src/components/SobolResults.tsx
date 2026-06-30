import { useMemo, useState } from "react";
import type { SobolPairRow, SobolRow } from "../lib/sensitivityApi";
import { fmtFloat } from "../lib/sensitivityFormat";
import ExplainerPanel from "./ExplainerPanel";
import SobolPairs from "./SobolPairs";
import SortableStatTable, { type StatTableColumn } from "./SortableStatTable";

interface Props {
  rows: SobolRow[];
  metric: string;
  nSamples: number;
  totalSims: number;
  outputVariance: number;
  baseSeed: number;
  pairs?: SobolPairRow[];
}

type SortKey = "st" | "s1" | "interaction";

const SORT_KEYS = [
  { key: "st" as const, label: "Total impact" },
  { key: "s1" as const, label: "Solo impact" },
  { key: "interaction" as const, label: "Interaction gap" },
];

const columns: StatTableColumn<SobolRow>[] = [
  { key: "stat", header: "Stat", align: "left", render: (row) => row.stat },
  {
    key: "base_delta",
    header: "base δ",
    render: (row) => fmtFloat(row.base_delta, 3),
  },
  {
    key: "s1",
    header: (
      <>
        Solo (S<sub>1</sub>)
      </>
    ),
    headerTitle:
      "Solo impact (S₁): how much this stat changes the outcome by itself, ignoring interactions with other stats. Range 0–1.",
    variant: "headline",
    render: (row) => fmtFloat(row.s1),
  },
  {
    key: "s1_ci",
    header: "95% CI",
    headerTitle:
      "95% confidence interval on the solo impact. Wider = noisier; raise N to tighten.",
    variant: "ci",
    render: (row) =>
      `[${fmtFloat(row.s1_ci95_low)}, ${fmtFloat(row.s1_ci95_high)}]`,
  },
  {
    key: "st",
    header: (
      <>
        Total (S<sub>T</sub>)
      </>
    ),
    headerTitle:
      "Total impact (S_T): how much this stat changes the outcome on its own AND through combinations with any other stat. Always ≥ solo impact.",
    variant: "headline",
    render: (row) => fmtFloat(row.st),
  },
  {
    key: "st_ci",
    header: "95% CI",
    headerTitle: "95% confidence interval on the total impact.",
    variant: "ci",
    render: (row) =>
      `[${fmtFloat(row.st_ci95_low)}, ${fmtFloat(row.st_ci95_high)}]`,
  },
  {
    key: "interaction",
    header: "Interaction gap",
    headerTitle:
      "Gap = Total − Solo. The fraction of this stat's impact that comes from interactions with other stats (not from the stat alone).",
    render: (row) => fmtFloat(row.interaction),
  },
];

export default function SobolResults({
  rows,
  metric,
  nSamples,
  totalSims,
  outputVariance,
  baseSeed,
  pairs,
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
      <ExplainerPanel
        storageKey="sobol"
        title="How to read this (Sobol variance decomposition)"
      >
        <p>
          <strong>What's the question?</strong> "Of all the variation I see in
          outcomes across this scenario, how much is each stat responsible for —
          and how much is the result of stats working together?"
        </p>
        <p>
          <strong>The two main numbers per row:</strong>
        </p>
        <ul style={{ marginTop: 0 }}>
          <li>
            <strong>
              Solo impact (S<sub>1</sub>)
            </strong>{" "}
            — what fraction of the outcome's variation this stat causes{" "}
            <em>by itself</em>. 0 means the stat does nothing on its own; 1
            means it's the only thing that matters. Practical range usually
            0–0.5.
          </li>
          <li>
            <strong>
              Total impact (S<sub>T</sub>)
            </strong>{" "}
            — same idea, but also counting interactions with every other stat.{" "}
            <em>
              S<sub>T</sub> ≥ S<sub>1</sub>
            </em>{" "}
            always. The gap (S<sub>T</sub> − S<sub>1</sub>) tells you how much
            of the stat's value depends on other stats being invested too.
          </li>
        </ul>
        <p>
          <strong>Reading a row at a glance:</strong>
        </p>
        <ul style={{ marginTop: 0 }}>
          <li>
            <strong>
              High S<sub>1</sub>, S<sub>T</sub> ≈ S<sub>1</sub>
            </strong>{" "}
            → invest in this stat on its own. It pays back regardless of what
            else you do.
          </li>
          <li>
            <strong>
              Low S<sub>1</sub>, high S<sub>T</sub>
            </strong>{" "}
            → the stat only matters in combination. Don't invest in it alone;
            pair it with its partner. Turn on "Also compute pairwise
            interactions" below to see which partner.
          </li>
          <li>
            <strong>Both near 0</strong> → this stat doesn't move the needle in
            this scenario.
          </li>
        </ul>
        <p>
          <strong>About the 95% CIs:</strong> these are confidence intervals on
          each index. A CI like <code>[0.20, 0.35]</code> says "we're 95% sure
          the true value is in that range." A CI that includes 0 means the
          measurement is too noisy at this sample size to claim a real effect —
          raise <strong>N</strong> to tighten them.
        </p>
        <p style={{ marginBottom: 0 }}>
          <strong>Sanity check:</strong> indices may slightly exceed 1 in finite
          samples (estimator noise). The CIs reflect that.
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
            <strong>{metric}</strong> · <strong>N={nSamples}</strong> samples ·{" "}
            {totalSims.toLocaleString()} total sims · V(Y) ={" "}
            {fmtFloat(outputVariance)} · seed {baseSeed}
          </>
        }
      >
        {pairs && pairs.length > 0 && (
          <SobolPairs pairs={pairs} statOrder={sorted.map((r) => r.stat)} />
        )}
      </SortableStatTable>
      <p
        style={{
          marginTop: "0.75rem",
          fontSize: "0.8rem",
          color: "var(--text-muted)",
        }}
      >
        Sobol decomposes the variance of the outcome across the scenario into
        contributions from each stat alone and from combinations of stats.
        Σ&nbsp;S<sub>T</sub>&nbsp;− Σ&nbsp;S<sub>1</sub> is the total
        interaction budget. Solo + interaction contributions can sum to less or
        more than 1 in finite samples due to estimator noise; the CIs reflect
        this.
        {!pairs && (
          <>
            {" "}
            To see <em>which specific pairs</em> of stats interact, enable "Also
            compute pairwise interactions" in the Sobol params above and re-run.
          </>
        )}
      </p>
    </div>
  );
}
