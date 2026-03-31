import { useEffect, useState } from "react";
import {
  type CompareCrewDistribution,
  type CrewRecommendation,
  compareCrewsDistributions,
  crewRecommendationToSimulateCrew,
  formatApiError,
  formatOptimizePhaseLabel,
  type SimulateStats,
} from "../lib/api";

const PER_PAGE_OPTIONS = [50, 100, 200, 500] as const;
const DEFAULT_PER_PAGE = 50;

/** Normalize captain/bridge/below_decks for display: API may return string[]; join with ", ". */
function formatCrewCell(value: string | string[] | null | undefined): string {
  if (value == null) return "";
  if (Array.isArray(value)) return value.filter(Boolean).join(", ");
  return String(value);
}

/** Percent point estimate with 95% CI in parentheses (Wilson/normal per server notes). */
function formatPctWithCi(p: number, lo: number, hi: number): string {
  const main = (p * 100).toFixed(2);
  const a = (lo * 100).toFixed(1);
  const b = (hi * 100).toFixed(1);
  return `${main}\u00a0(${a}\u2013${b})`;
}

/** Scenario context for POST /api/compare/crews (Monte Carlo distributions). */
export interface CompareWorkspaceParams {
  ship: string;
  hostile: string;
  shipTier: number;
  shipLevel: number;
  numSims: number;
  belowDecksSlots: number;
  profileId: string | null;
  /** Parity with simulate/optimize workspace selection. */
  supportBuffs?: string[];
}

function SideBySideHistograms({
  crews,
  title,
  getCounts,
}: {
  crews: CompareCrewDistribution[];
  title: string;
  getCounts: (c: CompareCrewDistribution) => number[];
}) {
  const series = crews.map(getCounts);
  const max = Math.max(1, ...series.flat());
  return (
    <div style={{ marginTop: 12 }}>
      <div
        style={{
          fontSize: "0.75rem",
          color: "var(--text-muted)",
          marginBottom: 6,
        }}
      >
        {title}
      </div>
      <div style={{ display: "flex", gap: 10, alignItems: "flex-end" }}>
        {crews.map((c, ci) => {
          const vals = getCounts(c);
          return (
            <div key={`${c.captain}-${ci}`} style={{ flex: 1, minWidth: 0 }}>
              <div
                style={{
                  fontSize: "0.7rem",
                  marginBottom: 4,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {c.captain}
              </div>
              <div
                style={{
                  display: "flex",
                  alignItems: "flex-end",
                  gap: 1,
                  height: 80,
                }}
              >
                {vals.map((count, i) => (
                  <div
                    key={i}
                    title={String(count)}
                    style={{
                      flex: 1,
                      minWidth: 2,
                      height: `${(count / max) * 100}%`,
                      minHeight: count > 0 ? 2 : 0,
                      background: "var(--accent)",
                      opacity: 0.88,
                      borderRadius: 1,
                    }}
                  />
                ))}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

interface SimResultsProps {
  simResult: SimulateStats | null;
  recommendations: CrewRecommendation[];
  loadingSim: boolean;
  loadingOptimize: boolean;
  optimizeProgress: number | null;
  optimizeCrewsDone: number | null;
  optimizeTotalCrews: number | null;
  optimizePhase?: string | null;
  optimizeEtaSeconds?: number | null;
  optimizeThroughput?: number | null;
  optimizePreview?: CrewRecommendation[] | null;
  compareWorkspace?: CompareWorkspaceParams | null;
}

export default function SimResults({
  simResult,
  recommendations,
  loadingSim,
  loadingOptimize,
  optimizeProgress,
  optimizeCrewsDone,
  optimizeTotalCrews,
  optimizePhase = null,
  optimizeEtaSeconds = null,
  optimizeThroughput = null,
  optimizePreview = null,
  compareWorkspace = null,
}: SimResultsProps) {
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [page, setPage] = useState(1);
  const [perPage, setPerPage] = useState(DEFAULT_PER_PAGE);
  const [compareDist, setCompareDist] = useState<
    CompareCrewDistribution[] | null
  >(null);
  const [compareSeed, setCompareSeed] = useState<number | null>(null);
  const [loadingCompareDist, setLoadingCompareDist] = useState(false);
  const [compareDistErr, setCompareDistErr] = useState<string | null>(null);
  const hasSim = simResult != null;
  const hasRecs = recommendations.length > 0;

  const total = recommendations.length;
  const totalPages = Math.max(1, Math.ceil(total / perPage));
  const safePage = Math.min(page, totalPages);
  const start = (safePage - 1) * perPage;
  const pageRecs = recommendations.slice(start, start + perPage);

  // Reset to page 1 when recommendations change (e.g. new optimize run) or when current page is out of range
  useEffect(() => {
    if (page > totalPages && totalPages >= 1) setPage(1);
  }, [totalPages, total]);

  useEffect(() => {
    setCompareDist(null);
    setCompareSeed(null);
    setCompareDistErr(null);
  }, [recommendations]);

  const toggleSelect = (i: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(i)) next.delete(i);
      else if (next.size < 5) next.add(i);
      return next;
    });
  };

  const selectedList = Array.from(selected).sort((a, b) => a - b);
  const showCompare = selectedList.length >= 2 && selectedList.length <= 5;

  const runCompareDistributions = async () => {
    if (!compareWorkspace || !showCompare) return;
    setCompareDistErr(null);
    setLoadingCompareDist(true);
    try {
      const crews = selectedList.map((idx) =>
        crewRecommendationToSimulateCrew(
          recommendations[idx],
          compareWorkspace.belowDecksSlots,
        ),
      );
      const res = await compareCrewsDistributions(
        {
          ship: compareWorkspace.ship,
          hostile: compareWorkspace.hostile,
          crews,
          num_sims: compareWorkspace.numSims,
          seed: Date.now() % 1_000_000_001,
          ship_tier: compareWorkspace.shipTier,
          ship_level: compareWorkspace.shipLevel,
          below_decks_slots: compareWorkspace.belowDecksSlots,
          proc_sample_trials: 60,
          support_buffs: compareWorkspace.supportBuffs,
        },
        compareWorkspace.profileId,
      );
      setCompareDist(res.crews);
      setCompareSeed(res.seed);
    } catch (e) {
      setCompareDist(null);
      setCompareDistErr(formatApiError(e));
    } finally {
      setLoadingCompareDist(false);
    }
  };

  return (
    <section
      style={{
        padding: "1rem",
        background: "var(--surface)",
        border: "1px solid var(--border)",
        borderRadius: 8,
        overflow: "auto",
      }}
    >
      <h2 style={{ margin: "0 0 0.75rem", fontSize: "1rem" }}>SimResults</h2>

      {(loadingSim || loadingOptimize) && (
        <div style={{ marginBottom: "0.75rem" }}>
          <p style={{ margin: 0, color: "var(--text-muted)" }}>
            {loadingOptimize
              ? "Optimization in progress… This may take a minute depending on scenario."
              : "Running…"}
          </p>
          {loadingOptimize && optimizeProgress != null && (
            <div style={{ marginTop: 8 }}>
              <div
                style={{
                  height: 10,
                  background: "var(--border)",
                  borderRadius: 5,
                  overflow: "hidden",
                }}
              >
                <div
                  style={{
                    width: `${optimizeProgress}%`,
                    height: "100%",
                    background: "var(--accent)",
                    borderRadius: 5,
                    transition: "width 0.2s ease",
                  }}
                />
              </div>
              <p
                style={{
                  margin: "4px 0 0",
                  fontSize: "0.8rem",
                  color: "var(--text-muted)",
                }}
              >
                {optimizePhase === "genetic" &&
                optimizeTotalCrews != null &&
                optimizeCrewsDone != null
                  ? `Generation ${optimizeCrewsDone} / ${optimizeTotalCrews} (${optimizeProgress}%)`
                  : optimizeTotalCrews != null && optimizeCrewsDone != null
                    ? `${optimizeCrewsDone} / ${optimizeTotalCrews} units (${optimizeProgress}%)`
                    : `${optimizeProgress}%`}
              </p>
              {(optimizePhase ||
                optimizeThroughput != null ||
                optimizeEtaSeconds != null) && (
                <p
                  style={{
                    margin: "6px 0 0",
                    fontSize: "0.75rem",
                    color: "var(--text-muted)",
                    lineHeight: 1.4,
                  }}
                >
                  {formatOptimizePhaseLabel(optimizePhase) && (
                    <span>{formatOptimizePhaseLabel(optimizePhase)}</span>
                  )}
                  {optimizeThroughput != null &&
                    optimizePhase !== "genetic" && (
                      <>
                        {formatOptimizePhaseLabel(optimizePhase) ? " · " : ""}~
                        {optimizeThroughput.toFixed(1)} crews/s
                      </>
                    )}
                  {optimizeEtaSeconds != null && (
                    <>
                      {formatOptimizePhaseLabel(optimizePhase) ||
                      (optimizeThroughput != null &&
                        optimizePhase !== "genetic")
                        ? " · "
                        : ""}
                      ETA ~{optimizeEtaSeconds}s
                    </>
                  )}
                </p>
              )}
              {optimizePreview != null && optimizePreview.length > 0 && (
                <div
                  style={{
                    marginTop: 10,
                    padding: 8,
                    background: "var(--bg)",
                    borderRadius: 6,
                    fontSize: "0.75rem",
                  }}
                >
                  <div
                    style={{
                      fontWeight: 600,
                      marginBottom: 6,
                      color: "var(--text-muted)",
                    }}
                  >
                    Top crews so far (preview)
                  </div>
                  <table style={{ width: "100%", borderCollapse: "collapse" }}>
                    <tbody>
                      {optimizePreview.map((r, i) => (
                        <tr key={i}>
                          <td
                            style={{
                              padding: "2px 8px 2px 0",
                              verticalAlign: "top",
                            }}
                          >
                            {r.captain}
                          </td>
                          <td
                            style={{
                              padding: "2px 0",
                              color: "var(--text-muted)",
                              whiteSpace: "nowrap",
                            }}
                          >
                            {(r.win_rate * 100).toFixed(1)}% win
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {hasSim && !loadingSim && (
        <div
          style={{
            marginBottom: "1rem",
            padding: "0.75rem",
            background: "var(--bg)",
            borderRadius: 6,
          }}
        >
          <strong>Last sim (current crew)</strong>
          <div
            style={{
              marginTop: 4,
              display: "flex",
              flexWrap: "wrap",
              gap: "0.5rem 1.5rem",
            }}
          >
            <span>Win rate: {(simResult.win_rate * 100).toFixed(2)}%</span>
            <span>Stall rate: {(simResult.stall_rate * 100).toFixed(2)}%</span>
            <span>Loss rate: {(simResult.loss_rate * 100).toFixed(2)}%</span>
            <span>
              Avg hull remaining:{" "}
              {(simResult.avg_hull_remaining * 100).toFixed(2)}%
            </span>
            <span style={{ color: "var(--text-muted)" }}>
              (n={simResult.n})
            </span>
            {simResult.win_rate_95_ci && (
              <span style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
                95% CI: [{simResult.win_rate_95_ci[0].toFixed(3)},{" "}
                {simResult.win_rate_95_ci[1].toFixed(3)}]
              </span>
            )}
          </div>
        </div>
      )}

      {hasRecs && (
        <>
          <p
            style={{
              margin: "0 0 0.5rem",
              fontSize: "0.85rem",
              color: "var(--text-muted)",
            }}
          >
            Select 2–5 rows to compare. Optimize columns show point % (95% CI):
            Wilson for win/stall/loss/R1; normal approx for hull score per
            trial.
          </p>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "1rem",
              flexWrap: "wrap",
              marginBottom: "0.5rem",
              fontSize: "0.85rem",
            }}
          >
            <label
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                color: "var(--text-muted)",
              }}
            >
              Results per page
              <select
                value={perPage}
                onChange={(e) => {
                  const n = Number(e.target.value);
                  setPerPage(n);
                  setPage((p) =>
                    Math.min(p, Math.max(1, Math.ceil(total / n))),
                  );
                }}
                style={{
                  padding: "0.25rem 0.5rem",
                  background: "var(--bg)",
                  border: "1px solid var(--border)",
                  borderRadius: 4,
                  color: "var(--text)",
                }}
                aria-label="Results per page"
              >
                {PER_PAGE_OPTIONS.map((n) => (
                  <option key={n} value={n}>
                    {n}
                  </option>
                ))}
              </select>
            </label>
            <span style={{ color: "var(--text-muted)" }}>
              Showing {start + 1}–{Math.min(start + perPage, total)} of {total}
            </span>
            {totalPages > 1 && (
              <span style={{ display: "flex", alignItems: "center", gap: 4 }}>
                <button
                  type="button"
                  onClick={() => setPage((p) => Math.max(1, p - 1))}
                  disabled={safePage <= 1}
                  aria-label="Previous page"
                  style={{
                    padding: "0.25rem 0.5rem",
                    background: "var(--bg)",
                    border: "1px solid var(--border)",
                    borderRadius: 4,
                    color: "var(--text)",
                    cursor: safePage <= 1 ? "not-allowed" : "pointer",
                    opacity: safePage <= 1 ? 0.6 : 1,
                  }}
                >
                  Prev
                </button>
                <span
                  style={{
                    color: "var(--text-muted)",
                    minWidth: 80,
                    textAlign: "center",
                  }}
                >
                  Page {safePage} of {totalPages}
                </span>
                <button
                  type="button"
                  onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
                  disabled={safePage >= totalPages}
                  aria-label="Next page"
                  style={{
                    padding: "0.25rem 0.5rem",
                    background: "var(--bg)",
                    border: "1px solid var(--border)",
                    borderRadius: 4,
                    color: "var(--text)",
                    cursor: safePage >= totalPages ? "not-allowed" : "pointer",
                    opacity: safePage >= totalPages ? 0.6 : 1,
                  }}
                >
                  Next
                </button>
              </span>
            )}
          </div>
          <table
            style={{
              width: "100%",
              borderCollapse: "collapse",
              fontSize: "0.9rem",
            }}
          >
            <thead>
              <tr style={{ borderBottom: "1px solid var(--border)" }}>
                <th
                  style={{ textAlign: "left", padding: "0.4rem", width: 32 }}
                />
                <th style={{ textAlign: "left", padding: "0.4rem" }}>#</th>
                <th style={{ textAlign: "left", padding: "0.4rem" }}>
                  Captain
                </th>
                <th style={{ textAlign: "left", padding: "0.4rem" }}>Bridge</th>
                <th style={{ textAlign: "left", padding: "0.4rem" }}>
                  Below Deck
                </th>
                <th style={{ textAlign: "right", padding: "0.4rem" }}>Win %</th>
                <th style={{ textAlign: "right", padding: "0.4rem" }}>
                  Stall %
                </th>
                <th style={{ textAlign: "right", padding: "0.4rem" }}>
                  Loss %
                </th>
                <th style={{ textAlign: "right", padding: "0.4rem" }}>R1 %</th>
                <th style={{ textAlign: "right", padding: "0.4rem" }}>
                  Hull %
                </th>
              </tr>
            </thead>
            <tbody>
              {pageRecs.map((r, i) => {
                const globalIndex = start + i;
                return (
                  <tr
                    key={globalIndex}
                    style={{
                      borderBottom: "1px solid var(--border)",
                      background: selected.has(globalIndex)
                        ? "rgba(232,149,46,0.1)"
                        : undefined,
                    }}
                  >
                    <td style={{ padding: "0.4rem" }}>
                      <input
                        type="checkbox"
                        checked={selected.has(globalIndex)}
                        onChange={() => toggleSelect(globalIndex)}
                        aria-label={`Select row ${globalIndex + 1}`}
                      />
                    </td>
                    <td style={{ padding: "0.4rem" }}>{globalIndex + 1}</td>
                    <td style={{ padding: "0.4rem" }}>
                      {formatCrewCell(r.captain)}
                    </td>
                    <td style={{ padding: "0.4rem" }}>
                      {formatCrewCell(r.bridge)}
                    </td>
                    <td style={{ padding: "0.4rem" }}>
                      {formatCrewCell(r.below_decks)}
                    </td>
                    <td
                      style={{
                        padding: "0.4rem",
                        textAlign: "right",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {formatPctWithCi(
                        r.win_rate,
                        r.win_rate_ci_low,
                        r.win_rate_ci_high,
                      )}
                    </td>
                    <td
                      style={{
                        padding: "0.4rem",
                        textAlign: "right",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {formatPctWithCi(
                        r.stall_rate,
                        r.stall_rate_ci_low,
                        r.stall_rate_ci_high,
                      )}
                    </td>
                    <td
                      style={{
                        padding: "0.4rem",
                        textAlign: "right",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {formatPctWithCi(
                        r.loss_rate,
                        r.loss_rate_ci_low,
                        r.loss_rate_ci_high,
                      )}
                    </td>
                    <td
                      style={{
                        padding: "0.4rem",
                        textAlign: "right",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {formatPctWithCi(
                        r.r1_kill_rate,
                        r.r1_kill_rate_ci_low,
                        r.r1_kill_rate_ci_high,
                      )}
                    </td>
                    <td
                      style={{
                        padding: "0.4rem",
                        textAlign: "right",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {formatPctWithCi(
                        r.avg_hull_remaining,
                        r.avg_hull_remaining_ci_low,
                        r.avg_hull_remaining_ci_high,
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>

          {showCompare && (
            <div
              style={{
                marginTop: "1rem",
                padding: "0.75rem",
                background: "var(--bg)",
                border: "1px solid var(--border)",
                borderRadius: 6,
              }}
            >
              <strong>Compare (delta)</strong>
              <div
                style={{
                  marginTop: 8,
                  display: "flex",
                  flexDirection: "column",
                  gap: 4,
                }}
              >
                {selectedList.map((idx, j) => {
                  const r = recommendations[idx];
                  const prev =
                    j === 0 ? null : recommendations[selectedList[j - 1]];
                  const deltaWin =
                    prev != null ? (r.win_rate - prev.win_rate) * 100 : 0;
                  const deltaStall =
                    prev != null ? (r.stall_rate - prev.stall_rate) * 100 : 0;
                  const deltaLoss =
                    prev != null ? (r.loss_rate - prev.loss_rate) * 100 : 0;
                  const deltaR1 =
                    prev != null
                      ? (r.r1_kill_rate - prev.r1_kill_rate) * 100
                      : 0;
                  const deltaHull =
                    prev != null
                      ? (r.avg_hull_remaining - prev.avg_hull_remaining) * 100
                      : 0;
                  return (
                    <div key={idx} style={{ fontSize: "0.85rem" }}>
                      <span style={{ fontWeight: 600 }}>#{idx + 1}</span>{" "}
                      {formatCrewCell(r.captain)} / {formatCrewCell(r.bridge)} /{" "}
                      {formatCrewCell(r.below_decks)}
                      {prev != null && (
                        <span
                          style={{ marginLeft: 8, color: "var(--text-muted)" }}
                        >
                          Δ Win {deltaWin >= 0 ? "+" : ""}
                          {deltaWin.toFixed(2)}%, Δ Stall{" "}
                          {deltaStall >= 0 ? "+" : ""}
                          {deltaStall.toFixed(2)}%, Δ Loss{" "}
                          {deltaLoss >= 0 ? "+" : ""}
                          {deltaLoss.toFixed(2)}%, Δ R1{" "}
                          {deltaR1 >= 0 ? "+" : ""}
                          {deltaR1.toFixed(2)}%, Δ Hull{" "}
                          {deltaHull >= 0 ? "+" : ""}
                          {deltaHull.toFixed(2)}%
                        </span>
                      )}
                    </div>
                  );
                })}
              </div>
              {compareWorkspace?.ship && compareWorkspace.hostile && (
                <div style={{ marginTop: 12 }}>
                  <button
                    type="button"
                    onClick={() => void runCompareDistributions()}
                    disabled={loadingCompareDist}
                    style={{
                      padding: "0.35rem 0.75rem",
                      background: "var(--accent)",
                      border: "none",
                      borderRadius: 4,
                      color: "var(--bg)",
                      cursor: loadingCompareDist ? "wait" : "pointer",
                      opacity: loadingCompareDist ? 0.7 : 1,
                    }}
                  >
                    {loadingCompareDist
                      ? "Running compare…"
                      : "Compare distributions (MC)"}
                  </button>
                  <p
                    style={{
                      margin: "6px 0 0",
                      fontSize: "0.75rem",
                      color: "var(--text-muted)",
                    }}
                  >
                    Re-runs Monte Carlo for selected crews only. Bars: rounds to
                    win (1–20, last bucket merges tail), hull remaining on clean
                    wins (10 bins), plus traced proc sample (~60 trials/crew).
                  </p>
                  {compareDistErr != null && (
                    <p
                      style={{
                        margin: "6px 0 0",
                        fontSize: "0.8rem",
                        color: "salmon",
                      }}
                    >
                      {compareDistErr}
                    </p>
                  )}
                </div>
              )}
              {compareDist != null && compareDist.length > 0 && (
                <div style={{ marginTop: 14 }}>
                  <strong style={{ fontSize: "0.9rem" }}>
                    Distribution comparison
                  </strong>
                  {compareSeed != null && (
                    <span
                      style={{
                        marginLeft: 8,
                        fontSize: "0.75rem",
                        color: "var(--text-muted)",
                      }}
                    >
                      seed={compareSeed}
                    </span>
                  )}
                  <SideBySideHistograms
                    crews={compareDist}
                    title="Rounds to win (clean wins only; bucket 20 = 20+)"
                    getCounts={(c) => c.rounds_histogram.map(([, n]) => n)}
                  />
                  <SideBySideHistograms
                    crews={compareDist}
                    title="Attacker hull % on clean wins (10 bins, 0–10% … 90–100%)"
                    getCounts={(c) => c.hull_remaining_bins}
                  />
                  {compareDist.some(
                    (c) => c.proc_rates && Object.keys(c.proc_rates).length > 0,
                  ) && (
                    <div style={{ marginTop: 12 }}>
                      <div
                        style={{
                          fontSize: "0.75rem",
                          color: "var(--text-muted)",
                          marginBottom: 6,
                        }}
                      >
                        Proc-like events (mean count per traced trial)
                      </div>
                      <div
                        style={{ display: "flex", flexWrap: "wrap", gap: 12 }}
                      >
                        {compareDist.map((c, i) => (
                          <div
                            key={`proc-${i}`}
                            style={{ fontSize: "0.75rem", minWidth: 120 }}
                          >
                            <div style={{ fontWeight: 600 }}>{c.captain}</div>
                            <ul style={{ margin: "4px 0 0", paddingLeft: 16 }}>
                              {Object.entries(c.proc_rates ?? {})
                                .sort(([a], [b]) => a.localeCompare(b))
                                .map(([k, v]) => (
                                  <li key={k}>
                                    {k}: {v.toFixed(2)}
                                  </li>
                                ))}
                            </ul>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
        </>
      )}

      {!hasSim && !hasRecs && !loadingSim && !loadingOptimize && (
        <p
          style={{ margin: 0, color: "var(--text-muted)", fontSize: "0.9rem" }}
        >
          Run Sim for current crew or Run Optimize for ranked recommendations.
        </p>
      )}
    </section>
  );
}
