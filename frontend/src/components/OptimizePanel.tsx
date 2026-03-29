import type { CSSProperties } from "react";
import { formatOptimizePhaseLabel } from "../lib/api";

interface OptimizePanelProps {
  collapsed: boolean;
  onToggleCollapsed: () => void;
  crew: import("../lib/types").CrewState;
  loadingOptimize: boolean;
  optimizeCrewsDone: number | null;
  optimizeTotalCrews: number | null;
  optimizePhase?: string | null;
  optimizeEtaSeconds?: number | null;
  optimizeThroughput?: number | null;
  maxCandidates: number | null;
  onMaxCandidatesChange: (value: number | null) => void;
  prioritizeBelowDecksAbility: boolean;
  onPrioritizeBelowDecksAbilityChange: (value: boolean) => void;
  availableSeeds: string[];
  selectedSeeds: string[];
  onSelectedSeedsChange: (seeds: string[]) => void;
  heuristicsOnly: boolean;
  onHeuristicsOnlyChange: (value: boolean) => void;
  belowDecksStrategy: "ordered" | "exploration";
  onBelowDecksStrategyChange: (value: "ordered" | "exploration") => void;
  optimizerStrategy: import("../lib/api").OptimizerStrategyType;
  onOptimizerStrategyChange: (
    value: import("../lib/api").OptimizerStrategyType,
  ) => void;
  optimizeMustInclude: string;
  onOptimizeMustIncludeChange: (value: string) => void;
  optimizeExclude: string;
  onOptimizeExcludeChange: (value: string) => void;
  optimizeCaptainMust: string;
  onOptimizeCaptainMustChange: (value: string) => void;
  optimizeBridgeMust: string;
  onOptimizeBridgeMustChange: (value: string) => void;
  optimizeBelowMust: string;
  onOptimizeBelowMustChange: (value: string) => void;
  optimizeGroupsJson: string;
  onOptimizeGroupsJsonChange: (value: string) => void;
}

const selectStyle: CSSProperties = {
  display: "block",
  marginTop: 4,
  width: "100%",
  padding: "0.4rem",
  background: "var(--bg)",
  border: "1px solid var(--border)",
  borderRadius: 4,
  color: "var(--text)",
};

const checkboxLabelStyle: CSSProperties = {
  fontSize: "0.85rem",
  display: "flex",
  alignItems: "center",
  gap: "0.5rem",
  cursor: "pointer",
};

export default function OptimizePanel({
  collapsed,
  onToggleCollapsed,
  loadingOptimize,
  optimizeCrewsDone,
  optimizeTotalCrews,
  optimizePhase = null,
  optimizeEtaSeconds = null,
  optimizeThroughput = null,
  maxCandidates,
  onMaxCandidatesChange,
  prioritizeBelowDecksAbility,
  onPrioritizeBelowDecksAbilityChange,
  availableSeeds,
  selectedSeeds,
  onSelectedSeedsChange,
  heuristicsOnly,
  onHeuristicsOnlyChange,
  belowDecksStrategy,
  onBelowDecksStrategyChange,
  optimizerStrategy,
  onOptimizerStrategyChange,
  optimizeMustInclude,
  onOptimizeMustIncludeChange,
  optimizeExclude,
  onOptimizeExcludeChange,
  optimizeCaptainMust,
  onOptimizeCaptainMustChange,
  optimizeBridgeMust,
  onOptimizeBridgeMustChange,
  optimizeBelowMust,
  onOptimizeBelowMustChange,
  optimizeGroupsJson,
  onOptimizeGroupsJsonChange,
}: OptimizePanelProps) {
  if (collapsed) {
    return (
      <aside
        style={{
          width: 48,
          background: "var(--surface)",
          borderLeft: "1px solid var(--border)",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          padding: "0.5rem",
        }}
      >
        <button
          type="button"
          onClick={onToggleCollapsed}
          aria-label="Expand panel"
          style={{
            padding: 4,
            background: "transparent",
            border: "none",
            color: "var(--text-muted)",
          }}
        >
          →
        </button>
        <span
          style={{ fontSize: 10, color: "var(--text-muted)", marginTop: 8 }}
        >
          Strategy
        </span>
        <span style={{ fontSize: 10, color: "var(--text-muted)" }}>—</span>
      </aside>
    );
  }

  function toggleSeed(seed: string) {
    if (selectedSeeds.includes(seed)) {
      onSelectedSeedsChange(selectedSeeds.filter((s) => s !== seed));
    } else {
      onSelectedSeedsChange([...selectedSeeds, seed]);
    }
  }

  return (
    <aside
      style={{
        width: 280,
        minWidth: 240,
        background: "var(--surface)",
        borderLeft: "1px solid var(--border)",
        padding: "1rem",
        display: "flex",
        flexDirection: "column",
        gap: "0.75rem",
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
        }}
      >
        <h2 style={{ margin: 0, fontSize: "1rem" }}>Strategy</h2>
        <button
          type="button"
          onClick={onToggleCollapsed}
          aria-label="Collapse panel"
          style={{
            padding: 4,
            background: "transparent",
            border: "none",
            color: "var(--text-muted)",
          }}
        >
          ←
        </button>
      </div>

      {/* ── Heuristics seeds ─────────────────────────────────────── */}
      {availableSeeds.length > 0 && (
        <div>
          <div
            style={{ fontSize: "0.85rem", fontWeight: 600, marginBottom: 4 }}
          >
            Heuristics seeds
          </div>
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: "0.3rem",
              maxHeight: 120,
              overflowY: "auto",
              border: "1px solid var(--border)",
              borderRadius: 4,
              padding: "0.4rem",
            }}
          >
            {availableSeeds.map((seed) => (
              <label key={seed} style={checkboxLabelStyle}>
                <input
                  type="checkbox"
                  checked={selectedSeeds.includes(seed)}
                  onChange={() => toggleSeed(seed)}
                  style={{ margin: 0 }}
                />
                <span style={{ fontSize: "0.8rem" }}>{seed}</span>
              </label>
            ))}
          </div>
        </div>
      )}

      {/* ── Below-decks strategy (shown when seeds selected) ─────── */}
      {selectedSeeds.length > 0 && (
        <>
          <label style={{ fontSize: "0.85rem" }}>
            Below-decks strategy
            <select
              value={belowDecksStrategy}
              onChange={(e) =>
                onBelowDecksStrategyChange(
                  e.target.value as "ordered" | "exploration",
                )
              }
              style={selectStyle}
            >
              <option value="ordered">
                Ordered — take first N from seed list
              </option>
              <option value="exploration">
                Exploration — try all combinations
              </option>
            </select>
          </label>

          <label style={checkboxLabelStyle}>
            <input
              type="checkbox"
              checked={heuristicsOnly}
              onChange={(e) => onHeuristicsOnlyChange(e.target.checked)}
              style={{ margin: 0 }}
            />
            <span>Heuristics only (skip broader search)</span>
          </label>
        </>
      )}

      {/* ── Optimizer strategy ───────────────────────────────────── */}
      <label style={{ fontSize: "0.85rem" }}>
        Optimizer strategy
        <select
          value={optimizerStrategy}
          onChange={(e) =>
            onOptimizerStrategyChange(
              e.target.value as import("../lib/api").OptimizerStrategyType,
            )
          }
          style={selectStyle}
        >
          <option value="exhaustive">Exhaustive</option>
          <option value="genetic">Genetic</option>
          <option value="tiered">Tiered (scout → confirm)</option>
        </select>
      </label>

      <p style={{ margin: 0, fontSize: "0.8rem", color: "var(--text-muted)" }}>
        Ranking uses server defaults: 80% win rate + 20% avg hull remaining (see
        optimizer ranking).
      </p>

      <div style={{ fontSize: "0.85rem", fontWeight: 600, marginTop: 4 }}>
        Search constraints
      </div>
      <p style={{ margin: 0, fontSize: "0.75rem", color: "var(--text-muted)" }}>
        Comma-separated names. Optional JSON array for groups, e.g.{" "}
        <code style={{ fontSize: "0.7rem" }}>
          [{`{"officers":["A","B"],"min_count":2}`}]
        </code>
      </p>
      <label style={{ fontSize: "0.8rem" }}>
        Must include (any seat)
        <input
          type="text"
          value={optimizeMustInclude}
          onChange={(e) => onOptimizeMustIncludeChange(e.target.value)}
          placeholder="Officer A, Officer B"
          style={{
            display: "block",
            marginTop: 4,
            width: "100%",
            padding: "0.35rem",
            background: "var(--bg)",
            border: "1px solid var(--border)",
            borderRadius: 4,
            color: "var(--text)",
          }}
        />
      </label>
      <label style={{ fontSize: "0.8rem" }}>
        Exclude
        <input
          type="text"
          value={optimizeExclude}
          onChange={(e) => onOptimizeExcludeChange(e.target.value)}
          style={{
            display: "block",
            marginTop: 4,
            width: "100%",
            padding: "0.35rem",
            background: "var(--bg)",
            border: "1px solid var(--border)",
            borderRadius: 4,
            color: "var(--text)",
          }}
        />
      </label>
      <label style={{ fontSize: "0.8rem" }}>
        Captain must be
        <input
          type="text"
          value={optimizeCaptainMust}
          onChange={(e) => onOptimizeCaptainMustChange(e.target.value)}
          style={{
            display: "block",
            marginTop: 4,
            width: "100%",
            padding: "0.35rem",
            background: "var(--bg)",
            border: "1px solid var(--border)",
            borderRadius: 4,
            color: "var(--text)",
          }}
        />
      </label>
      <label style={{ fontSize: "0.8rem" }}>
        Bridge must include
        <input
          type="text"
          value={optimizeBridgeMust}
          onChange={(e) => onOptimizeBridgeMustChange(e.target.value)}
          style={{
            display: "block",
            marginTop: 4,
            width: "100%",
            padding: "0.35rem",
            background: "var(--bg)",
            border: "1px solid var(--border)",
            borderRadius: 4,
            color: "var(--text)",
          }}
        />
      </label>
      <label style={{ fontSize: "0.8rem" }}>
        Below-decks must include
        <input
          type="text"
          value={optimizeBelowMust}
          onChange={(e) => onOptimizeBelowMustChange(e.target.value)}
          style={{
            display: "block",
            marginTop: 4,
            width: "100%",
            padding: "0.35rem",
            background: "var(--bg)",
            border: "1px solid var(--border)",
            borderRadius: 4,
            color: "var(--text)",
          }}
        />
      </label>
      <label style={{ fontSize: "0.8rem" }}>
        Groups (JSON)
        <textarea
          value={optimizeGroupsJson}
          onChange={(e) => onOptimizeGroupsJsonChange(e.target.value)}
          rows={2}
          placeholder='[{"officers":["A","B"],"min_count":2}]'
          style={{
            display: "block",
            marginTop: 4,
            width: "100%",
            padding: "0.35rem",
            background: "var(--bg)",
            border: "1px solid var(--border)",
            borderRadius: 4,
            color: "var(--text)",
            fontFamily: "monospace",
            fontSize: "0.75rem",
            resize: "vertical",
          }}
        />
      </label>

      <label style={{ fontSize: "0.85rem" }}>
        Max crews (optional)
        <input
          type="number"
          min={1}
          max={2_000_000}
          step={1}
          placeholder="No limit"
          value={maxCandidates ?? ""}
          onChange={(e) => {
            const raw = e.target.value.trim();
            if (raw === "") {
              onMaxCandidatesChange(null);
              return;
            }
            const n = parseInt(raw, 10);
            if (!Number.isNaN(n) && n >= 1) {
              onMaxCandidatesChange(Math.min(n, 2_000_000));
            }
          }}
          style={{
            display: "block",
            marginTop: 4,
            width: "100%",
            padding: "0.4rem",
            background: "var(--bg)",
            border: "1px solid var(--border)",
            borderRadius: 4,
            color: "var(--text)",
          }}
        />
      </label>

      <label style={checkboxLabelStyle}>
        <input
          type="checkbox"
          checked={prioritizeBelowDecksAbility}
          onChange={(e) =>
            onPrioritizeBelowDecksAbilityChange(e.target.checked)
          }
          style={{ margin: 0 }}
        />
        <span>Only below-decks officers with ability</span>
      </label>

      <p style={{ margin: 0, fontSize: "0.8rem", color: "var(--text-muted)" }}>
        {loadingOptimize &&
        optimizeCrewsDone != null &&
        optimizeTotalCrews != null &&
        optimizeTotalCrews > 0
          ? `Live: ${optimizePhase === "genetic" ? "gen" : "units"} ${optimizeCrewsDone} / ${optimizeTotalCrews}${
              formatOptimizePhaseLabel(optimizePhase)
                ? ` · ${formatOptimizePhaseLabel(optimizePhase)}`
                : ""
            }${
              optimizeThroughput != null && optimizePhase !== "genetic"
                ? ` · ~${optimizeThroughput.toFixed(1)}/s`
                : ""
            }${optimizeEtaSeconds != null ? ` · ETA ~${optimizeEtaSeconds}s` : ""}`
          : "Live status: — (run optimize to see phase, ETA, preview)"}
      </p>
    </aside>
  );
}
