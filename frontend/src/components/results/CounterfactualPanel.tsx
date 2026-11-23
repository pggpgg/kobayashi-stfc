import { type CSSProperties, useMemo, useState } from "react";
import type {
  CrewCounterfactualResponse,
  PairedDelta,
  SeatAblation,
  SubstituteCrewPlan,
} from "../../lib/api";

export interface LoadedCrewTarget {
  captain: string;
  bridge: string[];
  below_decks: string[];
}

interface CounterfactualPanelProps {
  data: CrewCounterfactualResponse | null;
  loading: boolean;
  error: string | null;
  unavailableOfficers: string[];
  onToggleUnavailable: (officer: string) => void;
  onClearUnavailable: () => void;
  onRunWithUnavailable: (unavailable: string[]) => void;
  onClose: () => void;
  onLoadCrew?: (crew: LoadedCrewTarget) => void;
}

const cardStyle: CSSProperties = {
  marginTop: "1rem",
  padding: "0.9rem 1rem",
  background: "var(--surface)",
  border: "1px solid var(--border)",
  borderRadius: 6,
};

const chipStyle: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 4,
  padding: "2px 7px",
  borderRadius: 4,
  border: "1px solid var(--border)",
  fontSize: "0.72rem",
  color: "var(--text-muted)",
  background: "var(--bg)",
};

const actionBtnStyle: CSSProperties = {
  padding: "0.28rem 0.65rem",
  fontSize: "0.78rem",
  borderRadius: 4,
  border: "1px solid var(--border)",
  background: "var(--bg)",
  color: "var(--text)",
  cursor: "pointer",
};

const primaryBtnStyle: CSSProperties = {
  ...actionBtnStyle,
  background: "var(--accent)",
  color: "var(--bg)",
  borderColor: "var(--accent)",
  fontWeight: 600,
};

function seatLabel(
  slotGroup: string,
  slotIndex: number | null | undefined,
): string {
  if (slotGroup === "captain") return "Captain";
  if (slotGroup === "bridge") {
    return slotIndex != null ? `Bridge ${slotIndex + 1}` : "Bridge";
  }
  return slotIndex != null ? `Below Deck ${slotIndex + 1}` : "Below Deck";
}

function formatSignedScoreDelta(delta: PairedDelta): string {
  const pts = delta.delta_score * 100;
  const halfCi = Math.max(
    0,
    ((delta.delta_score_ci_high - delta.delta_score_ci_low) * 100) / 2,
  );
  const sign = pts >= 0 ? "+" : "";
  return `${sign}${pts.toFixed(2)}% (±${halfCi.toFixed(2)}%)`;
}

function formatPct(value: number): string {
  return `${(value * 100).toFixed(1)}%`;
}

function applySingleSlotSwap(
  data: CrewCounterfactualResponse,
  ablation: SeatAblation,
  replacement: string,
): LoadedCrewTarget {
  const bridge = [...data.bridge];
  const below_decks = [...data.below_decks];
  let captain = data.captain;
  if (ablation.slot_group === "captain") {
    captain = replacement;
  } else if (
    ablation.slot_group === "bridge" &&
    ablation.slot_index != null &&
    ablation.slot_index < bridge.length
  ) {
    bridge[ablation.slot_index] = replacement;
  } else if (
    ablation.slot_group === "below_decks" &&
    ablation.slot_index != null &&
    ablation.slot_index < below_decks.length
  ) {
    below_decks[ablation.slot_index] = replacement;
  }
  return { captain, bridge, below_decks };
}

function gradeBadgeColor(grade: string): { border: string; color: string } {
  const palette: Record<string, { border: string; color: string }> = {
    A: { border: "rgba(67,181,129,0.5)", color: "#43b581" },
    B: { border: "rgba(99,179,237,0.5)", color: "#63b3ed" },
    C: { border: "rgba(232,149,46,0.55)", color: "#e8952e" },
    D: { border: "rgba(240,71,71,0.55)", color: "#f04747" },
  };
  return (
    palette[grade] ?? {
      border: "var(--border)",
      color: "var(--text-muted)",
    }
  );
}

export default function CounterfactualPanel({
  data,
  loading,
  error,
  unavailableOfficers,
  onToggleUnavailable,
  onClearUnavailable,
  onRunWithUnavailable,
  onClose,
  onLoadCrew,
}: CounterfactualPanelProps) {
  const [extraUnavailableInput, setExtraUnavailableInput] = useState("");

  const maxAbsContribution = useMemo(() => {
    if (!data || data.seat_ablations.length === 0) return 0.1;
    return Math.max(
      0.05,
      ...data.seat_ablations.map((a) => Math.abs(a.marginal_contribution)),
    );
  }, [data]);

  const unavailableSet = useMemo(
    () => new Set(unavailableOfficers.map((n) => n.trim().toLowerCase())),
    [unavailableOfficers],
  );

  const handleAddExtraUnavailable = () => {
    const trimmed = extraUnavailableInput.trim();
    if (!trimmed) return;
    if (!unavailableSet.has(trimmed.toLowerCase())) {
      onToggleUnavailable(trimmed);
    }
    setExtraUnavailableInput("");
  };

  return (
    <section
      aria-label="Counterfactual ablation and substitute crew planner"
      style={cardStyle}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "flex-start",
          gap: 12,
          flexWrap: "wrap",
          borderBottom: "1px solid var(--border)",
          paddingBottom: "0.6rem",
          marginBottom: "0.75rem",
        }}
      >
        <div>
          <strong style={{ fontSize: "0.95rem" }}>
            Counterfactual Ablations &amp; Substitute Planner
          </strong>
          {data && (
            <div
              style={{
                fontSize: "0.8rem",
                color: "var(--text-muted)",
                marginTop: 4,
              }}
            >
              Baseline:{" "}
              <strong style={{ color: "var(--text)" }}>{data.captain}</strong>
              {data.bridge.length > 0 && ` · ${data.bridge.join(", ")}`}
              {data.below_decks.length > 0 &&
                ` | BD: ${data.below_decks.join(", ")}`}
            </div>
          )}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          {data && (
            <span
              style={{
                fontSize: "0.78rem",
                color: "var(--text-muted)",
                fontVariantNumeric: "tabular-nums",
              }}
            >
              Score {formatPct(data.baseline.score)} · Win{" "}
              {formatPct(data.baseline.win_rate)} · Hull{" "}
              {formatPct(data.baseline.avg_hull_remaining)} ·{" "}
              {data.trials_per_variant} CRN trials × {data.variants_evaluated}{" "}
              variants
            </span>
          )}
          <button type="button" onClick={onClose} style={actionBtnStyle}>
            Close
          </button>
        </div>
      </div>

      {loading && (
        <div
          style={{
            padding: "0.75rem 0",
            color: "var(--text-muted)",
            fontSize: "0.86rem",
          }}
        >
          Running paired Common Random Numbers ablations &amp; legal substitute
          search…
        </div>
      )}

      {error && (
        <div
          role="alert"
          style={{
            padding: "0.55rem 0.75rem",
            marginBottom: "0.75rem",
            background: "rgba(240,71,71,0.14)",
            border: "1px solid rgba(240,71,71,0.45)",
            borderRadius: 4,
            fontSize: "0.84rem",
            color: "var(--text)",
          }}
        >
          {error}
        </div>
      )}

      {data && !loading && (
        <>
          {/* Section 1: Per-seat leave-one-out marginal contributions */}
          <div style={{ marginBottom: "1rem" }}>
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                flexWrap: "wrap",
                gap: 8,
                marginBottom: 8,
              }}
            >
              <div style={{ fontSize: "0.82rem", fontWeight: 600 }}>
                1. Seat Marginal Impact (Leave-One-Out CRN)
              </div>
              <span style={{ fontSize: "0.74rem", color: "var(--text-muted)" }}>
                Mark officers you lack or need on another dock to synthesize
                substitutes.
              </span>
            </div>

            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              {data.seat_ablations.map((ab) => {
                const isMarked = unavailableSet.has(
                  ab.officer.trim().toLowerCase(),
                );
                const barWidthPct = Math.min(
                  100,
                  (Math.abs(ab.marginal_contribution) / maxAbsContribution) *
                    100,
                );
                const positive = ab.marginal_contribution >= 0;
                const halfDropCi = Math.max(
                  0,
                  ((ab.drop_delta.delta_score_ci_high -
                    ab.drop_delta.delta_score_ci_low) *
                    100) /
                    2,
                );
                return (
                  <div
                    key={`${ab.slot_group}-${ab.slot_index ?? "cap"}-${ab.officer}`}
                    style={{
                      padding: "0.55rem 0.7rem",
                      border: isMarked
                        ? "1px solid var(--warning)"
                        : "1px solid var(--border)",
                      borderRadius: 5,
                      background: isMarked
                        ? "rgba(232,149,46,0.08)"
                        : "var(--bg)",
                    }}
                  >
                    <div
                      style={{
                        display: "grid",
                        gridTemplateColumns:
                          "minmax(160px, 1.3fr) minmax(140px, 1.2fr) auto",
                        gap: 10,
                        alignItems: "center",
                      }}
                    >
                      <div>
                        <span
                          style={{
                            fontSize: "0.72rem",
                            color: "var(--text-muted)",
                            marginRight: 6,
                            textTransform: "uppercase",
                          }}
                        >
                          {seatLabel(ab.slot_group, ab.slot_index)}
                        </span>
                        <strong style={{ fontSize: "0.88rem" }}>
                          {ab.officer}
                        </strong>
                        {ab.drop_delta.statistically_significant && (
                          <span
                            style={{
                              ...chipStyle,
                              marginLeft: 6,
                              color: positive ? "#43b581" : "#f04747",
                              borderColor: positive
                                ? "rgba(67,181,129,0.45)"
                                : "rgba(240,71,71,0.45)",
                            }}
                            title="95% paired CRN confidence interval excludes zero"
                          >
                            95% sig
                          </span>
                        )}
                      </div>

                      <div>
                        <div
                          style={{
                            display: "flex",
                            justifyContent: "space-between",
                            fontSize: "0.75rem",
                            fontVariantNumeric: "tabular-nums",
                            marginBottom: 3,
                          }}
                        >
                          <span
                            style={{
                              color: positive ? "#43b581" : "#f04747",
                              fontWeight: 600,
                            }}
                          >
                            {positive ? "+" : ""}
                            {(ab.marginal_contribution * 100).toFixed(2)}%
                            impact
                          </span>
                          <span style={{ color: "var(--text-muted)" }}>
                            ±{halfDropCi.toFixed(2)}%
                          </span>
                        </div>
                        <div
                          style={{
                            height: 6,
                            borderRadius: 3,
                            background: "var(--surface)",
                            overflow: "hidden",
                          }}
                        >
                          <div
                            style={{
                              height: "100%",
                              width: `${Math.max(4, barWidthPct)}%`,
                              background: positive ? "#43b581" : "#f04747",
                              borderRadius: 3,
                            }}
                          />
                        </div>
                      </div>

                      <label
                        style={{
                          display: "inline-flex",
                          alignItems: "center",
                          gap: 5,
                          fontSize: "0.76rem",
                          color: isMarked
                            ? "var(--warning)"
                            : "var(--text-muted)",
                          cursor: "pointer",
                          userSelect: "none",
                        }}
                      >
                        <input
                          type="checkbox"
                          checked={isMarked}
                          onChange={() => onToggleUnavailable(ab.officer)}
                          aria-label={`Mark ${ab.officer} unavailable`}
                        />
                        Unavailable
                      </label>
                    </div>

                    {ab.trigger_dependencies &&
                      ab.trigger_dependencies.length > 0 && (
                        <div
                          style={{
                            display: "flex",
                            flexWrap: "wrap",
                            gap: 5,
                            marginTop: 6,
                          }}
                        >
                          {ab.trigger_dependencies.map((dep) => (
                            <span
                              key={dep}
                              style={{
                                ...chipStyle,
                                color: "var(--accent)",
                                borderColor: "rgba(232,149,46,0.35)",
                              }}
                            >
                              {dep}
                            </span>
                          ))}
                        </div>
                      )}

                    {ab.best_replacements.length > 0 && (
                      <div
                        style={{
                          display: "flex",
                          flexWrap: "wrap",
                          alignItems: "center",
                          gap: 6,
                          marginTop: 7,
                          fontSize: "0.75rem",
                        }}
                      >
                        <span style={{ color: "var(--text-muted)" }}>
                          Drop-in swaps:
                        </span>
                        {ab.best_replacements.map((rep) => {
                          const positiveSwap = rep.delta.delta_score >= 0;
                          return (
                            <span
                              key={rep.officer}
                              style={{
                                ...chipStyle,
                                color: "var(--text)",
                                padding: "2px 6px",
                              }}
                            >
                              <strong>{rep.officer}</strong>
                              <span
                                style={{
                                  color: positiveSwap ? "#43b581" : "#e8952e",
                                  fontVariantNumeric: "tabular-nums",
                                }}
                              >
                                {formatSignedScoreDelta(rep.delta)}
                              </span>
                              {rep.synergy_match && (
                                <span
                                  style={{
                                    color: "var(--accent)",
                                    fontSize: "0.68rem",
                                  }}
                                >
                                  synergy
                                </span>
                              )}
                              {rep.preserves_triggers &&
                                rep.preserves_triggers.length > 0 && (
                                  <span
                                    style={{
                                      color: "#63b3ed",
                                      fontSize: "0.68rem",
                                    }}
                                  >
                                    {rep.preserves_triggers.join("+")}
                                  </span>
                                )}
                              {onLoadCrew && (
                                <button
                                  type="button"
                                  onClick={() =>
                                    onLoadCrew(
                                      applySingleSlotSwap(
                                        data,
                                        ab,
                                        rep.officer,
                                      ),
                                    )
                                  }
                                  style={{
                                    marginLeft: 2,
                                    padding: "0 5px",
                                    fontSize: "0.68rem",
                                    background: "var(--surface)",
                                    border: "1px solid var(--border)",
                                    borderRadius: 3,
                                    color: "var(--text)",
                                    cursor: "pointer",
                                  }}
                                >
                                  Apply
                                </button>
                              )}
                            </span>
                          );
                        })}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>

          {/* Section 2: Substitute Crew Planner */}
          <div
            style={{
              borderTop: "1px solid var(--border)",
              paddingTop: "0.75rem",
            }}
          >
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                flexWrap: "wrap",
                gap: 8,
                marginBottom: 8,
              }}
            >
              <div style={{ fontSize: "0.82rem", fontWeight: 600 }}>
                2. Substitute Crew Planner
                {data.unavailable_officers.length > 0
                  ? ` (Excluding: ${data.unavailable_officers.join(", ")})`
                  : " (Best Legal Alternatives)"}
              </div>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  flexWrap: "wrap",
                }}
              >
                <input
                  type="text"
                  value={extraUnavailableInput}
                  onChange={(e) => setExtraUnavailableInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      handleAddExtraUnavailable();
                    }
                  }}
                  placeholder="Also exclude officer…"
                  aria-label="Also exclude officer"
                  style={{
                    padding: "0.25rem 0.5rem",
                    fontSize: "0.78rem",
                    background: "var(--bg)",
                    border: "1px solid var(--border)",
                    borderRadius: 4,
                    color: "var(--text)",
                    width: 165,
                  }}
                />
                <button
                  type="button"
                  onClick={handleAddExtraUnavailable}
                  style={actionBtnStyle}
                >
                  Exclude
                </button>
                {unavailableOfficers.length > 0 && (
                  <button
                    type="button"
                    onClick={onClearUnavailable}
                    style={actionBtnStyle}
                  >
                    Clear ({unavailableOfficers.length})
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => onRunWithUnavailable(unavailableOfficers)}
                  style={primaryBtnStyle}
                >
                  Find Substitute Crews
                </button>
              </div>
            </div>

            {unavailableOfficers.length > 0 && (
              <div
                style={{
                  display: "flex",
                  flexWrap: "wrap",
                  gap: 5,
                  marginBottom: 8,
                }}
              >
                {unavailableOfficers.map((name) => (
                  <span
                    key={name}
                    style={{
                      ...chipStyle,
                      borderColor: "var(--warning)",
                      color: "var(--warning)",
                    }}
                  >
                    {name}
                    <button
                      type="button"
                      onClick={() => onToggleUnavailable(name)}
                      aria-label={`Remove ${name} from unavailable`}
                      style={{
                        border: "none",
                        background: "transparent",
                        color: "inherit",
                        cursor: "pointer",
                        padding: 0,
                        fontSize: "0.78rem",
                      }}
                    >
                      ×
                    </button>
                  </span>
                ))}
              </div>
            )}

            {data.substitutes.length === 0 ? (
              <p
                style={{
                  margin: 0,
                  fontSize: "0.82rem",
                  color: "var(--text-muted)",
                }}
              >
                No legal substitute crew found from the remaining roster under
                these exclusions.
              </p>
            ) : (
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                {data.substitutes.map((sub: SubstituteCrewPlan, idx) => {
                  const gradeColors = gradeBadgeColor(
                    sub.metrics.fidelity.grade,
                  );
                  const positiveSub = sub.delta.delta_score >= 0;
                  return (
                    <div
                      key={`${sub.captain}-${sub.bridge.join(",")}-${sub.below_decks.join(",")}-${idx}`}
                      style={{
                        padding: "0.55rem 0.7rem",
                        border: "1px solid var(--border)",
                        borderRadius: 5,
                        background: "var(--bg)",
                        display: "flex",
                        justifyContent: "space-between",
                        alignItems: "center",
                        gap: 10,
                        flexWrap: "wrap",
                      }}
                    >
                      <div style={{ minWidth: 240, flex: 1 }}>
                        <div
                          style={{
                            display: "flex",
                            alignItems: "center",
                            gap: 6,
                            flexWrap: "wrap",
                          }}
                        >
                          <span
                            style={{
                              fontSize: "0.74rem",
                              color: "var(--text-muted)",
                              fontWeight: 600,
                            }}
                          >
                            #{idx + 1}
                          </span>
                          <strong style={{ fontSize: "0.86rem" }}>
                            {sub.captain}
                          </strong>
                          <span
                            style={{
                              fontSize: "0.8rem",
                              color: "var(--text-muted)",
                            }}
                          >
                            · {sub.bridge.join(", ")}
                            {sub.below_decks.length > 0 &&
                              ` | BD: ${sub.below_decks.join(", ")}`}
                          </span>
                          <span
                            style={{
                              ...chipStyle,
                              borderColor: gradeColors.border,
                              color: gradeColors.color,
                              fontWeight: 600,
                            }}
                          >
                            {sub.metrics.fidelity.grade}{" "}
                            {Math.round(sub.metrics.fidelity.coverage_pct)}%
                          </span>
                        </div>
                        <div
                          style={{
                            fontSize: "0.76rem",
                            color: "var(--text-muted)",
                            marginTop: 4,
                          }}
                        >
                          {sub.summary}
                        </div>
                        {sub.changed_slots.length > 0 && (
                          <div
                            style={{
                              display: "flex",
                              flexWrap: "wrap",
                              gap: 5,
                              marginTop: 4,
                            }}
                          >
                            {sub.changed_slots.map((ch) => (
                              <span
                                key={`${ch.slot_group}-${ch.slot_index ?? "c"}-${ch.to}`}
                                style={chipStyle}
                              >
                                {seatLabel(ch.slot_group, ch.slot_index)}:{" "}
                                {ch.from} →{" "}
                                <strong style={{ color: "var(--text)" }}>
                                  {ch.to}
                                </strong>
                              </span>
                            ))}
                          </div>
                        )}
                      </div>

                      <div
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: 10,
                          fontVariantNumeric: "tabular-nums",
                        }}
                      >
                        <div
                          style={{ textAlign: "right", fontSize: "0.78rem" }}
                        >
                          <div
                            style={{
                              fontWeight: 600,
                              color: positiveSub ? "#43b581" : "var(--text)",
                            }}
                          >
                            {sub.retention_pct.toFixed(1)}% retained
                          </div>
                          <div
                            style={{
                              fontSize: "0.73rem",
                              color: positiveSub ? "#43b581" : "#e8952e",
                            }}
                          >
                            {formatSignedScoreDelta(sub.delta)}
                          </div>
                          <div
                            style={{
                              fontSize: "0.72rem",
                              color: "var(--text-muted)",
                            }}
                          >
                            Win {formatPct(sub.metrics.win_rate)} · Hull{" "}
                            {formatPct(sub.metrics.avg_hull_remaining)}
                          </div>
                        </div>
                        {onLoadCrew && (
                          <button
                            type="button"
                            onClick={() =>
                              onLoadCrew({
                                captain: sub.captain,
                                bridge: sub.bridge,
                                below_decks: sub.below_decks,
                              })
                            }
                            style={actionBtnStyle}
                          >
                            Load Crew
                          </button>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </>
      )}
    </section>
  );
}
