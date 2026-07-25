import {
  type CSSProperties,
  memo,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useProfile } from "../contexts/ProfileContext";
import { useWorkspaceMode } from "../contexts/WorkspaceModeContext";
import type {
  HostileListItem,
  OptimizeEstimate,
  ShipListItem,
} from "../lib/api";
import {
  type ComponentOverrideDeltas,
  type ComponentOverrides,
  fetchHostiles,
  fetchShips,
  formatApiError,
  formatOptimizePhaseLabel,
  getShipComponentOverrides,
  getShipTiersLevels,
} from "../lib/api";
import type { SupportBuffId } from "../lib/supportBuffs";
import type { CrewState } from "../lib/types";
import { DEFAULT_BELOW_DECK_UNLOCK_LEVELS } from "../lib/types";
import HostilePicker from "./HostilePicker";
import { ScenarioSelect } from "./ScenarioSelect";
import SupportBuffSelect from "./SupportBuffSelect";

const SIMS_PRESETS = [1000, 5000, 10000, 50000] as const;

const selectStyle = {
  padding: "0.4rem 0.6rem",
  background: "var(--bg)",
  border: "1px solid var(--border)",
  borderRadius: 6,
  color: "var(--text)",
} as const;

const COMPONENT_DELTA_FIELDS: {
  key: keyof ComponentOverrideDeltas;
  short: string;
  long: string;
  fmt: "int" | "pct" | "mult";
}[] = [
  { key: "attack", short: "atk", long: "Attack", fmt: "int" },
  { key: "hull_health", short: "hull", long: "Hull health", fmt: "int" },
  { key: "shield_health", short: "shield", long: "Shield health", fmt: "int" },
  { key: "armor", short: "armor", long: "Armor", fmt: "int" },
  {
    key: "shield_deflection",
    short: "deflect",
    long: "Shield deflection",
    fmt: "int",
  },
  { key: "dodge", short: "dodge", long: "Dodge", fmt: "int" },
  {
    key: "armor_piercing",
    short: "a.pierce",
    long: "Armor piercing",
    fmt: "int",
  },
  {
    key: "shield_piercing",
    short: "s.pierce",
    long: "Shield piercing",
    fmt: "int",
  },
  { key: "accuracy", short: "acc", long: "Accuracy", fmt: "int" },
  { key: "crit_chance", short: "crit", long: "Crit chance", fmt: "pct" },
  { key: "crit_damage", short: "critdmg", long: "Crit damage", fmt: "mult" },
];

function formatDeltaValue(value: number, fmt: "int" | "pct" | "mult"): string {
  const sign = value > 0 ? "+" : "";
  if (fmt === "pct") return `${sign}${(value * 100).toFixed(1)}%`;
  if (fmt === "mult") return `${sign}${value.toFixed(2)}`;
  return `${sign}${Math.round(value)}`;
}

/**
 * Parenthetical after the time estimate, e.g. ` (1,240 crews, ×3 chain)`.
 *
 * Each qualifier stands alone: a chain multiplier explains the *time*, so it has to show even when
 * the candidate count is unavailable (0), and an empty list renders nothing rather than `()`.
 */
function formatEstimateQualifiers(estimate: OptimizeEstimate): string {
  const parts: string[] = [];
  if (estimate.estimated_candidates > 0) {
    parts.push(`${estimate.estimated_candidates.toLocaleString()} crews`);
  }
  if (estimate.chain_kills_target != null) {
    parts.push(`×${estimate.chain_kills_target} chain`);
  }
  return parts.length > 0 ? ` (${parts.join(", ")})` : "";
}

/** Non-zero component deltas as "label value" strings (short or long labels). */
function componentDeltaEntries(
  deltas: ComponentOverrideDeltas,
  useLong: boolean,
): string[] {
  return COMPONENT_DELTA_FIELDS.flatMap((f) => {
    const value = deltas[f.key];
    if (!value) return [];
    return [`${useLong ? f.long : f.short} ${formatDeltaValue(value, f.fmt)}`];
  });
}

const overrideChipBase: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 4,
  padding: "0.25rem 0.5rem",
  fontSize: "0.72rem",
  borderRadius: 999,
  cursor: "help",
  whiteSpace: "nowrap",
};

/** Shows whether the active profile's synced components add stats over the ship's base hull tier. */
function ComponentOverridesChip({
  overrides,
}: {
  overrides: ComponentOverrides;
}) {
  if (!overrides.applied || !overrides.deltas) {
    return (
      <span
        title="Your synced components match this hull tier — no stat bonus is applied."
        style={{
          ...overrideChipBase,
          border: "1px solid var(--border)",
          color: "var(--text-muted)",
        }}
      >
        Components match tier
      </span>
    );
  }
  const shortEntries = componentDeltaEntries(overrides.deltas, false);
  const longList = componentDeltaEntries(overrides.deltas, true).join(", ");
  const face = shortEntries.slice(0, 3).join(" · ");
  const more = shortEntries.length - 3;
  return (
    <span
      title={`Synced component upgrades above hull tier are applied to sim/optimize: ${longList}`}
      style={{
        ...overrideChipBase,
        border: "1px solid var(--accent-dim)",
        background: "rgba(232, 149, 46, 0.15)",
        color: "var(--text)",
      }}
    >
      ⬆ {face}
      {more > 0 ? ` · +${more} more` : ""}
    </span>
  );
}

interface WorkspaceHeaderProps {
  shipId: string;
  scenarioId: string;
  onShipIdChange: (id: string) => void;
  onScenarioIdChange: (id: string) => void;
  enemyType: string;
  onEnemyTypeChange: (id: string) => void;
  shipTier: number;
  onShipTierChange: (tier: number) => void;
  shipLevel: number;
  onShipLevelChange: (level: number) => void;
  /** Called when tiers-levels fetch returns per-ship below-decks unlock schedule (`crew_slots`). */
  onBelowDeckUnlockLevelsChange: (unlockLevels: number[]) => void;
  crew: CrewState;
  simsPerCrew: number;
  onSimsPerCrewChange: (n: number) => void;
  estimate: OptimizeEstimate | null;
  lastOptimizeDurationMs: number | null;
  onRunSim: () => void;
  onRunOptimize: () => void;
  onCancelOptimize: () => void;
  onSavePreset: () => void;
  loadingSim: boolean;
  loadingOptimize: boolean;
  optimizeProgress: number | null;
  optimizeCrewsDone: number | null;
  optimizeTotalCrews: number | null;
  optimizePhase?: string | null;
  optimizeEtaSeconds?: number | null;
  /** Live stream vs polling while optimizing. */
  optimizeStreamMode?: "sse" | "reconnecting" | "polling" | null;
  selectedSupportBuffs: SupportBuffId[];
  onSelectedSupportBuffsChange: (ids: SupportBuffId[]) => void;
}

export default memo(function WorkspaceHeader({
  shipId,
  scenarioId,
  onShipIdChange,
  onScenarioIdChange,
  enemyType,
  onEnemyTypeChange,
  shipTier,
  onShipTierChange,
  shipLevel,
  onShipLevelChange,
  onBelowDeckUnlockLevelsChange,
  simsPerCrew,
  onSimsPerCrewChange,
  estimate,
  lastOptimizeDurationMs,
  onRunSim,
  onRunOptimize,
  onCancelOptimize,
  onSavePreset,
  loadingSim,
  loadingOptimize,
  optimizeProgress,
  optimizeCrewsDone,
  optimizeTotalCrews,
  optimizePhase = null,
  optimizeEtaSeconds = null,
  optimizeStreamMode = null,
  selectedSupportBuffs,
  onSelectedSupportBuffsChange,
}: WorkspaceHeaderProps) {
  const { activeProfileId } = useProfile();
  const { mode, ownedOnly } = useWorkspaceMode();
  const guided = mode === "guided";
  const [ships, setShips] = useState<ShipListItem[]>([]);
  const [shipsLoadState, setShipsLoadState] = useState<
    "idle" | "loading" | "done" | "error"
  >("idle");
  const [shipsError, setShipsError] = useState<string | null>(null);
  const [hostiles, setHostiles] = useState<HostileListItem[]>([]);
  const [tiers, setTiers] = useState<number[]>([1]);
  const [levels, setLevels] = useState<number[]>([1, 10, 20, 30, 40, 50, 60]);
  const [componentOverrides, setComponentOverrides] =
    useState<ComponentOverrides | null>(null);
  // Async catalog loads must see a preset/loop selection made after their
  // request started; otherwise their initial empty-prop closure picks row zero.
  const latestShipId = useRef(shipId);
  const latestScenarioId = useRef(scenarioId);
  latestShipId.current = shipId;
  latestScenarioId.current = scenarioId;
  const selectedRosterShip = useMemo(
    () => ships.find((s) => s.id === shipId),
    [ships, shipId],
  );
  const rosterLocksShipProgress =
    ownedOnly &&
    selectedRosterShip != null &&
    selectedRosterShip.tier != null &&
    selectedRosterShip.level != null;

  useEffect(() => {
    let c = false;
    setShipsLoadState("loading");
    setShipsError(null);
    fetchShips(ownedOnly, activeProfileId)
      .then((list) => {
        if (!c) {
          setShips(list);
          setShipsLoadState("done");
          if (list.length && !latestShipId.current) {
            const first = list[0];
            if (first) {
              onShipIdChange(first.id);
              if (ownedOnly && first.tier != null && first.level != null) {
                onShipTierChange(first.tier);
                onShipLevelChange(first.level);
              }
            }
          }
        }
      })
      .catch((err) => {
        if (!c) {
          setShipsError(formatApiError(err));
          setShipsLoadState("error");
        }
      });
    return () => {
      c = true;
    };
  }, [ownedOnly, activeProfileId]);

  // Roster mode: once the owned ship list is loaded, align tier/level with the API row for the
  // current selection (fixes stale localStorage and ensures re-fetch after profile change).
  useEffect(() => {
    if (!ownedOnly || shipsLoadState !== "done" || !shipId) return;
    const ship = ships.find((s) => s.id === shipId);
    if (ship?.tier != null && ship?.level != null) {
      onShipTierChange(ship.tier);
      onShipLevelChange(ship.level);
    }
  }, [
    ownedOnly,
    shipsLoadState,
    shipId,
    ships,
    onShipTierChange,
    onShipLevelChange,
  ]);

  // When ship changes: in roster mode pre-fill tier/level from roster
  const handleShipChange = (id: string) => {
    onShipIdChange(id);
    const ship = ships.find((s) => s.id === id);
    if (ownedOnly && ship && ship.tier != null && ship.level != null) {
      onShipTierChange(ship.tier);
      onShipLevelChange(ship.level);
    }
  };

  useEffect(() => {
    if (!shipId) {
      setTiers([1]);
      setLevels([1, 10, 20, 30, 40, 50, 60]);
      onBelowDeckUnlockLevelsChange([...DEFAULT_BELOW_DECK_UNLOCK_LEVELS]);
      return;
    }
    let c = false;
    getShipTiersLevels(shipId)
      .then((data) => {
        if (!c) {
          const t = data.tiers?.length ? data.tiers : [1];
          const l = data.levels?.length
            ? data.levels
            : [1, 10, 20, 30, 40, 50, 60];
          setTiers(t);
          setLevels(l);
          if (!t.includes(shipTier)) onShipTierChange(t[0] ?? 1);
          if (!l.includes(shipLevel)) onShipLevelChange(l[0] ?? 1);
          const rows = data.crew_slots as
            | { unlock_level: number }[]
            | undefined;
          if (rows?.length) {
            const unlockLevels = rows
              .map((r) => r.unlock_level)
              .filter((n) => Number.isFinite(n))
              .sort((a, b) => a - b);
            onBelowDeckUnlockLevelsChange(unlockLevels);
          } else {
            onBelowDeckUnlockLevelsChange([
              ...DEFAULT_BELOW_DECK_UNLOCK_LEVELS,
            ]);
          }
        }
      })
      .catch(() => {
        if (!c) {
          setTiers([1]);
          setLevels([1, 10, 20, 30, 40, 50, 60]);
          onBelowDeckUnlockLevelsChange([...DEFAULT_BELOW_DECK_UNLOCK_LEVELS]);
        }
      });
    return () => {
      c = true;
    };
  }, [
    shipId,
    onBelowDeckUnlockLevelsChange,
    onShipLevelChange,
    onShipTierChange,
  ]);

  useEffect(() => {
    let c = false;
    fetchHostiles().then((list) => {
      if (!c) setHostiles(list);
      if (list.length && !latestScenarioId.current) {
        onScenarioIdChange(list[0]?.id ?? "");
      }
    });
    return () => {
      c = true;
    };
  }, []);

  // Component-upgrade deltas for the chip — only meaningful with an owned roster (roster/guided).
  useEffect(() => {
    if (!ownedOnly || !shipId) {
      setComponentOverrides(null);
      return;
    }
    let c = false;
    getShipComponentOverrides(shipId, shipTier, shipLevel, activeProfileId)
      .then((r) => {
        if (!c) setComponentOverrides(r);
      })
      .catch(() => {
        if (!c) setComponentOverrides(null);
      });
    return () => {
      c = true;
    };
  }, [ownedOnly, shipId, shipTier, shipLevel, activeProfileId]);

  return (
    <header
      id="guided-scenario"
      style={{
        display: "flex",
        alignItems: "center",
        gap: "1rem",
        padding: "0.75rem 1rem",
        background: "var(--surface)",
        borderBottom: "1px solid var(--border)",
        flexWrap: "wrap",
      }}
    >
      <select
        aria-label="Ship"
        value={ships.length > 0 ? shipId : ""}
        onChange={(e) => handleShipChange(e.target.value)}
        style={selectStyle}
        disabled={shipsLoadState === "loading"}
      >
        {shipsLoadState === "loading" && <option>Loading…</option>}
        {shipsLoadState === "done" && ships.length === 0 && (
          <option value="">
            {ownedOnly ? "No ships in roster" : "No ships available"}
          </option>
        )}
        {shipsLoadState === "error" && shipsError && (
          <option value="">{shipsError}</option>
        )}
        {ships.map((s) => (
          <option key={s.id} value={s.id}>
            {s.ship_name}
          </option>
        ))}
      </select>
      <select
        aria-label="Ship tier"
        value={shipTier}
        onChange={(e) => onShipTierChange(Number(e.target.value))}
        style={selectStyle}
        disabled={rosterLocksShipProgress}
      >
        {tiers.map((t) => (
          <option key={t} value={t}>
            T{t}
          </option>
        ))}
      </select>
      <select
        aria-label="Ship level"
        value={shipLevel}
        onChange={(e) => onShipLevelChange(Number(e.target.value))}
        style={selectStyle}
        disabled={rosterLocksShipProgress}
      >
        {levels.map((l) => (
          <option key={l} value={l}>
            Lvl {l}
          </option>
        ))}
      </select>
      {componentOverrides && (
        <ComponentOverridesChip overrides={componentOverrides} />
      )}
      <HostilePicker
        hostiles={hostiles}
        value={scenarioId}
        onChange={onScenarioIdChange}
      />
      <ScenarioSelect value={enemyType} onChange={onEnemyTypeChange} />
      <SupportBuffSelect
        selected={selectedSupportBuffs}
        onChange={onSelectedSupportBuffsChange}
      />
      {rosterLocksShipProgress ? (
        <span style={{ fontSize: "0.72rem", color: "var(--text-muted)" }}>
          {mode === "guided" ? "Guided" : "Roster"} mode locks ship tier/level
          to owned ship data.
        </span>
      ) : null}
      <select
        hidden={guided}
        aria-label="Preset"
        style={{
          padding: "0.4rem 0.6rem",
          background: "var(--bg)",
          border: "1px solid var(--border)",
          borderRadius: 6,
          color: "var(--text)",
        }}
      >
        <option>Preset</option>
      </select>
      <div
        hidden={guided}
        style={{
          display: "flex",
          alignItems: "center",
          gap: "0.5rem",
          flexWrap: "wrap",
        }}
      >
        <label
          style={{
            fontSize: "0.8rem",
            color: "var(--text-muted)",
            display: "flex",
            alignItems: "center",
            gap: 6,
          }}
        >
          Fight iterations/crew
          <input
            type="number"
            min={1}
            max={100000}
            value={simsPerCrew}
            onChange={(e) =>
              onSimsPerCrewChange(
                Math.max(1, Math.min(100000, Number(e.target.value) || 1000)),
              )
            }
            style={{
              width: 72,
              padding: "0.35rem 0.5rem",
              background: "var(--bg)",
              border: "1px solid var(--border)",
              borderRadius: 4,
              color: "var(--text)",
            }}
          />
        </label>
        {SIMS_PRESETS.map((n) => (
          <button
            key={n}
            type="button"
            aria-label={`Set fight iterations to ${n.toLocaleString()}`}
            onClick={() => onSimsPerCrewChange(n)}
            style={{
              padding: "0.35rem 0.5rem",
              background:
                simsPerCrew === n ? "var(--accent)" : "var(--surface)",
              border: "1px solid var(--border)",
              borderRadius: 4,
              color: simsPerCrew === n ? "var(--bg)" : "var(--text)",
              fontSize: "0.8rem",
            }}
          >
            {n >= 1000 ? `${n / 1000}k` : n}
          </button>
        ))}
      </div>
      {estimate != null && (
        <span
          hidden={guided}
          style={{ fontSize: "0.8rem", color: "var(--text-muted)" }}
          title={
            estimate.chain_kills_target != null
              ? `Worst case: charges every trial for the full ${estimate.chain_kills_target}-kill chain. Chains that fail early cost less.`
              : undefined
          }
        >
          Est. ~
          {estimate.estimated_seconds < 1
            ? "<1"
            : estimate.estimated_seconds.toFixed(1)}{" "}
          s{formatEstimateQualifiers(estimate)}
        </span>
      )}
      {lastOptimizeDurationMs != null && (
        <span
          hidden={guided}
          style={{ fontSize: "0.8rem", color: "var(--text-muted)" }}
        >
          Completed in {(lastOptimizeDurationMs / 1000).toFixed(1)} s
        </span>
      )}
      {loadingOptimize && (
        <div
          hidden={guided}
          style={{ display: "flex", alignItems: "center", gap: 8 }}
          role="status"
          aria-live="polite"
        >
          {optimizeProgress != null && (
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                minWidth: 120,
              }}
            >
              <div
                role="progressbar"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={Math.round(optimizeProgress)}
                aria-label="Optimization progress"
                style={{
                  flex: 1,
                  height: 6,
                  background: "var(--border)",
                  borderRadius: 3,
                  overflow: "hidden",
                }}
              >
                <div
                  style={{
                    width: `${optimizeProgress}%`,
                    height: "100%",
                    background: "var(--accent)",
                    borderRadius: 3,
                    transition: "width 0.2s ease",
                  }}
                />
              </div>
              <span
                style={{
                  fontSize: "0.75rem",
                  color: "var(--text-muted)",
                  whiteSpace: "nowrap",
                }}
              >
                {optimizePhase === "genetic" &&
                optimizeTotalCrews != null &&
                optimizeCrewsDone != null &&
                optimizeTotalCrews > 0
                  ? `gen ${optimizeCrewsDone}/${optimizeTotalCrews} (${optimizeProgress}%)`
                  : optimizeTotalCrews != null &&
                      optimizeCrewsDone != null &&
                      optimizeTotalCrews > 0
                    ? `${optimizeCrewsDone}/${optimizeTotalCrews} (${optimizeProgress}%)`
                    : `${optimizeProgress}%`}
              </span>
              {(formatOptimizePhaseLabel(optimizePhase) ||
                optimizeEtaSeconds != null) && (
                <span
                  style={{
                    fontSize: "0.7rem",
                    color: "var(--text-muted)",
                    whiteSpace: "nowrap",
                    maxWidth: 200,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                  }}
                  title={
                    formatOptimizePhaseLabel(optimizePhase) +
                    (optimizeEtaSeconds != null
                      ? ` · ETA ~${optimizeEtaSeconds}s`
                      : "")
                  }
                >
                  {formatOptimizePhaseLabel(optimizePhase)}
                  {optimizeEtaSeconds != null
                    ? ` · ~${optimizeEtaSeconds}s`
                    : ""}
                </span>
              )}
            </div>
          )}
          {optimizeStreamMode === "reconnecting" && (
            <span
              style={{
                fontSize: "0.7rem",
                color: "var(--warning)",
                maxWidth: 200,
              }}
              title="The browser lost the live progress connection; retrying with backoff before falling back to polling."
            >
              Reconnecting to live progress…
            </span>
          )}
          {optimizeStreamMode === "polling" && (
            <span
              style={{
                fontSize: "0.7rem",
                color: "var(--text-muted)",
                maxWidth: 220,
              }}
              title="Updates load over HTTP polling instead of a live stream."
            >
              Polling for progress
            </span>
          )}
          <button
            type="button"
            onClick={onCancelOptimize}
            aria-label="Cancel optimization"
            style={{
              padding: "0.35rem 0.75rem",
              fontSize: "0.85rem",
              background: "var(--surface)",
              border: "1px solid var(--border)",
              borderRadius: 6,
              color: "var(--text)",
            }}
          >
            Cancel run
          </button>
        </div>
      )}
      <div hidden={guided} style={{ flex: 1, minWidth: 8 }} />
      <button
        hidden={guided}
        type="button"
        onClick={onSavePreset}
        style={{
          padding: "0.5rem 1rem",
          background: "var(--surface)",
          border: "1px solid var(--border)",
          borderRadius: 6,
          color: "var(--text)",
        }}
      >
        Save as Preset
      </button>
      <button
        hidden={guided}
        type="button"
        onClick={onRunSim}
        disabled={loadingSim || loadingOptimize}
        aria-busy={loadingSim}
        aria-label={loadingSim ? "Running simulation" : "Run simulation"}
        style={{
          padding: "0.5rem 1rem",
          background: "var(--accent-dim)",
          border: "none",
          borderRadius: 6,
          color: "var(--text)",
        }}
      >
        {loadingSim ? "Running…" : "Run Sim"}
      </button>
      <button
        hidden={guided}
        type="button"
        onClick={onRunOptimize}
        disabled={loadingSim || loadingOptimize}
        aria-busy={loadingOptimize}
        aria-label={
          loadingOptimize
            ? optimizeProgress != null
              ? `Optimizing, ${Math.round(optimizeProgress)} percent complete`
              : "Running optimization"
            : "Run optimization"
        }
        style={{
          padding: "0.5rem 1rem",
          background: "var(--accent)",
          border: "none",
          borderRadius: 6,
          color: "var(--bg)",
        }}
      >
        {loadingOptimize
          ? optimizeProgress != null
            ? `Optimizing… ${optimizeProgress}%`
            : "Running…"
          : "Run Optimize"}
      </button>
    </header>
  );
});
