import { type CSSProperties, useState } from "react";
import {
  EMPTY_RECOMMENDATION_FILTERS,
  hasActiveFilters,
  type PortfolioAnalyticsSummary,
  type RecommendationFilters,
} from "../../lib/portfolioAnalytics";

interface PortfolioAnalyticsBarProps {
  summary: PortfolioAnalyticsSummary;
  filteredCount: number;
  filters: RecommendationFilters;
  onChangeFilters: (next: RecommendationFilters) => void;
  onSelectDiverseTop: (indices: number[]) => void;
  linearEvalMode?: boolean;
}

const pillBase: CSSProperties = {
  padding: "2px 8px",
  fontSize: "0.75rem",
  borderRadius: 999,
  border: "1px solid var(--border)",
  background: "var(--bg)",
  color: "var(--text)",
  cursor: "pointer",
  display: "inline-flex",
  alignItems: "center",
  gap: 4,
  lineHeight: 1.5,
};

function pillStyle(active: boolean): CSSProperties {
  if (!active) return pillBase;
  return {
    ...pillBase,
    borderColor: "var(--accent)",
    background: "rgba(232,149,46,0.16)",
    fontWeight: 600,
  };
}

function gradeColor(grade: "A" | "B" | "C" | "D"): string {
  switch (grade) {
    case "A":
      return "#43b581";
    case "B":
      return "#63b3ed";
    case "C":
      return "#e8952e";
    case "D":
      return "#f04747";
  }
}

export default function PortfolioAnalyticsBar({
  summary,
  filteredCount,
  filters,
  onChangeFilters,
  onSelectDiverseTop,
  linearEvalMode = false,
}: PortfolioAnalyticsBarProps) {
  const [archiveOpen, setArchiveOpen] = useState(false);

  if (!summary.hasAnalytics) return null;

  const active = hasActiveFilters(filters);
  const canCompareDiverse =
    !linearEvalMode && summary.topDiverseIndices.length >= 2;

  return (
    <section
      aria-label="Portfolio discovery analytics"
      style={{
        marginBottom: "0.75rem",
        padding: "0.65rem 0.75rem",
        background: "var(--bg)",
        border: "1px solid var(--border)",
        borderRadius: 6,
        display: "flex",
        flexDirection: "column",
        gap: "0.5rem",
      }}
    >
      {/* Top summary row: archetypes, fidelity mix, archive toggle, diverse compare */}
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "0.5rem 0.75rem",
          fontSize: "0.8rem",
        }}
      >
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            alignItems: "center",
            gap: "0.4rem 0.85rem",
          }}
        >
          <strong style={{ fontSize: "0.82rem" }}>Portfolio insights</strong>
          {summary.archiveCells.length > 0 && (
            <span style={{ color: "var(--text-muted)" }}>
              {summary.distinctCaptainGroups} archetype
              {summary.distinctCaptainGroups === 1 ? "" : "s"} ·{" "}
              {summary.archiveCells.length} archive cell
              {summary.archiveCells.length === 1 ? "" : "s"}
            </span>
          )}
          {summary.hasFidelity && (
            <span
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
                color: "var(--text-muted)",
              }}
            >
              <span>Fidelity:</span>
              {(["A", "B", "C", "D"] as const).map((grade) => {
                const count = summary.fidelityCounts[grade];
                if (count === 0) return null;
                const isSelected = filters.fidelityGrade === grade;
                return (
                  <button
                    key={grade}
                    type="button"
                    aria-pressed={isSelected}
                    onClick={() =>
                      onChangeFilters({
                        ...filters,
                        fidelityGrade: isSelected ? null : grade,
                      })
                    }
                    style={{
                      ...pillStyle(isSelected),
                      padding: "0px 6px",
                      color: gradeColor(grade),
                      fontWeight: 600,
                    }}
                    title={`Filter to Grade ${grade} modeling fidelity rows`}
                  >
                    {grade}: {count}
                  </button>
                );
              })}
            </span>
          )}
        </div>

        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            alignItems: "center",
            gap: "0.4rem",
          }}
        >
          {summary.archiveCells.length > 0 && (
            <button
              type="button"
              aria-expanded={archiveOpen}
              onClick={() => setArchiveOpen((v) => !v)}
              style={pillStyle(archiveOpen || filters.qdCell !== null)}
            >
              {archiveOpen
                ? `Hide archive (${summary.archiveCells.length})`
                : `Explore archive (${summary.archiveCells.length} cells)`}
            </button>
          )}
          {canCompareDiverse && (
            <button
              type="button"
              onClick={() => onSelectDiverseTop(summary.topDiverseIndices)}
              style={{
                ...pillBase,
                borderColor: "var(--accent)",
                color: "var(--accent)",
                fontWeight: 600,
              }}
              title="Select the top crew from each distinct captain archetype (up to 5) for side-by-side comparison"
            >
              Compare top diverse ({summary.topDiverseIndices.length})
            </button>
          )}
        </div>
      </div>

      {/* Interactive lane & view filter chips */}
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          alignItems: "center",
          gap: "0.35rem",
        }}
      >
        <span
          style={{
            fontSize: "0.73rem",
            color: "var(--text-muted)",
            marginRight: 2,
          }}
        >
          Filter:
        </span>
        {summary.laneCounts.length > 1 &&
          summary.laneCounts.map((item) => {
            const isSelected = filters.lane === item.lane;
            return (
              <button
                key={item.lane}
                type="button"
                aria-pressed={isSelected}
                onClick={() =>
                  onChangeFilters({
                    ...filters,
                    lane: isSelected ? null : item.lane,
                  })
                }
                style={pillStyle(isSelected)}
              >
                {item.label} ({item.count})
              </button>
            );
          })}
        {summary.paretoRowCount > 0 && (
          <button
            type="button"
            aria-pressed={filters.paretoOnly}
            onClick={() =>
              onChangeFilters({
                ...filters,
                paretoOnly: !filters.paretoOnly,
              })
            }
            style={pillStyle(filters.paretoOnly)}
          >
            Pareto frontier ({summary.paretoRowCount})
          </button>
        )}
        {summary.caveatRowCount > 0 && (
          <button
            type="button"
            aria-pressed={filters.caveatsOnly}
            onClick={() =>
              onChangeFilters({
                ...filters,
                caveatsOnly: !filters.caveatsOnly,
              })
            }
            style={pillStyle(filters.caveatsOnly)}
          >
            With caveats ({summary.caveatRowCount})
          </button>
        )}
        {active && (
          <button
            type="button"
            onClick={() => onChangeFilters(EMPTY_RECOMMENDATION_FILTERS)}
            style={{
              ...pillBase,
              color: "var(--text-muted)",
            }}
          >
            Clear filters ({filteredCount} of {summary.totalRows})
          </button>
        )}
      </div>

      {/* Collapsible MAP-Elites Diversity Archive grid */}
      {archiveOpen && summary.archiveCells.length > 0 && (
        <fieldset
          aria-label="Diversity archive cells"
          style={{
            margin: 0,
            padding: "0.35rem 0 0",
            border: "none",
            borderTop: "1px solid var(--border)",
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(210px, 1fr))",
            gap: "0.45rem",
          }}
        >
          {summary.archiveCells.map((entry) => {
            const isSelected = filters.qdCell === entry.cell.raw;
            const rec = entry.bestCrew;
            return (
              <button
                key={entry.cell.raw}
                type="button"
                aria-pressed={isSelected}
                onClick={() =>
                  onChangeFilters({
                    ...filters,
                    qdCell: isSelected ? null : entry.cell.raw,
                  })
                }
                style={{
                  textAlign: "left",
                  padding: "0.45rem 0.55rem",
                  borderRadius: 6,
                  border: `1px solid ${isSelected ? "var(--accent)" : "var(--border)"}`,
                  background: isSelected
                    ? "rgba(232,149,46,0.14)"
                    : "var(--surface)",
                  color: "var(--text)",
                  cursor: "pointer",
                  display: "flex",
                  flexDirection: "column",
                  gap: 3,
                  fontSize: "0.76rem",
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
                  <strong style={{ fontSize: "0.78rem" }}>
                    {entry.cell.captainGroupLabel}
                  </strong>
                  <span
                    style={{ color: "var(--text-muted)", fontSize: "0.7rem" }}
                  >
                    #{entry.bestIndex + 1}
                    {entry.count > 1 ? ` · ${entry.count} crews` : ""}
                  </span>
                </div>
                <div
                  style={{ color: "var(--text-muted)", fontSize: "0.71rem" }}
                >
                  {entry.cell.speedLabel} · {entry.cell.survivalLabel}
                </div>
                <div
                  style={{
                    whiteSpace: "nowrap",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                  }}
                >
                  {rec.captain}
                </div>
                <div
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    color: "var(--text-muted)",
                    fontSize: "0.71rem",
                    fontVariantNumeric: "tabular-nums",
                  }}
                >
                  <span>
                    {(rec.win_rate * 100).toFixed(1)}% win ·{" "}
                    {(rec.avg_hull_remaining * 100).toFixed(1)}% hull
                  </span>
                  {rec.fidelity && (
                    <span
                      style={{
                        color: gradeColor(
                          rec.fidelity.grade as "A" | "B" | "C" | "D",
                        ),
                        fontWeight: 600,
                      }}
                    >
                      {rec.fidelity.grade}
                    </span>
                  )}
                </div>
              </button>
            );
          })}
        </fieldset>
      )}
    </section>
  );
}
