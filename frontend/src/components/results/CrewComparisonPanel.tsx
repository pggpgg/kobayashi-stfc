import type { CSSProperties } from "react";
import type {
  CompareCrewDistribution,
  CrewRecommendation,
  ParetoTag,
} from "../../lib/api";
import {
  diffCrewSeats,
  formatDiscoveryLaneLabel,
  parseQdCell,
  rowDiscoveryLanes,
} from "../../lib/portfolioAnalytics";

interface CrewComparisonPanelProps {
  recommendations: CrewRecommendation[];
  selectedIndices: number[];
  chainMode: boolean;
  canRunDistributions: boolean;
  loadingCompareDist: boolean;
  compareDist: CompareCrewDistribution[] | null;
  compareSeed: number | null;
  compareDistErr: string | null;
  onRunCompareDistributions: () => void;
  formatParetoTag: (tag: ParetoTag) => string;
  visibleParetoTags: (tags: readonly ParetoTag[] | undefined) => ParetoTag[];
}

const sectionLabelStyle: CSSProperties = {
  fontSize: "0.75rem",
  color: "var(--text-muted)",
  marginBottom: "var(--space-4)",
};

const tagStyle: CSSProperties = {
  display: "inline-block",
  padding: "1px 6px",
  borderRadius: 4,
  border: "1px solid var(--border)",
  fontSize: "0.7rem",
  color: "var(--text-muted)",
  background: "var(--surface)",
};

function formatCrewCell(value: string | string[] | null | undefined): string {
  if (value == null) return "";
  if (Array.isArray(value)) return value.filter(Boolean).join(", ");
  return String(value);
}

function formatSignedDelta(value: number): string {
  return `${value >= 0 ? "+" : ""}${value.toFixed(2)}%`;
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
      <div style={sectionLabelStyle}>{title}</div>
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

export default function CrewComparisonPanel({
  recommendations,
  selectedIndices,
  chainMode,
  canRunDistributions,
  loadingCompareDist,
  compareDist,
  compareSeed,
  compareDistErr,
  onRunCompareDistributions,
  formatParetoTag,
  visibleParetoTags,
}: CrewComparisonPanelProps) {
  const baseline = recommendations[selectedIndices[0]];
  if (!baseline) return null;

  return (
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

      {/* Compact sequential delta summary */}
      <div
        style={{
          marginTop: 8,
          display: "flex",
          flexDirection: "column",
          gap: 4,
        }}
      >
        {selectedIndices.map((idx, j) => {
          const r = recommendations[idx];
          const prev = j === 0 ? null : recommendations[selectedIndices[j - 1]];
          const deltaWin =
            prev != null ? (r.win_rate - prev.win_rate) * 100 : 0;
          const deltaStall =
            prev != null ? (r.stall_rate - prev.stall_rate) * 100 : 0;
          const deltaLoss =
            prev != null ? (r.loss_rate - prev.loss_rate) * 100 : 0;
          const deltaR1 =
            prev != null ? (r.r1_kill_rate - prev.r1_kill_rate) * 100 : 0;
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
                <span style={{ marginLeft: 8, color: "var(--text-muted)" }}>
                  Δ {chainMode ? "P(chain)" : "Win"}{" "}
                  {formatSignedDelta(deltaWin)}, Δ Stall{" "}
                  {formatSignedDelta(deltaStall)}, Δ Loss{" "}
                  {formatSignedDelta(deltaLoss)}, Δ R1{" "}
                  {formatSignedDelta(deltaR1)}, Δ{" "}
                  {chainMode ? "Secondary" : "Your hull"}{" "}
                  {formatSignedDelta(deltaHull)}, Δ Enemy hull{" "}
                  {formatSignedDelta(deltaEnemyHull)}
                </span>
              )}
            </div>
          );
        })}
      </div>

      {/* Side-by-side seat diffs, portfolio archive cells, fidelity & caveats */}
      <section
        aria-label="Side-by-side crew breakdown"
        style={{
          marginTop: 12,
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))",
          gap: "0.6rem",
        }}
      >
        {selectedIndices.map((idx, j) => {
          const r = recommendations[idx];
          const seatDiff = j === 0 ? null : diffCrewSeats(baseline, r);
          const cell = parseQdCell(r.qd_cell);
          const lanes = rowDiscoveryLanes(r);
          const tags = visibleParetoTags(r.pareto_tags);

          return (
            <div
              key={`card-${idx}`}
              style={{
                padding: "0.6rem 0.7rem",
                border: "1px solid var(--border)",
                borderRadius: 6,
                background: "var(--surface)",
                fontSize: "0.78rem",
                display: "flex",
                flexDirection: "column",
                gap: 6,
              }}
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  gap: 6,
                }}
              >
                <strong>
                  #{idx + 1} {r.captain}
                </strong>
                <span
                  style={{ color: "var(--text-muted)", fontSize: "0.72rem" }}
                >
                  {j === 0
                    ? "Baseline"
                    : `${seatDiff?.totalSwappedOfficers ?? 0} swap${seatDiff?.totalSwappedOfficers === 1 ? "" : "s"} vs #${selectedIndices[0] + 1}`}
                </span>
              </div>

              {/* Seat diff details */}
              {seatDiff && (
                <div
                  style={{
                    fontSize: "0.73rem",
                    color: "var(--text-muted)",
                    display: "flex",
                    flexDirection: "column",
                    gap: 2,
                  }}
                >
                  {seatDiff.captainChanged && (
                    <div>
                      Captain: {seatDiff.baselineCaptain} →{" "}
                      <span style={{ color: "var(--text)" }}>
                        {seatDiff.candidateCaptain}
                      </span>
                    </div>
                  )}
                  {(seatDiff.bridgeAdded.length > 0 ||
                    seatDiff.bridgeRemoved.length > 0) && (
                    <div>
                      Bridge:{" "}
                      {seatDiff.bridgeAdded.length > 0 && (
                        <span style={{ color: "#43b581" }}>
                          +{seatDiff.bridgeAdded.join(", ")}
                        </span>
                      )}
                      {seatDiff.bridgeAdded.length > 0 &&
                        seatDiff.bridgeRemoved.length > 0 &&
                        " / "}
                      {seatDiff.bridgeRemoved.length > 0 && (
                        <span style={{ color: "#f04747" }}>
                          -{seatDiff.bridgeRemoved.join(", ")}
                        </span>
                      )}
                    </div>
                  )}
                  {(seatDiff.belowAdded.length > 0 ||
                    seatDiff.belowRemoved.length > 0) && (
                    <div>
                      Below:{" "}
                      {seatDiff.belowAdded.length > 0 && (
                        <span style={{ color: "#43b581" }}>
                          +{seatDiff.belowAdded.join(", ")}
                        </span>
                      )}
                      {seatDiff.belowAdded.length > 0 &&
                        seatDiff.belowRemoved.length > 0 &&
                        " / "}
                      {seatDiff.belowRemoved.length > 0 && (
                        <span style={{ color: "#f04747" }}>
                          -{seatDiff.belowRemoved.join(", ")}
                        </span>
                      )}
                    </div>
                  )}
                </div>
              )}

              {/* Provenance, QD cell, fidelity, evidence */}
              <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
                {cell && (
                  <span style={tagStyle} title={`Archive cell: ${cell.raw}`}>
                    {cell.captainGroupLabel} · {cell.speedLabel} ·{" "}
                    {cell.survivalLabel}
                  </span>
                )}
                {lanes.map((lane) => (
                  <span key={lane} style={tagStyle}>
                    {formatDiscoveryLaneLabel(lane)}
                  </span>
                ))}
                {tags.map((tag) => (
                  <span key={tag} style={tagStyle}>
                    {formatParetoTag(tag)}
                  </span>
                ))}
              </div>

              <div
                style={{
                  color: "var(--text-muted)",
                  fontSize: "0.73rem",
                  fontVariantNumeric: "tabular-nums",
                }}
              >
                {r.fidelity ? (
                  <span>
                    Fidelity {r.fidelity.grade} (
                    {Math.round(r.fidelity.coverage_pct)}% ·{" "}
                    {r.fidelity.implemented}/{r.fidelity.total_combat_effects}{" "}
                    implemented)
                  </span>
                ) : null}
                {r.fidelity && r.trials_run > 0 ? " · " : ""}
                {r.trials_run > 0 && (
                  <span>
                    {r.trials_run.toLocaleString()} trials
                    {r.evidence_level
                      ? ` (${r.evidence_level.replace(/_/g, " ")})`
                      : ""}
                  </span>
                )}
              </div>

              {r.uncertainty_reason && (
                <div
                  style={{
                    fontSize: "0.72rem",
                    color: "#e8952e",
                    lineHeight: 1.35,
                  }}
                >
                  Caveat: {r.uncertainty_reason}
                </div>
              )}
            </div>
          );
        })}
      </section>

      {canRunDistributions && (
        <div style={{ marginTop: 12 }}>
          <button
            type="button"
            onClick={onRunCompareDistributions}
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
            Re-runs Monte Carlo for selected crews only. Bars: rounds to win
            (1–20, last bucket merges tail), hull remaining on clean wins (10
            bins), plus traced proc sample (~60 trials/crew).
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
              <div style={sectionLabelStyle}>
                Proc-like events (mean count per traced trial)
              </div>
              <div style={{ display: "flex", flexWrap: "wrap", gap: 12 }}>
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
  );
}
