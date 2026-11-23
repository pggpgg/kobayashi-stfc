import {
  type CSSProperties,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import HostilePicker from "../components/HostilePicker";
import OfficerNameMultiSelect from "../components/OfficerNameMultiSelect";
import { useProfile } from "../contexts/ProfileContext";
import { useWorkspaceMode } from "../contexts/WorkspaceModeContext";
import {
  allocateFleet,
  type FleetAllocateResponse,
  type FleetSlotAssignment,
  fetchHostiles,
  fetchOfficers,
  fetchShips,
  formatApiError,
  type HostileListItem,
  type OfficerListItem,
  type ShipListItem,
} from "../lib/api";

interface DockDraft {
  id: string;
  label: string;
  ship: string;
  hostile: string;
  priorityWeight: number;
  chainEnabled: boolean;
  chainKills: number;
  chainSecondary: "min_hull_damage" | "max_expected_loot";
}

const MAX_DOCKS = 5;
const MIN_DOCKS = 2;

const cardStyle: CSSProperties = {
  background: "var(--surface)",
  border: "1px solid var(--border)",
  borderRadius: 6,
  padding: "0.9rem 1rem",
};

const controlStyle: CSSProperties = {
  padding: "0.35rem 0.55rem",
  background: "var(--bg)",
  border: "1px solid var(--border)",
  borderRadius: 4,
  color: "var(--text)",
  fontSize: "0.84rem",
};

const chipStyle: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 4,
  padding: "2px 7px",
  borderRadius: 4,
  border: "1px solid var(--border)",
  fontSize: "0.73rem",
  color: "var(--text-muted)",
  background: "var(--bg)",
};

function formatPct(value: number): string {
  return `${(value * 100).toFixed(1)}%`;
}

function gradePalette(grade: string): { border: string; color: string } {
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

function solverModeLabel(mode: string): string {
  if (mode === "exact_branch_and_bound") return "Exact Branch & Bound";
  if (mode === "branch_and_bound_bounded") return "Bounded Branch & Bound";
  if (mode === "sequential_contention_fallback")
    return "Priority Contention Repair";
  return mode;
}

function makeInitialDocks(
  ships: ShipListItem[],
  hostiles: HostileListItem[],
): DockDraft[] {
  const ship0 =
    ships.find((s) => s.id === "uss_saladin")?.id ?? ships[0]?.id ?? "";
  const ship1 =
    ships.find((s) => s.id === "enterprise_nx_01")?.id ?? ships[1]?.id ?? ship0;
  const hostile0 =
    hostiles.find((h) => h.id === "1140710508")?.id ?? hostiles[0]?.id ?? "";
  const hostile1 =
    hostiles.find((h) => h.id === "1120713907")?.id ??
    hostiles[1]?.id ??
    hostile0;
  return [
    {
      id: "dock-1",
      label: "Dock 1",
      ship: ship0,
      hostile: hostile0,
      priorityWeight: 1.0,
      chainEnabled: false,
      chainKills: 5,
      chainSecondary: "min_hull_damage",
    },
    {
      id: "dock-2",
      label: "Dock 2",
      ship: ship1,
      hostile: hostile1,
      priorityWeight: 1.0,
      chainEnabled: false,
      chainKills: 5,
      chainSecondary: "min_hull_damage",
    },
  ];
}

export default function FleetWorkspace() {
  const { activeProfileId } = useProfile();
  const { ownedOnly } = useWorkspaceMode();

  const [ships, setShips] = useState<ShipListItem[]>([]);
  const [hostiles, setHostiles] = useState<HostileListItem[]>([]);
  const [officers, setOfficers] = useState<OfficerListItem[]>([]);
  const [docks, setDocks] = useState<DockDraft[]>([]);
  const [objective, setObjective] = useState<"total_score" | "min_regret">(
    "total_score",
  );
  const [simsPerCrew, setSimsPerCrew] = useState(250);
  const [candidatesPerSlot, setCandidatesPerSlot] = useState(64);
  const [unavailableOfficers, setUnavailableOfficers] = useState<string[]>([]);

  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<FleetAllocateResponse | null>(null);
  const [expandedStandalone, setExpandedStandalone] = useState<
    Record<number, boolean>
  >({});

  const abortRef = useRef<AbortController | null>(null);
  const nextDockIdRef = useRef(3);

  useEffect(() => {
    return () => {
      abortRef.current?.abort();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    Promise.all([
      fetchShips(ownedOnly, activeProfileId),
      fetchHostiles(),
      fetchOfficers(ownedOnly, activeProfileId),
    ])
      .then(([shipList, hostileList, officerList]) => {
        if (cancelled) return;
        setShips(shipList);
        setHostiles(hostileList);
        setOfficers(officerList);
        setDocks((prev) =>
          prev.length >= MIN_DOCKS
            ? prev
            : makeInitialDocks(shipList, hostileList),
        );
      })
      .catch((e) => {
        if (!cancelled) setError(formatApiError(e));
      });
    return () => {
      cancelled = true;
    };
  }, [activeProfileId, ownedOnly]);

  const shipNameById = useMemo(
    () => new Map(ships.map((s) => [s.id, s.ship_name])),
    [ships],
  );
  const hostileNameById = useMemo(
    () =>
      new Map(
        hostiles.map((h) => [h.id, `${h.hostile_name} (Lvl ${h.level})`]),
      ),
    [hostiles],
  );

  const updateDock = useCallback((id: string, patch: Partial<DockDraft>) => {
    setDocks((prev) => prev.map((d) => (d.id === id ? { ...d, ...patch } : d)));
  }, []);

  const addDock = useCallback(() => {
    setDocks((prev) => {
      if (prev.length >= MAX_DOCKS) return prev;
      const n = nextDockIdRef.current++;
      const fallbackShip =
        ships[prev.length % Math.max(1, ships.length)]?.id ?? "";
      const fallbackHostile =
        hostiles[prev.length % Math.max(1, hostiles.length)]?.id ?? "";
      return [
        ...prev,
        {
          id: `dock-${n}`,
          label: `Dock ${prev.length + 1}`,
          ship: fallbackShip,
          hostile: fallbackHostile,
          priorityWeight: 1.0,
          chainEnabled: false,
          chainKills: 5,
          chainSecondary: "min_hull_damage",
        },
      ];
    });
  }, [hostiles, ships]);

  const removeDock = useCallback((id: string) => {
    setDocks((prev) =>
      prev.length <= MIN_DOCKS ? prev : prev.filter((d) => d.id !== id),
    );
  }, []);

  const canRun =
    docks.length >= MIN_DOCKS &&
    docks.every((d) => Boolean(d.ship) && Boolean(d.hostile)) &&
    !loading;

  const handleAllocateFleet = useCallback(async () => {
    if (!canRun) return;
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    setLoading(true);
    setError(null);
    try {
      const response = await allocateFleet(
        {
          objective,
          sims_per_crew: simsPerCrew,
          candidates_per_slot: candidatesPerSlot,
          seed: 42,
          unavailable_officers: unavailableOfficers,
          slots: docks.map((d) => ({
            label: d.label.trim() || undefined,
            ship: d.ship,
            hostile: d.hostile,
            priority_weight: d.priorityWeight,
            ...(d.chainEnabled
              ? {
                  chain_grind: {
                    enabled: true,
                    kills_target: d.chainKills,
                    secondary: d.chainSecondary,
                  },
                }
              : {}),
          })),
        },
        activeProfileId,
        controller.signal,
      );
      if (controller.signal.aborted) return;
      setResult(response);
      setExpandedStandalone({});
    } catch (e) {
      if (controller.signal.aborted) return;
      setError(formatApiError(e));
    } finally {
      if (!controller.signal.aborted) setLoading(false);
    }
  }, [
    activeProfileId,
    canRun,
    candidatesPerSlot,
    docks,
    objective,
    simsPerCrew,
    unavailableOfficers,
  ]);

  return (
    <div
      style={{
        maxWidth: 1280,
        width: "100%",
        margin: "0 auto",
        padding: "1rem 1.25rem 2rem",
        display: "flex",
        flexDirection: "column",
        gap: "1rem",
      }}
    >
      <header
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "flex-end",
          gap: 12,
          flexWrap: "wrap",
        }}
      >
        <div>
          <h1 style={{ margin: 0, fontSize: "1.3rem" }}>
            Multi-Ship Fleet Allocator
          </h1>
          <p
            style={{
              margin: "0.25rem 0 0",
              fontSize: "0.86rem",
              color: "var(--text-muted)",
            }}
          >
            Jointly optimize 2–5 simultaneous drydocks with zero duplicate
            officers across ships. Resolves contested staples globally via exact
            Branch-and-Bound over diverse Portfolio candidate banks.
          </p>
        </div>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <button
            type="button"
            onClick={addDock}
            disabled={docks.length >= MAX_DOCKS || loading}
            style={{
              ...controlStyle,
              cursor:
                docks.length >= MAX_DOCKS || loading
                  ? "not-allowed"
                  : "pointer",
              opacity: docks.length >= MAX_DOCKS || loading ? 0.55 : 1,
            }}
          >
            + Add Dock ({docks.length}/{MAX_DOCKS})
          </button>
          <button
            type="button"
            onClick={() => void handleAllocateFleet()}
            disabled={!canRun}
            style={{
              ...controlStyle,
              background: "var(--accent)",
              color: "var(--bg)",
              borderColor: "var(--accent)",
              fontWeight: 600,
              cursor: canRun ? "pointer" : "not-allowed",
              opacity: canRun ? 1 : 0.6,
            }}
          >
            {loading ? "Allocating Fleet…" : "Optimize Fleet"}
          </button>
        </div>
      </header>

      {/* Global settings */}
      <section style={cardStyle} aria-label="Fleet optimization settings">
        <div
          style={{
            display: "grid",
            gridTemplateColumns:
              "repeat(auto-fit, minmax(190px, 1fr)) minmax(260px, 1.5fr)",
            gap: 12,
            alignItems: "end",
          }}
        >
          <label
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 4,
              fontSize: "0.8rem",
            }}
          >
            <span style={{ color: "var(--text-muted)" }}>Fleet Objective</span>
            <select
              value={objective}
              onChange={(e) =>
                setObjective(e.target.value as "total_score" | "min_regret")
              }
              style={controlStyle}
              aria-label="Fleet Objective"
            >
              <option value="total_score">Maximize Total Weighted Score</option>
              <option value="min_regret">
                Protect Weakest Dock (Min-Regret)
              </option>
            </select>
          </label>

          <label
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 4,
              fontSize: "0.8rem",
            }}
          >
            <span style={{ color: "var(--text-muted)" }}>Sims / Crew</span>
            <select
              value={simsPerCrew}
              onChange={(e) => setSimsPerCrew(Number(e.target.value))}
              style={controlStyle}
              aria-label="Sims per crew"
            >
              <option value={150}>150 (Fast)</option>
              <option value={250}>250 (Balanced)</option>
              <option value={500}>500 (Deep)</option>
            </select>
          </label>

          <label
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 4,
              fontSize: "0.8rem",
            }}
          >
            <span style={{ color: "var(--text-muted)" }}>
              Portfolio Budget / Dock
            </span>
            <select
              value={candidatesPerSlot}
              onChange={(e) => setCandidatesPerSlot(Number(e.target.value))}
              style={controlStyle}
              aria-label="Portfolio budget per dock"
            >
              <option value={48}>48 crews / dock</option>
              <option value={64}>64 crews / dock</option>
              <option value={96}>96 crews / dock</option>
            </select>
          </label>

          <div>
            <OfficerNameMultiSelect
              label="Exclude Busy / Locked Officers (All Docks)"
              officers={officers}
              valueNames={unavailableOfficers}
              onChangeNames={setUnavailableOfficers}
              placeholder="e.g. miners or away-team officers…"
            />
          </div>
        </div>
      </section>

      {/* Drydock mission configurator */}
      <section
        aria-label="Drydock missions"
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(340px, 1fr))",
          gap: 12,
        }}
      >
        {docks.map((dock, idx) => (
          <div key={dock.id} style={cardStyle}>
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                gap: 8,
                marginBottom: 10,
              }}
            >
              <input
                type="text"
                value={dock.label}
                onChange={(e) => updateDock(dock.id, { label: e.target.value })}
                aria-label={`Dock ${idx + 1} label`}
                style={{
                  ...controlStyle,
                  fontWeight: 600,
                  flex: 1,
                }}
              />
              <label
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 4,
                  fontSize: "0.76rem",
                  color: "var(--text-muted)",
                }}
              >
                Weight
                <select
                  value={dock.priorityWeight}
                  onChange={(e) =>
                    updateDock(dock.id, {
                      priorityWeight: Number(e.target.value),
                    })
                  }
                  aria-label={`Dock ${idx + 1} priority weight`}
                  style={{ ...controlStyle, padding: "0.2rem 0.35rem" }}
                >
                  <option value={0.5}>0.5×</option>
                  <option value={1}>1.0×</option>
                  <option value={1.5}>1.5×</option>
                  <option value={2}>2.0×</option>
                </select>
              </label>
              {docks.length > MIN_DOCKS && (
                <button
                  type="button"
                  onClick={() => removeDock(dock.id)}
                  aria-label={`Remove ${dock.label}`}
                  style={{
                    ...controlStyle,
                    padding: "0.2rem 0.5rem",
                    cursor: "pointer",
                  }}
                >
                  Remove
                </button>
              )}
            </div>

            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              <label
                style={{
                  display: "flex",
                  flexDirection: "column",
                  gap: 3,
                  fontSize: "0.78rem",
                }}
              >
                <span style={{ color: "var(--text-muted)" }}>Ship</span>
                <select
                  value={dock.ship}
                  onChange={(e) =>
                    updateDock(dock.id, { ship: e.target.value })
                  }
                  aria-label={`Dock ${idx + 1} ship`}
                  style={controlStyle}
                >
                  <option value="">Select ship…</option>
                  {ships.map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.ship_name} ({s.ship_class})
                    </option>
                  ))}
                </select>
              </label>

              <div
                style={{
                  display: "flex",
                  flexDirection: "column",
                  gap: 3,
                  fontSize: "0.78rem",
                }}
              >
                <span style={{ color: "var(--text-muted)" }}>
                  Hostile Target
                </span>
                <HostilePicker
                  hostiles={hostiles}
                  value={dock.hostile}
                  onChange={(h) => updateDock(dock.id, { hostile: h })}
                  style={{ ...controlStyle, width: "100%" }}
                />
              </div>

              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  flexWrap: "wrap",
                  fontSize: "0.78rem",
                  paddingTop: 4,
                }}
              >
                <label
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 5,
                    cursor: "pointer",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={dock.chainEnabled}
                    onChange={(e) =>
                      updateDock(dock.id, { chainEnabled: e.target.checked })
                    }
                  />
                  Chain Grind
                </label>
                {dock.chainEnabled && (
                  <>
                    <input
                      type="number"
                      min={2}
                      max={30}
                      value={dock.chainKills}
                      onChange={(e) =>
                        updateDock(dock.id, {
                          chainKills: Math.max(
                            2,
                            Math.min(30, Number(e.target.value) || 5),
                          ),
                        })
                      }
                      aria-label={`Dock ${idx + 1} chain kills`}
                      style={{ ...controlStyle, width: 64 }}
                    />
                    <select
                      value={dock.chainSecondary}
                      onChange={(e) =>
                        updateDock(dock.id, {
                          chainSecondary: e.target.value as
                            | "min_hull_damage"
                            | "max_expected_loot",
                        })
                      }
                      aria-label={`Dock ${idx + 1} chain secondary objective`}
                      style={controlStyle}
                    >
                      <option value="min_hull_damage">Min hull damage</option>
                      <option value="max_expected_loot">
                        Max expected loot
                      </option>
                    </select>
                  </>
                )}
              </div>
            </div>
          </div>
        ))}
      </section>

      {error && (
        <div
          role="alert"
          style={{
            padding: "0.6rem 0.9rem",
            background: "rgba(240,71,71,0.14)",
            border: "1px solid rgba(240,71,71,0.5)",
            borderRadius: 6,
            fontSize: "0.86rem",
          }}
        >
          {error}
        </div>
      )}

      {result && (
        <>
          {/* Fleet KPI summary strip */}
          <section
            aria-label="Fleet allocation summary"
            style={{
              ...cardStyle,
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              gap: 14,
              flexWrap: "wrap",
            }}
          >
            <div style={{ display: "flex", gap: 18, flexWrap: "wrap" }}>
              <div>
                <div
                  style={{ fontSize: "0.72rem", color: "var(--text-muted)" }}
                >
                  FLEET EFFICIENCY
                </div>
                <div
                  style={{
                    fontSize: "1.25rem",
                    fontWeight: 700,
                    color: "#43b581",
                    fontVariantNumeric: "tabular-nums",
                  }}
                >
                  {result.fleet_efficiency_pct.toFixed(1)}%
                </div>
              </div>
              <div>
                <div
                  style={{ fontSize: "0.72rem", color: "var(--text-muted)" }}
                >
                  WEAKEST DOCK RETENTION
                </div>
                <div
                  style={{
                    fontSize: "1.25rem",
                    fontWeight: 700,
                    fontVariantNumeric: "tabular-nums",
                  }}
                >
                  {result.min_dock_retention_pct.toFixed(1)}%
                </div>
              </div>
              <div>
                <div
                  style={{ fontSize: "0.72rem", color: "var(--text-muted)" }}
                >
                  WEIGHTED SCORE / CEILING
                </div>
                <div
                  style={{
                    fontSize: "1.05rem",
                    fontWeight: 600,
                    fontVariantNumeric: "tabular-nums",
                  }}
                >
                  {result.total_fleet_score.toFixed(3)} /{" "}
                  {result.standalone_upper_bound.toFixed(3)}
                </div>
              </div>
            </div>

            <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
              <span
                style={{
                  ...chipStyle,
                  borderColor: result.conflict_free
                    ? "rgba(67,181,129,0.55)"
                    : "rgba(240,71,71,0.55)",
                  color: result.conflict_free ? "#43b581" : "#f04747",
                  fontWeight: 600,
                }}
              >
                {result.conflict_free
                  ? "Zero Officer Conflicts"
                  : "Conflict Warning"}
              </span>
              <span style={chipStyle}>
                {solverModeLabel(result.solver_mode)}
              </span>
              <span style={chipStyle}>
                {result.objective === "min_regret"
                  ? "Min-Regret"
                  : "Max Total Score"}
              </span>
            </div>
          </section>

          {/* Contested officer diagnostics */}
          {result.contested_officers.length > 0 && (
            <section
              aria-label="Contested officers resolution"
              style={cardStyle}
            >
              <div
                style={{
                  fontSize: "0.84rem",
                  fontWeight: 600,
                  marginBottom: 8,
                }}
              >
                Contested Officers Resolved ({result.contested_officers.length})
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                {result.contested_officers.map((c) => (
                  <div
                    key={c.officer}
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                      gap: 10,
                      flexWrap: "wrap",
                      padding: "0.45rem 0.65rem",
                      background: "var(--bg)",
                      border: "1px solid var(--border)",
                      borderRadius: 4,
                      fontSize: "0.8rem",
                    }}
                  >
                    <div>
                      <strong>{c.officer}</strong>{" "}
                      <span style={{ color: "var(--text-muted)" }}>
                        wanted by {c.wanted_by_docks.join(" & ")}
                      </span>
                    </div>
                    <div style={{ color: "var(--text-muted)" }}>{c.reason}</div>
                  </div>
                ))}
              </div>
            </section>
          )}

          {/* Per-dock assignment cards */}
          <section
            aria-label="Fleet dock assignments"
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(360px, 1fr))",
              gap: 12,
            }}
          >
            {result.slots.map((slot: FleetSlotAssignment) => {
              const crew = slot.assigned_crew;
              const standalone = slot.standalone_best;
              const isFullRetention = slot.retention_pct >= 99.9;
              const showStandalone = Boolean(
                expandedStandalone[slot.slot_index],
              );
              return (
                <div key={slot.slot_index} style={cardStyle}>
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "flex-start",
                      gap: 8,
                      marginBottom: 8,
                    }}
                  >
                    <div>
                      <strong style={{ fontSize: "0.95rem" }}>
                        {slot.label}
                      </strong>
                      <div
                        style={{
                          fontSize: "0.78rem",
                          color: "var(--text-muted)",
                        }}
                      >
                        {shipNameById.get(slot.ship) ?? slot.ship} vs{" "}
                        {hostileNameById.get(slot.hostile) ?? slot.hostile} ·{" "}
                        {slot.priority_weight}× weight
                      </div>
                    </div>
                    <span
                      style={{
                        ...chipStyle,
                        borderColor: isFullRetention
                          ? "rgba(67,181,129,0.5)"
                          : "rgba(232,149,46,0.55)",
                        color: isFullRetention ? "#43b581" : "#e8952e",
                        fontWeight: 600,
                        fontVariantNumeric: "tabular-nums",
                      }}
                    >
                      {slot.retention_pct.toFixed(1)}% retained
                    </span>
                  </div>

                  {crew ? (
                    <>
                      <div
                        style={{
                          padding: "0.6rem 0.7rem",
                          background: "var(--bg)",
                          border: "1px solid var(--border)",
                          borderRadius: 5,
                          marginBottom: 8,
                        }}
                      >
                        <div
                          style={{
                            display: "flex",
                            justifyContent: "space-between",
                            alignItems: "center",
                            gap: 8,
                            flexWrap: "wrap",
                            marginBottom: 4,
                          }}
                        >
                          <strong style={{ fontSize: "0.9rem" }}>
                            {crew.captain}
                          </strong>
                          <span
                            style={{
                              ...chipStyle,
                              ...gradePalette(crew.fidelity.grade),
                              fontWeight: 600,
                            }}
                          >
                            {crew.fidelity.grade}{" "}
                            {Math.round(crew.fidelity.coverage_pct)}%
                          </span>
                        </div>
                        <div style={{ fontSize: "0.82rem" }}>
                          <span style={{ color: "var(--text-muted)" }}>
                            Bridge:{" "}
                          </span>
                          {crew.bridge.join(", ")}
                        </div>
                        {crew.below_decks.length > 0 && (
                          <div style={{ fontSize: "0.8rem", marginTop: 2 }}>
                            <span style={{ color: "var(--text-muted)" }}>
                              Below Decks:{" "}
                            </span>
                            {crew.below_decks.join(", ")}
                          </div>
                        )}
                        <div
                          style={{
                            marginTop: 6,
                            fontSize: "0.78rem",
                            color: "var(--text-muted)",
                            fontVariantNumeric: "tabular-nums",
                          }}
                        >
                          Score {formatPct(crew.score)} · Win{" "}
                          {formatPct(crew.win_rate)} · Hull{" "}
                          {formatPct(crew.avg_hull_remaining)} · R1{" "}
                          {formatPct(crew.r1_kill_rate)}
                        </div>
                      </div>

                      {slot.yielded_officers &&
                        slot.yielded_officers.length > 0 && (
                          <div
                            style={{
                              fontSize: "0.76rem",
                              color: "var(--text-muted)",
                              marginBottom: 6,
                            }}
                          >
                            Yielded to other docks:{" "}
                            <strong style={{ color: "var(--warning)" }}>
                              {slot.yielded_officers.join(", ")}
                            </strong>{" "}
                            (-{(slot.score_loss_vs_standalone * 100).toFixed(2)}
                            % score)
                          </div>
                        )}

                      {standalone && !isFullRetention && (
                        <div>
                          <button
                            type="button"
                            onClick={() =>
                              setExpandedStandalone((prev) => ({
                                ...prev,
                                [slot.slot_index]: !prev[slot.slot_index],
                              }))
                            }
                            style={{
                              ...controlStyle,
                              padding: "0.2rem 0.55rem",
                              fontSize: "0.75rem",
                              cursor: "pointer",
                            }}
                          >
                            {showStandalone
                              ? "Hide Standalone Best"
                              : "Compare vs Standalone Best"}
                          </button>
                          {showStandalone && (
                            <div
                              style={{
                                marginTop: 6,
                                padding: "0.45rem 0.6rem",
                                background: "var(--bg)",
                                border: "1px dashed var(--border)",
                                borderRadius: 4,
                                fontSize: "0.77rem",
                                color: "var(--text-muted)",
                              }}
                            >
                              <div>
                                Standalone #1:{" "}
                                <strong style={{ color: "var(--text)" }}>
                                  {standalone.captain}
                                </strong>{" "}
                                · {standalone.bridge.join(", ")}
                                {standalone.below_decks.length > 0 &&
                                  ` | BD: ${standalone.below_decks.join(", ")}`}
                              </div>
                              <div style={{ marginTop: 2 }}>
                                Score {formatPct(standalone.score)} · Win{" "}
                                {formatPct(standalone.win_rate)} · Hull{" "}
                                {formatPct(standalone.avg_hull_remaining)}
                              </div>
                            </div>
                          )}
                        </div>
                      )}
                    </>
                  ) : (
                    <p
                      style={{
                        margin: 0,
                        fontSize: "0.82rem",
                        color: "var(--error)",
                      }}
                    >
                      No conflict-free crew could be assigned from the available
                      roster.
                    </p>
                  )}
                </div>
              );
            })}
          </section>
        </>
      )}
    </div>
  );
}
