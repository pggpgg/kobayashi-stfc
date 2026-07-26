import {
  type CSSProperties,
  memo,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import {
  type CompareCrewDistribution,
  type CrewRecommendation,
  compareCrewsDistributions,
  crewRecommendationToSimulateCrew,
  formatApiError,
  formatOptimizePhaseLabel,
  type ParetoTag,
  type RefinementDetail,
  type RefinementSlotChange,
  type SimulateStats,
} from "../lib/api";

const PER_PAGE_OPTIONS = [50, 100, 200, 500] as const;
const DEFAULT_PER_PAGE = 50;

const TABLE_CELL_PAD = "var(--space-6) var(--space-8)";
const TABLE_NUM_PAD = "var(--space-6) var(--space-5)";
const CREW_CELL_MAX_CH = 42;

/** Sticky header cell shared by every results-table column. */
const thBase: CSSProperties = {
  position: "sticky",
  top: 0,
  zIndex: 2,
  background: "var(--surface)",
  borderBottom: "1px solid var(--border)",
};

/** Repeated style objects, hoisted so each is defined once (and not re-allocated per render). */
const styles = {
  thText: { ...thBase, textAlign: "left", padding: TABLE_CELL_PAD },
  thCheckbox: {
    ...thBase,
    textAlign: "left",
    padding: TABLE_NUM_PAD,
    width: 34,
  },
  thIndex: {
    ...thBase,
    textAlign: "left",
    padding: TABLE_NUM_PAD,
    width: 56,
    color: "var(--text-muted)",
    fontWeight: 600,
  },
  thNumeric: {
    ...thBase,
    textAlign: "right",
    padding: TABLE_NUM_PAD,
    whiteSpace: "nowrap",
    fontVariantNumeric: "tabular-nums",
  },
  tdIndex: {
    padding: TABLE_NUM_PAD,
    color: "var(--text-muted)",
    fontVariantNumeric: "tabular-nums",
  },
  tdCrew: {
    padding: TABLE_CELL_PAD,
    whiteSpace: "nowrap",
    maxWidth: 360,
    overflow: "hidden",
    textOverflow: "ellipsis",
  },
  tdMethod: {
    padding: TABLE_CELL_PAD,
    color: "var(--text-muted)",
    whiteSpace: "nowrap",
    fontSize: "0.82rem",
  },
  tdWhy: {
    padding: TABLE_CELL_PAD,
    whiteSpace: "nowrap",
  },
  whyBadge: {
    display: "inline-block",
    padding: "1px var(--space-4)",
    marginRight: "var(--space-4)",
    border: "1px solid var(--border)",
    borderRadius: "var(--radius-1)",
    color: "var(--text-muted)",
    fontSize: "0.72rem",
    lineHeight: 1.6,
  },
  tdNumeric: {
    padding: TABLE_NUM_PAD,
    textAlign: "right",
    whiteSpace: "nowrap",
    fontVariantNumeric: "tabular-nums",
  },
  control: {
    padding: "var(--space-2) var(--space-8)",
    background: "var(--bg)",
    border: "1px solid var(--border)",
    borderRadius: "var(--radius-1)",
    color: "var(--text)",
  },
  sectionLabel: {
    fontSize: "0.75rem",
    color: "var(--text-muted)",
    marginBottom: "var(--space-4)",
  },
} satisfies Record<string, CSSProperties>;

/** Normalize captain/bridge/below_decks for display: API may return string[]; join with ", ". */
function formatCrewCell(value: string | string[] | null | undefined): string {
  if (value == null) return "";
  if (Array.isArray(value)) return value.filter(Boolean).join(", ");
  return String(value);
}

function truncateMiddle(s: string, max: number): string {
  const t = s.trim();
  if (t.length <= max) return t;
  const left = Math.max(1, Math.floor((max - 1) * 0.6));
  const right = Math.max(1, max - 1 - left);
  return `${t.slice(0, left)}…${t.slice(t.length - right)}`;
}

/** Percent point estimate with 95% CI in parentheses (Wilson/normal per server notes). */
function formatPctWithCi(p: number, lo: number, hi: number): string {
  const main = (p * 100).toFixed(2);
  const a = (lo * 100).toFixed(1);
  const b = (hi * 100).toFixed(1);
  return `${main}\u00a0(${a}\u2013${b})`;
}

function formatExpectedHullDamage(value: number | undefined): string {
  if (value == null || !Number.isFinite(value)) return "—";
  return Math.round(value).toLocaleString();
}

function formatMethodProvenance(value: string | undefined): string {
  const map: Record<string, string> = {
    curated_warm_start: "Curated",
    exhaustive_mc: "Exhaustive MC",
    exhaustive_two_phase: "Two-phase MC",
    genetic: "Genetic",
    heuristic_seed: "Heuristic seed",
    heuristics: "Heuristics",
    large_neighborhood_repair: "Rebuilt seats",
    linear_eval: "Linear eval",
    local_captain_swap: "Captain swap",
    local_swap: "Seat swap",
    monte_carlo: "Monte Carlo",
    seeded_genetic: "Seeded GA",
    tiered_confirmed: "Tiered",
    warm_start: "Warm start",
  };
  return value != null ? (map[value] ?? value.replace(/_/g, " ")) : "";
}

/** Display order for recommendation badges: named views first, front membership last. */
const PARETO_TAG_ORDER: readonly ParetoTag[] = [
  "safest",
  "fastest_farming",
  "best_chain",
  "most_different",
  "pareto_optimal",
] as const;

const PARETO_TAG_LABELS: Record<ParetoTag, string> = {
  safest: "Safest",
  fastest_farming: "Fastest",
  best_chain: "Best chain",
  most_different: "Different",
  pareto_optimal: "Pareto",
};

/**
 * Badges to render for a row. A named view already implies front membership, so `Pareto` shows only
 * when it is the whole story — otherwise every strong row would carry a badge that says nothing
 * about how it differs from the row above it.
 */
export function visibleParetoTags(
  tags: readonly ParetoTag[] | undefined,
): ParetoTag[] {
  if (tags == null || tags.length === 0) return [];
  const named = PARETO_TAG_ORDER.filter(
    (tag) => tag !== "pareto_optimal" && tags.includes(tag),
  );
  if (named.length > 0) return named;
  return tags.includes("pareto_optimal") ? ["pareto_optimal"] : [];
}

export function formatParetoTag(tag: ParetoTag): string {
  return PARETO_TAG_LABELS[tag] ?? tag;
}

/**
 * One line per changed seat, e.g. `Bridge: Kirk → Spock`. Used as the Method cell's tooltip on a
 * refined row, so the row can say which officers refinement moved rather than only that it ran.
 */
function formatRefinementChanges(refinement: RefinementDetail): string {
  const groupLabel: Record<RefinementSlotChange["slot"], string> = {
    captain: "Captain",
    bridge: "Bridge",
    below_decks: "Below deck",
  };
  const seats = refinement.changed_slots
    .map((change) => {
      const seat =
        change.index != null
          ? `${groupLabel[change.slot]} ${change.index + 1}`
          : groupLabel[change.slot];
      return `${seat}: ${change.from} → ${change.to}`;
    })
    .join("\n");
  const gain = (refinement.gain * 100).toFixed(2);
  return `Refined from a finalist crew (+${gain} score):\n${seats}`;
}

/** Compact marker for how deeply a row was confirmed, or null when the API did not report it. */
function formatTrialsRun(value: number | undefined): string | null {
  if (value == null || !Number.isFinite(value) || value <= 0) return null;
  return value >= 1000
    ? `${(value / 1000).toFixed(value % 1000 === 0 ? 0 : 1)}k`
    : String(value);
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
      <div style={styles.sectionLabel}>{title}</div>
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
  /** Trust/fidelity notices returned by the simulation or optimizer API. */
  warnings?: string[];
  /** Officers that contributed no combat effects because LCARS resolution failed. */
  unresolvedOfficers?: string[];
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
  /** Last optimize run effective strategy (from API scenario.effective_strategy). */
  optimizeEffectiveStrategy?: string | null;
}

export default memo(function SimResults({
  simResult,
  recommendations,
  warnings = [],
  unresolvedOfficers = [],
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
  optimizeEffectiveStrategy = null,
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
  const visibleWarnings = unresolvedOfficers.length
    ? warnings.filter(
        (warning) =>
          !warning.startsWith(
            "Officer(s) with no LCARS combat definition contributed no effects:",
          ),
      )
    : warnings;
  const hasResultWarnings =
    visibleWarnings.length > 0 || unresolvedOfficers.length > 0;

  const chainMeta = useMemo(
    () => recommendations.find((r) => r.chain)?.chain,
    [recommendations],
  );
  const chainMode = chainMeta != null;
  const linearEvalMode =
    optimizeEffectiveStrategy === "linear_eval" ||
    recommendations.some((r) => r.expected_hull_damage != null);
  const chainWinHeader = chainMode
    ? `P(${chainMeta.kills_target}-kill)`
    : "Win %";
  const chainHullHeader =
    chainMode && chainMeta.secondary_objective === "max_loot_per_hull_proxy"
      ? "Loot proxy*|hit"
      : chainMode
        ? "Hull %*|hit"
        : "Your hull %";
  const chainR1Header = chainMode ? "R1 (1st link)" : "R1 %";
  const showMethodProvenance = recommendations.some(
    (r) => r.method_provenance != null && r.method_provenance.trim() !== "",
  );
  // Runs the tagging pass declines (linear eval, single result) return no tags at all — no column.
  const showWhy = useMemo(
    () =>
      recommendations.some((r) => visibleParetoTags(r.pareto_tags).length > 0),
    [recommendations],
  );
  // Only worth a column when the rows differ in depth: a table where every row was confirmed at the
  // same budget says nothing the run settings do not already say.
  const showTrialsRun = useMemo(() => {
    const depths = new Set(
      recommendations
        .map((r) => r.trials_run)
        .filter((n): n is number => n != null && n > 0),
    );
    return depths.size > 1;
  }, [recommendations]);
  const numericTableHeaders = useMemo(
    () =>
      linearEvalMode
        ? ["Expected hull damage"]
        : chainMode
          ? [
              chainWinHeader,
              "Stall %",
              "Loss %",
              chainR1Header,
              chainHullHeader,
              "Enemy hull %",
            ]
          : [
              "Win %",
              "Stall %",
              "Loss %",
              "R1 %",
              "Your hull %",
              "Enemy hull %",
            ],
    [linearEvalMode, chainMode, chainWinHeader, chainR1Header, chainHullHeader],
  );

  const total = recommendations.length;
  const totalPages = Math.max(1, Math.ceil(total / perPage));
  const safePage = Math.min(page, totalPages);
  const start = (safePage - 1) * perPage;
  const pageRecs = useMemo(
    () => recommendations.slice(start, start + perPage),
    [recommendations, start, perPage],
  );

  // Reset to page 1 when recommendations change (e.g. new optimize run) or when current page is out of range
  useEffect(() => {
    if (page > totalPages && totalPages >= 1) setPage(1);
  }, [totalPages, total]);

  useEffect(() => {
    setCompareDist(null);
    setCompareSeed(null);
    setCompareDistErr(null);
  }, [recommendations]);

  const toggleSelect = useCallback((i: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(i)) next.delete(i);
      else if (next.size < 5) next.add(i);
      return next;
    });
  }, []);

  const selectedList = useMemo(
    () => Array.from(selected).sort((a, b) => a - b),
    [selected],
  );
  const totalSelected = selected.size;
  const showCompare =
    !linearEvalMode && selectedList.length >= 2 && selectedList.length <= 5;

  const runCompareDistributions = useCallback(async () => {
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
  }, [compareWorkspace, recommendations, selectedList, showCompare]);

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

      {hasResultWarnings && (
        <div
          role="status"
          aria-live="polite"
          style={{
            marginBottom: "0.85rem",
            padding: "0.7rem 0.8rem",
            background: "rgba(232,149,46,0.12)",
            border: "1px solid var(--warning)",
            borderRadius: 6,
            fontSize: "0.85rem",
            lineHeight: 1.45,
          }}
        >
          <strong>Review these results</strong>
          <ul style={{ margin: "0.4rem 0 0", paddingLeft: "1.2rem" }}>
            {unresolvedOfficers.length > 0 && (
              <li>
                Unresolved officers contributed no combat effects:{" "}
                {unresolvedOfficers.join(", ")}.
              </li>
            )}
            {visibleWarnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
        </div>
      )}

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
              Your hull remaining (wins):{" "}
              {(simResult.avg_hull_remaining * 100).toFixed(2)}%
            </span>
            <span>
              Enemy hull remaining (avg):{" "}
              {(simResult.avg_defender_hull_remaining * 100).toFixed(2)}%
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
          {linearEvalMode && (
            <p
              style={{
                margin: "0 0 0.5rem",
                padding: "0.5rem 0.65rem",
                fontSize: "0.85rem",
                color: "var(--text-muted)",
                background: "rgba(232,149,46,0.08)",
                border: "1px solid var(--border)",
                borderRadius: 6,
              }}
            >
              Approximate ranking by expected hull damage — win rates were not
              simulated. Use tiered or exhaustive to confirm crews in combat.
            </p>
          )}
          <p
            style={{
              margin: "0 0 0.5rem",
              fontSize: "0.85rem",
              color: "var(--text-muted)",
            }}
          >
            {linearEvalMode ? (
              <>
                Rows sorted by closed-form expected hull damage over the fight
                length. Compare is disabled (requires Monte Carlo).
              </>
            ) : chainMode ? (
              <>
                Select 2–5 rows to compare. <strong>Chain grind:</strong>{" "}
                {chainMeta.kills_target} consecutive wins vs the same hostile;
                attacker hull carries between fights; shields start full each
                fight. First column is chain completion rate (Wilson CI).
                &quot;Hull %*|hit&quot; / &quot;Loot proxy*|hit&quot; is the
                secondary mean given a successful chain (normal approx CI).
                Stall/loss/R1 follow the first link.
              </>
            ) : (
              <>
                Select 2–5 rows to compare. Optimize columns show point % (95%
                CI): Wilson for win/stall/loss/R1; normal approx for hull scores
                per trial (your hull on wins; enemy hull all trials).
              </>
            )}
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
                style={styles.control}
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
            <span style={{ color: "var(--text-muted)" }}>
              Selected {totalSelected}/5
            </span>
            {totalPages > 1 && (
              <span style={{ display: "flex", alignItems: "center", gap: 4 }}>
                <button
                  type="button"
                  onClick={() => setPage((p) => Math.max(1, p - 1))}
                  disabled={safePage <= 1}
                  aria-label="Previous page"
                  style={{
                    ...styles.control,
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
                    ...styles.control,
                    cursor: safePage >= totalPages ? "not-allowed" : "pointer",
                    opacity: safePage >= totalPages ? 0.6 : 1,
                  }}
                >
                  Next
                </button>
              </span>
            )}
          </div>
          <div
            style={{
              border: "1px solid var(--border)",
              borderRadius: 6,
              overflow: "auto",
              maxHeight: "62vh",
              background: "var(--surface)",
            }}
          >
            <table
              style={{
                width: "100%",
                borderCollapse: "separate",
                borderSpacing: 0,
                fontSize: "0.9rem",
              }}
            >
              <thead>
                <tr>
                  <th style={styles.thCheckbox} />
                  <th style={styles.thIndex}>#</th>
                  <th style={styles.thText}>Captain</th>
                  <th style={styles.thText}>Bridge</th>
                  <th style={styles.thText}>Below Deck</th>
                  {showWhy && (
                    <th
                      style={styles.thText}
                      title="What this crew leads on, beyond its rank. Hover a badge for the full reason."
                    >
                      Why
                    </th>
                  )}
                  {showMethodProvenance && (
                    <th style={styles.thText}>Method</th>
                  )}
                  {showTrialsRun && (
                    <th
                      style={styles.thNumeric}
                      title="Monte Carlo trials actually run for this row — its depth of evidence"
                    >
                      Trials
                    </th>
                  )}
                  {numericTableHeaders.map((h) => (
                    <th key={h} style={styles.thNumeric}>
                      {h}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {pageRecs.map((r, i) => {
                  const globalIndex = start + i;
                  const isSelected = selected.has(globalIndex);
                  const crewDisabled = !isSelected && totalSelected >= 5;
                  const rowBg =
                    i % 2 === 1 ? "rgba(255,255,255,0.02)" : "transparent";
                  return (
                    <tr
                      key={globalIndex}
                      title={
                        crewDisabled
                          ? "Selection limit reached (max 5). Unselect a row to add another."
                          : undefined
                      }
                      style={{
                        borderBottom: "1px solid var(--border)",
                        background: isSelected
                          ? "rgba(232,149,46,0.12)"
                          : rowBg,
                        opacity: crewDisabled ? 0.7 : 1,
                      }}
                      onClick={(e) => {
                        // Let checkbox manage itself; otherwise row click toggles.
                        if ((e.target as HTMLElement).tagName === "INPUT") {
                          return;
                        }
                        toggleSelect(globalIndex);
                      }}
                    >
                      <td style={{ padding: TABLE_NUM_PAD }}>
                        <input
                          type="checkbox"
                          checked={isSelected}
                          disabled={crewDisabled}
                          onChange={() => toggleSelect(globalIndex)}
                          aria-label={`Select row ${globalIndex + 1}`}
                        />
                      </td>
                      <td style={styles.tdIndex}>{globalIndex + 1}</td>
                      {(
                        [
                          { label: "Captain", value: r.captain },
                          { label: "Bridge", value: r.bridge },
                          { label: "Below Deck", value: r.below_decks },
                        ] as const
                      ).map((c) => {
                        const full = formatCrewCell(c.value);
                        const shown = truncateMiddle(full, CREW_CELL_MAX_CH);
                        return (
                          <td key={c.label} style={styles.tdCrew} title={full}>
                            {shown}
                          </td>
                        );
                      })}
                      {showWhy && (
                        <td
                          style={styles.tdWhy}
                          title={r.recommendation_reason ?? undefined}
                        >
                          {visibleParetoTags(r.pareto_tags).map((tag) => (
                            <span key={tag} style={styles.whyBadge}>
                              {formatParetoTag(tag)}
                            </span>
                          ))}
                        </td>
                      )}
                      {showMethodProvenance && (
                        <td
                          style={styles.tdMethod}
                          title={
                            r.refinement
                              ? formatRefinementChanges(r.refinement)
                              : (r.method_provenance ?? "")
                          }
                        >
                          {formatMethodProvenance(r.method_provenance)}
                          {r.refinement && (
                            <span
                              style={{
                                color: "var(--text-muted)",
                                marginLeft: "var(--space-4)",
                              }}
                            >
                              {`+${(r.refinement.gain * 100).toFixed(1)}`}
                            </span>
                          )}
                        </td>
                      )}
                      {showTrialsRun && (
                        <td
                          style={styles.tdNumeric}
                          title={`${r.trials_run} Monte Carlo trials backed this row`}
                        >
                          {formatTrialsRun(r.trials_run) ?? "—"}
                        </td>
                      )}
                      {linearEvalMode ? (
                        <td style={styles.tdNumeric}>
                          {formatExpectedHullDamage(r.expected_hull_damage)}
                        </td>
                      ) : (
                        <>
                          <td style={styles.tdNumeric}>
                            {formatPctWithCi(
                              r.win_rate,
                              r.win_rate_ci_low,
                              r.win_rate_ci_high,
                            )}
                          </td>
                          <td style={styles.tdNumeric}>
                            {formatPctWithCi(
                              r.stall_rate,
                              r.stall_rate_ci_low,
                              r.stall_rate_ci_high,
                            )}
                          </td>
                          <td style={styles.tdNumeric}>
                            {formatPctWithCi(
                              r.loss_rate,
                              r.loss_rate_ci_low,
                              r.loss_rate_ci_high,
                            )}
                          </td>
                          <td style={styles.tdNumeric}>
                            {formatPctWithCi(
                              r.r1_kill_rate,
                              r.r1_kill_rate_ci_low,
                              r.r1_kill_rate_ci_high,
                            )}
                          </td>
                          <td style={styles.tdNumeric}>
                            {formatPctWithCi(
                              r.avg_hull_remaining,
                              r.avg_hull_remaining_ci_low,
                              r.avg_hull_remaining_ci_high,
                            )}
                          </td>
                          <td style={styles.tdNumeric}>
                            {formatPctWithCi(
                              r.avg_defender_hull_remaining,
                              r.avg_defender_hull_remaining_ci_low,
                              r.avg_defender_hull_remaining_ci_high,
                            )}
                          </td>
                        </>
                      )}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>

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
                  const deltaEnemyHull =
                    prev != null
                      ? (r.avg_defender_hull_remaining -
                          prev.avg_defender_hull_remaining) *
                        100
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
                          Δ {chainMode ? "P(chain)" : "Win"}{" "}
                          {deltaWin >= 0 ? "+" : ""}
                          {deltaWin.toFixed(2)}%, Δ Stall{" "}
                          {deltaStall >= 0 ? "+" : ""}
                          {deltaStall.toFixed(2)}%, Δ Loss{" "}
                          {deltaLoss >= 0 ? "+" : ""}
                          {deltaLoss.toFixed(2)}%, Δ R1{" "}
                          {deltaR1 >= 0 ? "+" : ""}
                          {deltaR1.toFixed(2)}%, Δ{" "}
                          {chainMode ? "Secondary" : "Your hull"}{" "}
                          {deltaHull >= 0 ? "+" : ""}
                          {deltaHull.toFixed(2)}%, Δ Enemy hull{" "}
                          {deltaEnemyHull >= 0 ? "+" : ""}
                          {deltaEnemyHull.toFixed(2)}%
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
                      <div style={styles.sectionLabel}>
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
});
