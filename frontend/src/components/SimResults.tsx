import {
  type CSSProperties,
  memo,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import {
  type ChainLootSummary,
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
import {
  EMPTY_RECOMMENDATION_FILTERS,
  filterRecommendations,
  formatDiscoveryLaneLabel,
  type RecommendationFilters,
  summarizePortfolioResults,
} from "../lib/portfolioAnalytics";
import CrewComparisonPanel from "./results/CrewComparisonPanel";
import PortfolioAnalyticsBar from "./results/PortfolioAnalyticsBar";

const PER_PAGE_OPTIONS = [50, 100, 200, 500] as const;
const DEFAULT_PER_PAGE = 50;
const EMPTY_SELECTED_ROWS: ReadonlySet<number> = new Set();

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

function formatCompactLoot(value: number): string {
  if (!Number.isFinite(value)) return "—";
  if (value >= 1_000_000_000) return `${(value / 1_000_000_000).toFixed(2)}B`;
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(2)}M`;
  if (value >= 10_000) return `${(value / 1_000).toFixed(1)}k`;
  return Math.round(value).toLocaleString();
}

function formatChainLootTooltip(loot: ChainLootSummary): string {
  const lines = [
    `Expected ${loot.resource_name}: ${Math.round(loot.expected_loot_mean).toLocaleString()} (95% CI: ${Math.round(loot.expected_loot_ci_low).toLocaleString()}–${Math.round(loot.expected_loot_ci_high).toLocaleString()})`,
    `Base drop/kill: ${Math.round(loot.base_drop_per_kill).toLocaleString()} · Multiplier: ${loot.loot_multiplier.toFixed(2)}×`,
  ];
  if (loot.cargo_capacity != null) {
    lines.push(
      `Cargo capacity: ${Math.round(loot.cargo_capacity).toLocaleString()}${loot.cargo_capped ? " (cargo hold full)" : ""}`,
    );
  }
  if (loot.loot_sources && loot.loot_sources.length > 0) {
    lines.push(`Officer bonuses: ${loot.loot_sources.join(", ")}`);
  }
  return lines.join("\n");
}

function formatMethodProvenance(value: string | undefined): string {
  return formatDiscoveryLaneLabel(value);
}

function fidelityBadgeStyle(grade: string): CSSProperties {
  const palette: Record<string, { border: string; color: string }> = {
    A: { border: "rgba(67,181,129,0.5)", color: "#43b581" },
    B: { border: "rgba(99,179,237,0.5)", color: "#63b3ed" },
    C: { border: "rgba(232,149,46,0.55)", color: "#e8952e" },
    D: { border: "rgba(240,71,71,0.55)", color: "#f04747" },
  };
  const colors = palette[grade] ?? {
    border: "var(--border)",
    color: "var(--text-muted)",
  };
  return {
    display: "inline-block",
    padding: "1px var(--space-4)",
    border: `1px solid ${colors.border}`,
    borderRadius: "var(--radius-1)",
    color: colors.color,
    fontSize: "0.72rem",
    fontWeight: 600,
    lineHeight: 1.5,
    fontVariantNumeric: "tabular-nums",
  };
}

function formatFidelityTooltip(r: CrewRecommendation): string | undefined {
  const parts: string[] = [];
  if (r.fidelity) {
    const f = r.fidelity;
    parts.push(
      `Modeling fidelity ${f.grade} (${f.coverage_pct.toFixed(1)}%): ${f.implemented} implemented, ${f.partial} partial, ${f.ignored} ignored across ${f.total_combat_effects} active combat effect(s).`,
    );
    if (f.caveats && f.caveats.length > 0) {
      parts.push(`Caveats:\n- ${f.caveats.join("\n- ")}`);
    }
  }
  if (r.uncertainty_reason) {
    parts.push(`Uncertainty: ${r.uncertainty_reason}`);
  }
  return parts.length > 0 ? parts.join("\n") : undefined;
}

function formatMethodTooltip(r: CrewRecommendation): string {
  const parts: string[] = [];
  if (r.refinement) {
    parts.push(formatRefinementChanges(r.refinement));
  } else if (r.method_provenance) {
    parts.push(
      `Primary source: ${formatMethodProvenance(r.method_provenance)}`,
    );
  }
  if (r.proposing_methods && r.proposing_methods.length > 1) {
    parts.push(
      `Proposed by: ${r.proposing_methods.map(formatMethodProvenance).join(", ")}`,
    );
  }
  if (r.qd_cell) {
    parts.push(`Archive cell: ${r.qd_cell}`);
  }
  return parts.join("\n");
}

function formatTrialsTooltip(r: CrewRecommendation): string {
  const parts: string[] = [
    `${r.trials_run} Monte Carlo trials backed this row`,
  ];
  if (r.evidence_level) {
    parts.push(`Evidence: ${r.evidence_level.replace(/_/g, " ")}`);
  }
  if (r.uncertainty_reason) {
    parts.push(r.uncertainty_reason);
  }
  return parts.join(" · ");
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

interface ComparisonState {
  recommendations: CrewRecommendation[];
  selected: ReadonlySet<number>;
  workspaceKey: string;
  loading: boolean;
  crews: CompareCrewDistribution[] | null;
  seed: number | null;
  error: string | null;
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
  const [selection, setSelection] = useState(() => ({
    recommendations,
    indices: new Set<number>(),
  }));
  // Selection belongs to a result set, not to positions in a later run. Guard
  // during render so even the first render of a shorter list is safe.
  const selected =
    selection.recommendations === recommendations
      ? selection.indices
      : EMPTY_SELECTED_ROWS;
  const [filterState, setFilterState] = useState<{
    recommendations: CrewRecommendation[];
    filters: RecommendationFilters;
  }>(() => ({
    recommendations,
    filters: EMPTY_RECOMMENDATION_FILTERS,
  }));
  const filters =
    filterState.recommendations === recommendations
      ? filterState.filters
      : EMPTY_RECOMMENDATION_FILTERS;
  const setFilters = useCallback(
    (next: RecommendationFilters) => {
      setFilterState({ recommendations, filters: next });
      setPage(1);
    },
    [recommendations],
  );
  const [page, setPage] = useState(1);
  const [perPage, setPerPage] = useState(DEFAULT_PER_PAGE);
  const [comparison, setComparison] = useState<ComparisonState | null>(null);
  const workspaceKey = JSON.stringify(
    compareWorkspace && [
      compareWorkspace.ship,
      compareWorkspace.hostile,
      compareWorkspace.shipTier,
      compareWorkspace.shipLevel,
      compareWorkspace.numSims,
      compareWorkspace.belowDecksSlots,
      compareWorkspace.profileId,
      compareWorkspace.supportBuffs ?? [],
    ],
  );
  const currentComparison =
    comparison?.recommendations === recommendations &&
    comparison.selected === selected &&
    comparison.workspaceKey === workspaceKey
      ? comparison
      : null;
  const compareDist = currentComparison?.crews ?? null;
  const compareSeed = currentComparison?.seed ?? null;
  const loadingCompareDist = currentComparison?.loading ?? false;
  const compareDistErr = currentComparison?.error ?? null;
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
  const chainLootMeta = useMemo(
    () => recommendations.find((r) => r.chain?.loot)?.chain?.loot ?? null,
    [recommendations],
  );
  const chainMode = chainMeta != null;
  const showChainLoot = chainMode && chainLootMeta != null;
  const linearEvalMode =
    optimizeEffectiveStrategy === "linear_eval" ||
    recommendations.some((r) => r.expected_hull_damage != null);
  const chainWinHeader = chainMode
    ? `P(${chainMeta.kills_target}-kill)`
    : "Win %";
  const chainLootHeader = chainLootMeta?.resource_name
    ? `Expected ${chainLootMeta.resource_name}`
    : "Expected loot";
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
  const showFidelity = recommendations.some((r) => r.fidelity != null);
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
              ...(showChainLoot ? [chainLootHeader] : []),
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
    [
      linearEvalMode,
      chainMode,
      chainWinHeader,
      chainR1Header,
      showChainLoot,
      chainLootHeader,
      chainHullHeader,
    ],
  );

  const portfolioSummary = useMemo(
    () => summarizePortfolioResults(recommendations),
    [recommendations],
  );
  const filteredRows = useMemo(
    () => filterRecommendations(recommendations, filters),
    [recommendations, filters],
  );
  const total = filteredRows.length;
  const totalPages = Math.max(1, Math.ceil(total / perPage));
  const safePage = Math.min(page, totalPages);
  const start = (safePage - 1) * perPage;
  const pageRecs = useMemo(
    () => filteredRows.slice(start, start + perPage),
    [filteredRows, start, perPage],
  );

  // Reset to page 1 when recommendations change (e.g. new optimize run) or when current page is out of range
  useEffect(() => {
    if (page > totalPages && totalPages >= 1) setPage(1);
  }, [totalPages, total]);

  useEffect(() => {
    setSelection((current) =>
      current.recommendations === recommendations
        ? current
        : { recommendations, indices: new Set<number>() },
    );
    setFilterState((current) =>
      current.recommendations === recommendations
        ? current
        : { recommendations, filters: EMPTY_RECOMMENDATION_FILTERS },
    );
    setComparison((current) =>
      current?.recommendations === recommendations ? current : null,
    );
  }, [recommendations]);

  const selectDiverseTop = useCallback(
    (indices: number[]) => {
      setSelection({
        recommendations,
        indices: new Set(indices.slice(0, 5)),
      });
    },
    [recommendations],
  );

  const toggleSelect = useCallback(
    (i: number) => {
      setSelection((prev) => {
        const current =
          prev.recommendations === recommendations
            ? prev.indices
            : EMPTY_SELECTED_ROWS;
        const next = new Set(current);
        if (next.has(i)) next.delete(i);
        else if (next.size < 5) next.add(i);
        else return prev;
        return { recommendations, indices: next };
      });
    },
    [recommendations],
  );

  const selectedList = useMemo(
    () => Array.from(selected).sort((a, b) => a - b),
    [selected],
  );
  const totalSelected = selected.size;
  const showCompare =
    !linearEvalMode && selectedList.length >= 2 && selectedList.length <= 5;

  const runCompareDistributions = useCallback(async () => {
    if (!compareWorkspace || !showCompare) return;
    const request: ComparisonState = {
      recommendations,
      selected,
      workspaceKey,
      loading: true,
      crews: null,
      seed: null,
      error: null,
    };
    setComparison(request);
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
      // A newer request owns the state as soon as it starts. Context guards
      // above also hide responses for a different selection/scenario/result set.
      setComparison((current) =>
        current === request
          ? { ...request, loading: false, crews: res.crews, seed: res.seed }
          : current,
      );
    } catch (e) {
      setComparison((current) =>
        current === request
          ? { ...request, loading: false, error: formatApiError(e) }
          : current,
      );
    }
  }, [
    compareWorkspace,
    recommendations,
    selected,
    selectedList,
    showCompare,
    workspaceKey,
  ]);

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
                fight. First column is chain completion rate (Wilson CI).{" "}
                {showChainLoot ? (
                  <>
                    &quot;{chainLootHeader}&quot; shows expected payout per
                    sortie (hostile drops, applicable officer loot bonuses, and
                    cargo hold cap; hover for breakdown). &quot;Hull
                    %*|hit&quot; shows ending hull after completing all{" "}
                    {chainMeta.kills_target} kills.
                  </>
                ) : (
                  <>
                    &quot;{chainHullHeader}&quot; is the secondary mean given a
                    successful chain (normal approx CI).
                  </>
                )}{" "}
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
          <PortfolioAnalyticsBar
            summary={portfolioSummary}
            filteredCount={total}
            filters={filters}
            onChangeFilters={setFilters}
            onSelectDiverseTop={selectDiverseTop}
            linearEvalMode={linearEvalMode}
          />
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
              {total === 0
                ? "Showing 0 of 0"
                : `Showing ${start + 1}–${Math.min(start + perPage, total)} of ${total}`}
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
                  {showFidelity && (
                    <th
                      style={styles.thText}
                      title="Active-seat combat modeling coverage grade (A–D) for this crew. Hover a badge for caveats."
                    >
                      Fidelity
                    </th>
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
                {pageRecs.map(({ rec: r, originalIndex }, i) => {
                  const globalIndex = originalIndex;
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
                          title={formatMethodTooltip(r)}
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
                      {showFidelity && (
                        <td
                          style={styles.tdWhy}
                          title={formatFidelityTooltip(r)}
                        >
                          {r.fidelity ? (
                            <span style={fidelityBadgeStyle(r.fidelity.grade)}>
                              {`${r.fidelity.grade} ${Math.round(r.fidelity.coverage_pct)}%`}
                            </span>
                          ) : (
                            "—"
                          )}
                        </td>
                      )}
                      {showTrialsRun && (
                        <td
                          style={styles.tdNumeric}
                          title={formatTrialsTooltip(r)}
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
                          {showChainLoot && (
                            <td
                              style={styles.tdNumeric}
                              title={
                                r.chain?.loot
                                  ? formatChainLootTooltip(r.chain.loot)
                                  : undefined
                              }
                            >
                              {r.chain?.loot ? (
                                <>
                                  {formatCompactLoot(
                                    r.chain.loot.expected_loot_mean,
                                  )}
                                  {r.chain.loot.cargo_capped && (
                                    <span
                                      style={{
                                        marginLeft: 4,
                                        fontSize: "0.72rem",
                                        color: "var(--warning)",
                                      }}
                                    >
                                      (full)
                                    </span>
                                  )}
                                </>
                              ) : (
                                "—"
                              )}
                            </td>
                          )}
                          <td style={styles.tdNumeric}>
                            {chainMode &&
                            chainMeta.secondary_objective ===
                              "max_loot_per_hull_proxy"
                              ? r.avg_hull_remaining.toFixed(2)
                              : formatPctWithCi(
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
            <CrewComparisonPanel
              recommendations={recommendations}
              selectedIndices={selectedList}
              chainMode={chainMode}
              canRunDistributions={Boolean(
                compareWorkspace?.ship && compareWorkspace.hostile,
              )}
              loadingCompareDist={loadingCompareDist}
              compareDist={compareDist}
              compareSeed={compareSeed}
              compareDistErr={compareDistErr}
              onRunCompareDistributions={() => void runCompareDistributions()}
              formatParetoTag={formatParetoTag}
              visibleParetoTags={visibleParetoTags}
            />
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
