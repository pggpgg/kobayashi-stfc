import type { CSSProperties, ReactNode } from "react";
import { formatOptimizePhaseLabel, type OfficerListItem } from "../lib/api";
import OfficerNameMultiSelect from "./OfficerNameMultiSelect";

/** Match server `MAX_TIERED_SCOUT_SIMS` / `MAX_TIERED_TOP_K` (see `src/server/api/requests.rs`). */
const MAX_TIERED_SCOUT_SIMS_UI = 100_000;
const MAX_TIERED_TOP_K_UI = 500;
/** Match server novelty caps (`MAX_NOVELTY_*` in `src/server/api/requests.rs`). */
const MAX_NOVELTY_DIVERSE_TOP_UI = 500;
const MAX_NOVELTY_POOL_UI = 10_000;

interface OptimizePanelProps {
  collapsed: boolean;
  onToggleCollapsed: () => void;
  /** Officer catalog for searchable constraint multiselects (same source as crew builder). */
  officerOptions: OfficerListItem[];
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
  fastDiscovery: boolean;
  onFastDiscoveryChange: (value: boolean) => void;
  belowDecksStrategy: "ordered" | "exploration";
  onBelowDecksStrategyChange: (value: "ordered" | "exploration") => void;
  optimizerStrategy: import("../lib/api").OptimizerStrategyType;
  onOptimizerStrategyChange: (
    value: import("../lib/api").OptimizerStrategyType,
  ) => void;
  /** Tiered only: scout sims per crew; null = server default (500). */
  tieredScoutSims: number | null;
  onTieredScoutSimsChange: (value: number | null) => void;
  /** Tiered only: top K for confirmation; null = server default (20). */
  tieredTopK: number | null;
  onTieredTopKChange: (value: number | null) => void;
  /** Blank = off. When set to a number in (0, 1], server reorders the recommendation head for diverse officer material (MMR + Jaccard). */
  noveltyLambdaText: string;
  onNoveltyLambdaTextChange: (value: string) => void;
  noveltyDiverseTopText: string;
  onNoveltyDiverseTopTextChange: (value: string) => void;
  noveltyPoolText: string;
  onNoveltyPoolTextChange: (value: string) => void;
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
  chainGrindEnabled: boolean;
  onChainGrindEnabledChange: (value: boolean) => void;
  chainKillsTarget: number;
  onChainKillsTargetChange: (value: number) => void;
  chainSecondary: "min_hull_damage" | "max_loot_per_hull_proxy";
  onChainSecondaryChange: (
    value: "min_hull_damage" | "max_loot_per_hull_proxy",
  ) => void;
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

const labelWithHintRowStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 4,
  marginBottom: 4,
};

function HelpHint({ text }: { text: string }) {
  return (
    <span
      title={text}
      role="note"
      aria-label={text}
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        width: 14,
        height: 14,
        fontSize: 10,
        fontWeight: 700,
        lineHeight: 1,
        borderRadius: "50%",
        border: "1px solid var(--border)",
        color: "var(--text-muted)",
        cursor: "help",
        flexShrink: 0,
      }}
    >
      ?
    </span>
  );
}

function FieldLabelWithHint({
  children,
  hint,
}: {
  children: ReactNode;
  hint: string;
}) {
  return (
    <div style={labelWithHintRowStyle}>
      {children}
      <HelpHint text={hint} />
    </div>
  );
}

export default function OptimizePanel({
  collapsed,
  onToggleCollapsed,
  officerOptions,
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
  fastDiscovery,
  onFastDiscoveryChange,
  belowDecksStrategy,
  onBelowDecksStrategyChange,
  optimizerStrategy,
  onOptimizerStrategyChange,
  tieredScoutSims,
  onTieredScoutSimsChange,
  tieredTopK,
  onTieredTopKChange,
  noveltyLambdaText,
  onNoveltyLambdaTextChange,
  noveltyDiverseTopText,
  onNoveltyDiverseTopTextChange,
  noveltyPoolText,
  onNoveltyPoolTextChange,
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
  chainGrindEnabled,
  onChainGrindEnabledChange,
  chainKillsTarget,
  onChainKillsTargetChange,
  chainSecondary,
  onChainSecondaryChange,
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
      <div>
        <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
          <div style={{ fontSize: "0.85rem", fontWeight: 600 }}>
            Heuristics seeds
          </div>
          <HelpHint
            text={
              "Named starter crew lists the optimizer evaluates before the broader search, so you can steer results toward lineups you already trust. Add seed files under data/heuristics in this repository (the server exposes discovered seed names here)."
            }
          />
        </div>
        {availableSeeds.length > 0 ? (
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
              marginTop: 4,
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
        ) : (
          <p
            style={{
              margin: "4px 0 0",
              fontSize: "0.75rem",
              color: "var(--text-muted)",
            }}
          >
            No seeds were returned. Add lists under{" "}
            <code style={{ fontSize: "0.7rem" }}>data/heuristics</code> in the
            project tree.
          </p>
        )}
      </div>

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

          <label
            style={{
              ...checkboxLabelStyle,
              opacity: optimizerStrategy === "genetic" ? 0.45 : 1,
            }}
          >
            <input
              type="checkbox"
              checked={fastDiscovery}
              disabled={optimizerStrategy === "genetic"}
              onChange={(e) => onFastDiscoveryChange(e.target.checked)}
              aria-label="Fast discovery: merge seed crews into main optimize pipeline"
              style={{ margin: 0 }}
            />
            <span style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
              Fast discovery (merge seeds into main pipeline)
              <HelpHint
                text={
                  optimizerStrategy === "genetic"
                    ? "Not available with genetic strategy (use seeds without this option for seeded GA)."
                    : "Seed crews are prepended to the optimizer warm-start list so they go through the same approximate analytical rank and Monte Carlo path as generated crews, instead of a separate full-sim pass on every seed combination first."
                }
              />
            </span>
          </label>
        </>
      )}

      {/* ── Optimizer strategy ───────────────────────────────────── */}
      <div style={{ fontSize: "0.85rem" }}>
        <FieldLabelWithHint
          hint={
            "Exhaustive: generate candidate crews from officer pools (respecting Max crews when set), then run your full Sims per crew on each. Genetic: evolve populations of crews for very large spaces; ignores the Max crews cap. Tiered: a fast scouting pass with fewer sims per crew ranks candidates, then your full sim count runs only on the best Top K crews."
          }
        >
          <span>Optimizer strategy</span>
        </FieldLabelWithHint>
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
      </div>

      {optimizerStrategy === "tiered" && (
        <>
          <div style={{ fontSize: "0.85rem" }}>
            <FieldLabelWithHint
              hint={
                "Number of Monte Carlo trials per crew during the tiered scouting phase (always fewer than your main Sims per crew). Lower is faster but ranks candidates more noisily. Leave blank for the server default (typically 500)."
              }
            >
              <span>Tiered scout sims / crew (optional)</span>
            </FieldLabelWithHint>
            <input
              type="number"
              min={1}
              max={MAX_TIERED_SCOUT_SIMS_UI}
              step={1}
              placeholder="500 (default)"
              value={tieredScoutSims ?? ""}
              onChange={(e) => {
                const raw = e.target.value.trim();
                if (raw === "") {
                  onTieredScoutSimsChange(null);
                  return;
                }
                const n = parseInt(raw, 10);
                if (!Number.isNaN(n) && n >= 1) {
                  onTieredScoutSimsChange(
                    Math.min(n, MAX_TIERED_SCOUT_SIMS_UI),
                  );
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
          </div>
          <div style={{ fontSize: "0.85rem" }}>
            <FieldLabelWithHint
              hint={
                "How many top crews from the scout pass are promoted to the confirmation phase, where each receives your full Sims per crew. Leave blank for the server default (typically 20)."
              }
            >
              <span>Tiered top K (optional)</span>
            </FieldLabelWithHint>
            <input
              type="number"
              min={1}
              max={MAX_TIERED_TOP_K_UI}
              step={1}
              placeholder="20 (default)"
              value={tieredTopK ?? ""}
              onChange={(e) => {
                const raw = e.target.value.trim();
                if (raw === "") {
                  onTieredTopKChange(null);
                  return;
                }
                const n = parseInt(raw, 10);
                if (!Number.isNaN(n) && n >= 1) {
                  onTieredTopKChange(Math.min(n, MAX_TIERED_TOP_K_UI));
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
          </div>
          <p
            style={{
              margin: "4px 0 0",
              fontSize: "0.72rem",
              color: "var(--text-muted)",
            }}
          >
            Leave blank for server defaults (500 scout sims, top 20). Max{" "}
            {MAX_TIERED_SCOUT_SIMS_UI.toLocaleString()} / {MAX_TIERED_TOP_K_UI}.
          </p>
        </>
      )}

      <div>
        <label style={checkboxLabelStyle}>
          <input
            type="checkbox"
            checked={chainGrindEnabled}
            onChange={(e) => onChainGrindEnabledChange(e.target.checked)}
            style={{ margin: 0 }}
          />
          <span>
            Chain grind (N wins, hull carries, shields full each fight)
          </span>
        </label>
        {chainGrindEnabled && (
          <>
            <label
              style={{ fontSize: "0.85rem", display: "block", marginTop: 8 }}
            >
              Kills target (N)
              <input
                type="number"
                min={1}
                max={50}
                value={chainKillsTarget}
                onChange={(e) =>
                  onChainKillsTargetChange(
                    Math.min(50, Math.max(1, Number(e.target.value) || 1)),
                  )
                }
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
            <label
              style={{ fontSize: "0.85rem", display: "block", marginTop: 8 }}
            >
              Secondary (tie-break after chain success rate)
              <select
                value={chainSecondary}
                onChange={(e) =>
                  onChainSecondaryChange(
                    e.target.value as
                      | "min_hull_damage"
                      | "max_loot_per_hull_proxy",
                  )
                }
                style={selectStyle}
              >
                <option value="min_hull_damage">
                  Max hull after N kills (min damage)
                </option>
                <option value="max_loot_per_hull_proxy">
                  Loot / hull proxy (placeholder)
                </option>
              </select>
            </label>
            <p
              style={{
                margin: "6px 0 0",
                fontSize: "0.72rem",
                color: "var(--text-muted)",
              }}
            >
              Same hostile each link. Analytical prefilter is skipped. Results
              table labels update for chain metrics.
            </p>
          </>
        )}
      </div>

      <p style={{ margin: 0, fontSize: "0.8rem", color: "var(--text-muted)" }}>
        {chainGrindEnabled
          ? "Ranking: chain success rate first, then secondary among successful trials."
          : "Ranking uses server defaults: 80% win rate + 20% avg hull remaining (see optimizer ranking)."}
      </p>

      <div style={{ fontSize: "0.85rem", marginTop: 10 }}>
        <FieldLabelWithHint
          hint={
            "Maximal marginal relevance (MMR) on officer sets: λ in (0, 1] blends win rate vs redundancy. Higher λ stays closer to pure strength order. Leave blank to disable. Optional head size and pool limit refine how many leading rows are reordered and how wide the candidate pool is (server defaults apply when omitted)."
          }
        >
          <span>Novelty λ (optional, 0–1)</span>
        </FieldLabelWithHint>
        <input
          type="text"
          inputMode="decimal"
          placeholder="Off — pure strength order"
          value={noveltyLambdaText}
          onChange={(e) => onNoveltyLambdaTextChange(e.target.value)}
          aria-label="Novelty lambda for MMR ranking"
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
        <label
          style={{
            fontSize: "0.85rem",
            display: "block",
            marginTop: 8,
            opacity: noveltyLambdaText.trim() ? 1 : 0.45,
          }}
        >
          Diverse head size (optional)
          <input
            type="number"
            min={1}
            max={MAX_NOVELTY_DIVERSE_TOP_UI}
            step={1}
            placeholder={`Default ${20} when λ set`}
            disabled={!noveltyLambdaText.trim()}
            value={noveltyDiverseTopText}
            onChange={(e) => onNoveltyDiverseTopTextChange(e.target.value)}
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
        <label
          style={{
            fontSize: "0.85rem",
            display: "block",
            marginTop: 8,
            opacity: noveltyLambdaText.trim() ? 1 : 0.45,
          }}
        >
          Novelty pool size (optional)
          <input
            type="number"
            min={2}
            max={MAX_NOVELTY_POOL_UI}
            step={1}
            placeholder="Server default when λ set"
            disabled={!noveltyLambdaText.trim()}
            value={noveltyPoolText}
            onChange={(e) => onNoveltyPoolTextChange(e.target.value)}
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
        <p
          style={{
            margin: "6px 0 0",
            fontSize: "0.72rem",
            color: "var(--text-muted)",
          }}
        >
          Diverse head / pool require a valid λ. Pool must be ≥ head when both
          are set (server validates). Max head {MAX_NOVELTY_DIVERSE_TOP_UI},
          pool {MAX_NOVELTY_POOL_UI.toLocaleString()}.
        </p>
      </div>

      <div style={{ fontSize: "0.85rem", fontWeight: 600, marginTop: 4 }}>
        Search constraints
      </div>
      <p style={{ margin: 0, fontSize: "0.75rem", color: "var(--text-muted)" }}>
        Officer filters use searchable lists (names sent to the server match
        your catalog). Optional JSON array for groups, e.g.{" "}
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
      <OfficerNameMultiSelect
        label={<span>Exclude</span>}
        officers={officerOptions}
        valueComma={optimizeExclude}
        onChangeComma={onOptimizeExcludeChange}
      />
      <OfficerNameMultiSelect
        label={<span>Captain must be</span>}
        officers={officerOptions}
        valueComma={optimizeCaptainMust}
        onChangeComma={onOptimizeCaptainMustChange}
        placeholder="Search; multiple names mean captain can be any of them"
      />
      <OfficerNameMultiSelect
        label={<span>Bridge must include</span>}
        officers={officerOptions}
        valueComma={optimizeBridgeMust}
        onChangeComma={onOptimizeBridgeMustChange}
      />
      <OfficerNameMultiSelect
        label={<span>Below-decks must include</span>}
        officers={officerOptions}
        valueComma={optimizeBelowMust}
        onChangeComma={onOptimizeBelowMustChange}
      />
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
