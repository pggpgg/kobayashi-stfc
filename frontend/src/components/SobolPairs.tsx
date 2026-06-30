import { useMemo, useState } from "react";
import type { SobolPairRow } from "../lib/sensitivityApi";
import { fmtFloat } from "../lib/sensitivityFormat";

interface Props {
  pairs: SobolPairRow[];
  /** Stat keys in display order, derived from the per-stat rows. Used as the heatmap
   *  row/column ordering so it matches the sort the user already sees above. */
  statOrder: string[];
}

type ViewMode = "list" | "heatmap";

/** Lookup table: "stat_a|stat_b" → row, with both orderings filled in for O(1) lookup. */
function buildPairIndex(pairs: SobolPairRow[]): Map<string, SobolPairRow> {
  const map = new Map<string, SobolPairRow>();
  for (const p of pairs) {
    map.set(`${p.stat_a}|${p.stat_b}`, p);
    map.set(`${p.stat_b}|${p.stat_a}`, p);
  }
  return map;
}

/** Linear interpolation between two HSL endpoints; returns a CSS `hsl(...)` string. */
function heatmapColor(value: number, maxValue: number): string {
  if (maxValue <= 0) return "transparent";
  const t = Math.max(0, Math.min(1, value / maxValue));
  // Light cool gray (low) → saturated blue (high). Lightness drops as t rises.
  const lightness = 96 - t * 60; // 96% → 36%
  const saturation = 8 + t * 70; // 8% → 78%
  return `hsl(218, ${saturation}%, ${lightness}%)`;
}

export default function SobolPairs({ pairs, statOrder }: Props) {
  const [view, setView] = useState<ViewMode>("list");
  const [showAll, setShowAll] = useState<boolean>(false);

  const sortedDesc = useMemo(() => {
    const copy = [...pairs];
    copy.sort((a, b) => b.s_ij - a.s_ij);
    return copy;
  }, [pairs]);

  const maxSij = useMemo(
    () => sortedDesc.reduce((m, p) => Math.max(m, p.s_ij), 0),
    [sortedDesc],
  );
  const pairIndex = useMemo(() => buildPairIndex(pairs), [pairs]);

  const TOP_N = 12;
  const visibleList =
    showAll || sortedDesc.length <= TOP_N
      ? sortedDesc
      : sortedDesc.slice(0, TOP_N);

  if (pairs.length === 0) {
    return null;
  }

  return (
    <div style={{ marginTop: "1.5rem" }}>
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          gap: "1rem",
          marginBottom: "0.5rem",
        }}
      >
        <h4 style={{ margin: 0 }}>
          Pairwise interactions (S<sub>ij</sub>)
        </h4>
        <div
          style={{
            fontSize: "0.8rem",
            color: "var(--text-muted)",
          }}
        >
          View:{" "}
          {(["list", "heatmap"] as const).map((m) => (
            <button
              key={m}
              type="button"
              onClick={() => setView(m)}
              style={{
                marginLeft: "0.4rem",
                padding: "0.15rem 0.55rem",
                border: "1px solid var(--border)",
                background: view === m ? "var(--accent)" : "transparent",
                color: view === m ? "var(--bg)" : "inherit",
                borderRadius: 3,
                cursor: "pointer",
                fontSize: "0.8rem",
              }}
            >
              {m === "list" ? "Top pairs" : "Heatmap"}
            </button>
          ))}
        </div>
      </div>

      {view === "list" && (
        <>
          <table
            style={{
              width: "100%",
              borderCollapse: "collapse",
              fontSize: "0.88rem",
              fontVariantNumeric: "tabular-nums",
            }}
          >
            <thead>
              <tr style={{ borderBottom: "1px solid var(--border)" }}>
                <th style={{ textAlign: "left", padding: "0.4rem 0.5rem" }}>
                  Stat A
                </th>
                <th style={{ textAlign: "left", padding: "0.4rem 0.5rem" }}>
                  Stat B
                </th>
                <th
                  style={{ textAlign: "right", padding: "0.4rem 0.5rem" }}
                  title="Pure pairwise interaction strength. The share of outcome variance these two stats explain together, beyond what each does on its own."
                >
                  Together (S<sub>ij</sub>)
                </th>
                <th
                  style={{
                    textAlign: "right",
                    padding: "0.4rem 0.5rem",
                    color: "var(--text-muted)",
                    fontSize: "0.82rem",
                  }}
                  title="95% bootstrap confidence interval. Pairs whose CI crosses 0 might be noise."
                >
                  95% CI
                </th>
              </tr>
            </thead>
            <tbody>
              {visibleList.map((p, i) => {
                const ciCrossesZero = p.s_ij_ci95_low <= 0 + 1e-9;
                return (
                  <tr
                    key={`${p.stat_a}|${p.stat_b}`}
                    style={{
                      background:
                        i % 2 === 1 ? "rgba(255,255,255,0.03)" : undefined,
                      opacity: ciCrossesZero ? 0.5 : 1,
                    }}
                    title={
                      ciCrossesZero
                        ? "95% CI includes 0 — this pair may not have a real interaction at this sample size."
                        : undefined
                    }
                  >
                    <td style={{ padding: "0.4rem 0.5rem" }}>{p.stat_a}</td>
                    <td style={{ padding: "0.4rem 0.5rem" }}>{p.stat_b}</td>
                    <td
                      style={{
                        textAlign: "right",
                        padding: "0.4rem 0.5rem",
                        fontWeight: 600,
                      }}
                    >
                      {fmtFloat(p.s_ij)}
                    </td>
                    <td
                      style={{
                        textAlign: "right",
                        padding: "0.4rem 0.5rem",
                        color: "var(--text-muted)",
                        fontSize: "0.82rem",
                      }}
                    >
                      [{fmtFloat(p.s_ij_ci95_low)}, {fmtFloat(p.s_ij_ci95_high)}
                      ]
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {sortedDesc.length > TOP_N && (
            <button
              type="button"
              onClick={() => setShowAll((v) => !v)}
              style={{
                marginTop: "0.5rem",
                padding: "0.25rem 0.6rem",
                border: "1px solid var(--border)",
                background: "transparent",
                color: "var(--text-muted)",
                borderRadius: 3,
                cursor: "pointer",
                fontSize: "0.82rem",
              }}
            >
              {showAll
                ? `Hide all (${sortedDesc.length}) — show top ${TOP_N}`
                : `Show all ${sortedDesc.length} pairs`}
            </button>
          )}
        </>
      )}

      {view === "heatmap" && (
        <SobolHeatmap
          statOrder={statOrder}
          pairIndex={pairIndex}
          maxSij={maxSij}
        />
      )}
    </div>
  );
}

interface HeatmapProps {
  statOrder: string[];
  pairIndex: Map<string, SobolPairRow>;
  maxSij: number;
}

function SobolHeatmap({ statOrder, pairIndex, maxSij }: HeatmapProps) {
  const cellSize = 26;
  const labelGutter = 140;
  const k = statOrder.length;
  const w = labelGutter + k * cellSize;
  const h = labelGutter + k * cellSize;

  return (
    <div style={{ overflowX: "auto" }}>
      <svg
        width={w}
        height={h}
        style={{
          fontSize: 10,
          fontFamily: "inherit",
          fontVariantNumeric: "tabular-nums",
        }}
        role="img"
        aria-label="Pairwise Sobol index heatmap"
      >
        <title>
          Heatmap of pairwise Sobol indices. Darker cells = stronger interaction
          between those two stats.
        </title>
        {/* Column labels (rotated) */}
        {statOrder.map((stat, j) => (
          <text
            key={`col-${stat}`}
            x={labelGutter + j * cellSize + cellSize / 2}
            y={labelGutter - 6}
            textAnchor="end"
            transform={`rotate(-55, ${labelGutter + j * cellSize + cellSize / 2}, ${labelGutter - 6})`}
            fill="var(--text-muted)"
          >
            {stat}
          </text>
        ))}
        {/* Row labels */}
        {statOrder.map((stat, i) => (
          <text
            key={`row-${stat}`}
            x={labelGutter - 6}
            y={labelGutter + i * cellSize + cellSize / 2 + 3}
            textAnchor="end"
            fill="var(--text-muted)"
          >
            {stat}
          </text>
        ))}
        {/* Cells */}
        {statOrder.flatMap((a, i) =>
          statOrder.map((b, j) => {
            const pair = pairIndex.get(`${a}|${b}`);
            const v = a === b ? 0 : (pair?.s_ij ?? 0);
            const fill = a === b ? "var(--bg)" : heatmapColor(v, maxSij);
            const titleText =
              a === b
                ? `${a} (diagonal, no self-interaction)`
                : pair
                  ? `${a} × ${b}: S_ij = ${fmtFloat(pair.s_ij)} (95% CI [${fmtFloat(pair.s_ij_ci95_low)}, ${fmtFloat(pair.s_ij_ci95_high)}])`
                  : `${a} × ${b}: no data`;
            return (
              <g key={`cell-${i}-${j}`}>
                <rect
                  x={labelGutter + j * cellSize}
                  y={labelGutter + i * cellSize}
                  width={cellSize}
                  height={cellSize}
                  fill={fill}
                  stroke="rgba(0,0,0,0.05)"
                  strokeWidth={1}
                >
                  <title>{titleText}</title>
                </rect>
              </g>
            );
          }),
        )}
      </svg>
      <div
        style={{
          marginTop: "0.5rem",
          fontSize: "0.78rem",
          color: "var(--text-muted)",
        }}
      >
        Cells are symmetric (S<sub>ij</sub> = S<sub>ji</sub>); the diagonal is
        blank (a stat doesn't interact with itself). Hover any cell for the
        exact value + 95% CI.
      </div>
    </div>
  );
}
