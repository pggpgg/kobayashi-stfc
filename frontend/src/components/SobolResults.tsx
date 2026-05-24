import { useMemo, useState } from "react";
import type { SobolPairRow, SobolRow } from "../lib/sensitivityApi";
import ExplainerPanel from "./ExplainerPanel";
import SobolPairs from "./SobolPairs";

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
              ? "Total impact"
              : key === "s1"
                ? "Solo impact"
                : "Interaction gap"}
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
              title="Solo impact (S₁): how much this stat changes the outcome by itself, ignoring interactions with other stats. Range 0–1."
            >
              Solo (S<sub>1</sub>)
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="95% confidence interval on the solo impact. Wider = noisier; raise N to tighten."
            >
              95% CI
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="Total impact (S_T): how much this stat changes the outcome on its own AND through combinations with any other stat. Always ≥ solo impact."
            >
              Total (S<sub>T</sub>)
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="95% confidence interval on the total impact."
            >
              95% CI
            </th>
            <th
              style={{ textAlign: "right", padding: "0.45rem 0.5rem" }}
              title="Gap = Total − Solo. The fraction of this stat's impact that comes from interactions with other stats (not from the stat alone)."
            >
              Interaction gap
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
      {pairs && pairs.length > 0 && (
        <SobolPairs pairs={pairs} statOrder={sorted.map((r) => r.stat)} />
      )}
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
