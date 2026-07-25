import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { RungStatusBadge } from "../components/RungStatusBadge";
import { useProfile } from "../contexts/ProfileContext";
import {
  fetchHostiles,
  fetchShips,
  formatApiError,
  type HostileListItem,
  type ShipListItem,
} from "../lib/api";
import { loopFrontier } from "../lib/loopRungStatus";
import {
  hostileLabel,
  LOOP_CATALOG,
  LOOP_GOALS,
  type LoopGoalId,
  loopGoal,
  resolveLoopHostiles,
  resolveLoopHostilesAscending,
} from "../lib/loopsCatalog";
import {
  getLoopBestRecord,
  type LoopBestRecord,
  type LoopRunContext,
  listLoopRecords,
  savePendingLoopRun,
} from "../lib/loopsWorkspaceStorage";
import { useLoopClimb } from "../lib/useLoopClimb";

const SELECTED_LOOP_KEY = "kobayashi_loops_selected";

function initialLoopId(): string {
  try {
    const stored = localStorage.getItem(SELECTED_LOOP_KEY);
    if (stored && LOOP_CATALOG.some((loop) => loop.id === stored))
      return stored;
  } catch {}
  return LOOP_CATALOG[0].id;
}

function percent(value: number): string {
  if (!Number.isFinite(value)) return "—";
  return `${Math.round(value * 100)}%`;
}

function classLabel(value: string): string {
  return value ? value[0].toLocaleUpperCase() + value.slice(1) : "Unknown";
}

function policyLabel(policy: "required" | "recommended" | "open"): string {
  switch (policy) {
    case "required":
      return "Specialty ship required";
    case "recommended":
      return "Specialty ship encouraged";
    case "open":
      return "Open ship choice";
  }
}

function BestCrewSummary({ record }: { record: LoopBestRecord }) {
  // Under chain grind the engine reuses `avg_hull_remaining` to carry the chain's
  // *secondary* objective (see SimulationResult's note), and for this workspace that
  // secondary is a loot-per-hull proxy — an unbounded number, not a hull fraction.
  // Rendering it as "% hull" produced readings like "24241% hull". Chain records
  // therefore report the chain metrics they actually hold.
  const isChain = record.chainSuccessRate != null;
  return (
    <div className="loop-best">
      <div className="loop-best__heading">Saved best</div>
      <div className="loop-best__crew">
        {record.crew.captain}
        {record.crew.bridge.length > 0
          ? ` · ${record.crew.bridge.join(" · ")}`
          : ""}
      </div>
      <div className="loop-best__metrics">
        {isChain ? (
          <>
            <span
              title={`Chance of completing ${record.chainKillsTarget ?? "N"} consecutive kills`}
            >
              {percent(record.chainSuccessRate ?? 0)} chain
              {record.chainKillsTarget ? ` ×${record.chainKillsTarget}` : ""}
            </span>
            <span>{percent(record.winRate)} wins</span>
            <span>{percent(record.roundOneKillRate)} R1</span>
          </>
        ) : (
          <>
            <span>{percent(record.winRate)} wins</span>
            <span>{percent(record.roundOneKillRate)} R1</span>
            <span title="Mean attacker hull remaining">
              {percent(record.averageHullRemaining)} hull
            </span>
          </>
        )}
      </div>
      <div className="loop-best__ship">
        {record.shipId} · T{record.shipTier} L{record.shipLevel}
      </div>
    </div>
  );
}

function ClimbControls({
  climb,
  onStart,
  nextTargetId,
  totalRungs,
}: {
  climb: ReturnType<typeof useLoopClimb>;
  onStart: () => void;
  nextTargetId: string | null;
  totalRungs: number;
}) {
  const { plan, progress, isClimbing } = climb;
  const done = plan?.results.length ?? 0;
  const current = plan?.rungs[plan.cursor];

  if (isClimbing) {
    return (
      <div className="loop-climb loop-climb--running">
        <span>
          Climbing {current ? `level ${current.targetLevel}` : "…"} · rung{" "}
          {(plan?.cursor ?? 0) + 1} of {plan?.rungs.length ?? totalRungs}
          {progress?.progress ? ` · ${Math.round(progress.progress)}%` : ""}
        </span>
        <button type="button" onClick={climb.cancel}>
          Cancel climb
        </button>
      </div>
    );
  }

  const summary =
    plan && plan.status !== "running" && done > 0
      ? `Last climb: ${plan.results.filter((r) => r.outcome === "cleared").length} cleared of ${done} attempted${
          plan.status === "cancelled" ? " (cancelled)" : ""
        }`
      : null;

  return (
    <div className="loop-climb">
      <button
        type="button"
        onClick={onStart}
        disabled={totalRungs === 0 || !nextTargetId}
        title={
          nextTargetId
            ? "Run each rung in turn from your frontier upward, warm-starting from the rung below"
            : "Every rung on this ladder is already cleared"
        }
      >
        {nextTargetId ? "Climb this loop" : "Ladder cleared"}
      </button>
      {summary && <small>{summary}</small>}
    </div>
  );
}

export default function LoopsWorkspace() {
  const navigate = useNavigate();
  const { activeProfileId } = useProfile();
  const [selectedLoopId, setSelectedLoopId] = useState(initialLoopId);
  const [goalId, setGoalId] = useState<LoopGoalId>("kills_per_hull");
  const [selectedShipId, setSelectedShipId] = useState("");
  const [hostiles, setHostiles] = useState<HostileListItem[]>([]);
  const [ships, setShips] = useState<ShipListItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [recordsRevision, setRecordsRevision] = useState(0);

  const selectedLoop =
    LOOP_CATALOG.find((loop) => loop.id === selectedLoopId) ?? LOOP_CATALOG[0];

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    Promise.all([
      fetchHostiles(),
      fetchShips(false, activeProfileId),
      fetchShips(true, activeProfileId),
    ])
      .then(([hostileList, allShips, ownedShips]) => {
        if (cancelled) return;
        setHostiles(hostileList);
        if (ownedShips.length === 0) {
          setShips(allShips);
          return;
        }
        const available = [...ownedShips];
        const availableIds = new Set(available.map((ship) => ship.id));
        const specialtyIds = new Set(
          LOOP_CATALOG.flatMap((loop) => loop.specialtyShipIds),
        );
        for (const ship of allShips) {
          if (specialtyIds.has(ship.id) && !availableIds.has(ship.id)) {
            available.push(ship);
          }
        }
        setShips(available);
      })
      .catch((cause) => {
        if (!cancelled) setError(formatApiError(cause));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [activeProfileId]);

  useEffect(() => {
    try {
      localStorage.setItem(SELECTED_LOOP_KEY, selectedLoop.id);
    } catch {}
    if (!selectedLoop.goals.includes(goalId)) {
      setGoalId(selectedLoop.goals[0]);
    }
    const specialty = selectedLoop.specialtyShipIds.find((id) =>
      ships.some((ship) => ship.id === id),
    );
    if (selectedLoop.shipPolicy === "required" || !selectedShipId) {
      setSelectedShipId(specialty ?? ships[0]?.id ?? "");
    }
  }, [selectedLoop, ships, goalId, selectedShipId]);

  useEffect(() => {
    const refresh = () => setRecordsRevision((revision) => revision + 1);
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, []);

  const loopHostiles = useMemo(
    () => resolveLoopHostiles(hostiles, selectedLoop),
    [hostiles, selectedLoop],
  );
  const levels = useMemo(() => {
    const grouped = new Map<number, HostileListItem[]>();
    for (const hostile of loopHostiles) {
      const level = grouped.get(hostile.level) ?? [];
      level.push(hostile);
      grouped.set(hostile.level, level);
    }
    return [...grouped.entries()].sort(([a], [b]) => b - a);
  }, [loopHostiles]);
  const allRecords = useMemo(
    () => listLoopRecords(activeProfileId),
    [activeProfileId, recordsRevision],
  );
  /**
   * Rung status for the selected goal, plus which rung to attack next. Computed over
   * the *ascending* ladder because progression runs bottom-up, while `levels` above
   * is sorted for display with the hardest target first.
   */
  const frontier = useMemo(() => {
    const ascending = resolveLoopHostilesAscending(hostiles, selectedLoop);
    const bestByTargetId = new Map(
      ascending.map((hostile) => [
        hostile.id,
        getLoopBestRecord(activeProfileId, selectedLoop.id, hostile.id, goalId),
      ]),
    );
    return loopFrontier(
      ascending.map((hostile) => hostile.id),
      bestByTargetId,
      goalId,
    );
  }, [hostiles, selectedLoop, activeProfileId, goalId, recordsRevision]);
  const loopRecords = allRecords.filter(
    (record) => record.loopId === selectedLoop.id,
  );
  const completedTargets = new Set(
    loopRecords.map((record) => record.targetId),
  );

  const climb = useLoopClimb(activeProfileId);

  // Refresh the ladder as each rung completes so badges track the climb live.
  useEffect(() => {
    if (climb.plan) setRecordsRevision((revision) => revision + 1);
  }, [climb.plan]);

  const selectLoop = (id: string) => {
    if (climb.isClimbing) return;
    setSelectedLoopId(id);
    setSelectedShipId("");
  };

  const startClimb = () => {
    const shipId = selectedShipId;
    if (!shipId) {
      setError("Choose a ship before climbing this loop.");
      return;
    }
    const ascending = resolveLoopHostilesAscending(hostiles, selectedLoop);
    if (ascending.length === 0) return;
    const ship = ships.find((candidate) => candidate.id === shipId);
    climb.start({
      loopId: selectedLoop.id,
      loopName: selectedLoop.name,
      goalId,
      shipId,
      shipPolicy: selectedLoop.shipPolicy,
      specialtyShipIds: [...selectedLoop.specialtyShipIds],
      shipTier: ship?.tier ?? 1,
      shipLevel: ship?.level ?? 1,
      rungs: ascending.map((hostile) => ({
        targetId: hostile.id,
        targetName: hostileLabel(hostile),
        targetLevel: hostile.level,
      })),
      // Resume above the frontier rather than re-proving rungs already cleared.
      startAtIndex: frontier.frontierIndex + 1,
    });
  };

  const openOptimizer = (
    hostile: HostileListItem,
    best: LoopBestRecord | null,
  ) => {
    const shipId = selectedShipId || best?.shipId;
    if (!shipId) {
      setError("Choose a ship before opening this ladder rung.");
      return;
    }
    const context: LoopRunContext = {
      loopId: selectedLoop.id,
      loopName: selectedLoop.name,
      targetId: hostile.id,
      targetName: hostileLabel(hostile),
      targetLevel: hostile.level,
      goalId,
      shipId,
      shipPolicy: selectedLoop.shipPolicy,
      specialtyShipIds: [...selectedLoop.specialtyShipIds],
    };
    savePendingLoopRun(activeProfileId, context);
    navigate("/", {
      state: {
        loopRun: context,
        preset: {
          id: `loop-${selectedLoop.id}-${hostile.id}`,
          name: `${selectedLoop.name} L${hostile.level}`,
          ship: shipId,
          scenario: hostile.id,
          crew: best
            ? {
                captain: best.crew.captain,
                bridge: best.crew.bridge,
                below_deck: best.crew.belowDecks,
              }
            : {},
          schema_version: 2,
          provenance: {
            saved_at: best?.recordedAt ?? new Date().toISOString(),
            kobayashi_version: "loops-workspace",
            source: "loops_workspace",
          },
        },
      },
    });
  };

  const availableGoals = LOOP_GOALS.filter((goal) =>
    selectedLoop.goals.includes(goal.id),
  );
  const selectedGoal = loopGoal(goalId);
  const requiredShip = selectedLoop.specialtyShipIds[0];

  return (
    <div className="loops-page">
      <header className="loops-hero">
        <div>
          <div className="loops-eyebrow">Persistent progression</div>
          <h1>Loops workspace</h1>
          <p>
            Keep the best crew you have found at every rung, then challenge it
            again as your officers, research, buildings, and ships improve.
          </p>
        </div>
        <div className="loops-profile-note">
          <strong>{allRecords.length}</strong>
          <span>saved best results</span>
          <small>Stored on this device for the active profile</small>
        </div>
      </header>

      <div className="loops-layout">
        <aside className="loops-catalog" aria-label="Gameplay loops">
          <div className="loops-catalog__heading">Choose a loop</div>
          {LOOP_CATALOG.map((loop) => {
            const count = allRecords.filter(
              (record) => record.loopId === loop.id,
            ).length;
            return (
              <button
                key={loop.id}
                type="button"
                className={
                  loop.id === selectedLoop.id
                    ? "loop-picker loop-picker--active"
                    : "loop-picker"
                }
                onClick={() => selectLoop(loop.id)}
                disabled={climb.isClimbing}
              >
                <span>{loop.name}</span>
                {count > 0 && <small>{count} saved</small>}
              </button>
            );
          })}
        </aside>

        <section className="loops-main">
          <div className="loop-overview">
            <div className="loop-overview__copy">
              <div className="loop-badges">
                <span>{policyLabel(selectedLoop.shipPolicy)}</span>
                <span>{selectedLoop.engagement.replace("_", " ")}</span>
                {selectedLoop.minOps && <span>Ops {selectedLoop.minOps}+</span>}
              </div>
              <h2>{selectedLoop.name}</h2>
              <p>{selectedLoop.summary}</p>
              <div className="loop-progression">
                <strong>Feeds:</strong> {selectedLoop.progression}
              </div>
              <div className="loop-ship-guidance">
                <strong>Ship rule:</strong> {selectedLoop.specialtyShipLabel}
              </div>
            </div>

            <div className="loop-controls">
              <label>
                Rank crews by
                <select
                  value={goalId}
                  onChange={(event) =>
                    setGoalId(event.target.value as LoopGoalId)
                  }
                >
                  {availableGoals.map((goal) => (
                    <option key={goal.id} value={goal.id}>
                      {goal.label}
                    </option>
                  ))}
                </select>
              </label>
              <small>{selectedGoal.description}</small>
              <label>
                Ship to test
                <select
                  value={selectedShipId}
                  disabled={selectedLoop.shipPolicy === "required"}
                  onChange={(event) => setSelectedShipId(event.target.value)}
                >
                  {requiredShip &&
                    !ships.some((ship) => ship.id === requiredShip) && (
                      <option value={requiredShip}>{requiredShip}</option>
                    )}
                  {ships.map((ship) => (
                    <option key={ship.id} value={ship.id}>
                      {ship.ship_name}
                    </option>
                  ))}
                </select>
              </label>
              {selectedLoop.sourceUrl && (
                <a
                  href={selectedLoop.sourceUrl}
                  target="_blank"
                  rel="noreferrer"
                >
                  Official loop guide ↗
                </a>
              )}
            </div>
          </div>

          {error && (
            <div className="loops-error" role="alert">
              {error}
            </div>
          )}

          <div className="ladder-heading">
            <div>
              <div className="loops-eyebrow">Progression ladder</div>
              <h3>Climb toward stronger targets</h3>
              <ClimbControls
                climb={climb}
                onStart={startClimb}
                nextTargetId={frontier.nextTargetId}
                totalRungs={loopHostiles.length}
              />
            </div>
            <span>
              {frontier.clearedCount} cleared · {completedTargets.size} /{" "}
              {loopHostiles.length} recorded
            </span>
          </div>

          {loading && <p className="loops-muted">Loading hostile catalog…</p>}
          {!loading && levels.length === 0 && (
            <div className="loops-empty">
              <strong>
                This loop is catalogued, but its targets are not in the current
                data set.
              </strong>
              <span>
                It will populate automatically when a matching hostile arrives
                in a data refresh.
              </span>
            </div>
          )}

          {!loading && levels.length > 0 && (
            <div className="loop-ladder">
              <div className="loop-ladder__top">Higher challenge</div>
              {levels.map(([level, targets]) => (
                <section className="ladder-rung" key={level}>
                  <div className="ladder-level">
                    <span>Level</span>
                    <strong>{level}</strong>
                  </div>
                  <div className="ladder-targets">
                    {targets.map((hostile) => {
                      const best = getLoopBestRecord(
                        activeProfileId,
                        selectedLoop.id,
                        hostile.id,
                        goalId,
                      );
                      return (
                        <article className="ladder-target" key={hostile.id}>
                          <div className="ladder-target__header">
                            <div>
                              <strong>{hostileLabel(hostile)}</strong>
                              <span>
                                {classLabel(hostile.ship_class)} · {hostile.id}
                              </span>
                              <div style={{ marginTop: "0.25rem" }}>
                                <RungStatusBadge
                                  info={
                                    frontier.statuses.get(hostile.id) ?? {
                                      status: "untried",
                                      metric: null,
                                      best: null,
                                    }
                                  }
                                  isNext={frontier.nextTargetId === hostile.id}
                                />
                              </div>
                            </div>
                            <button
                              type="button"
                              onClick={() => openOptimizer(hostile, best)}
                              // One CPU permit server-side: a manual run started
                              // mid-climb would hang until the climb releases it.
                              disabled={climb.isClimbing}
                              title={
                                climb.isClimbing
                                  ? "Unavailable while this loop is climbing"
                                  : undefined
                              }
                            >
                              {best ? "Re-optimize" : "Optimize"}
                            </button>
                          </div>
                          {best ? (
                            <BestCrewSummary record={best} />
                          ) : (
                            <div className="loop-unattempted">
                              No saved crew for{" "}
                              {selectedGoal.shortLabel.toLocaleLowerCase()} yet.
                            </div>
                          )}
                        </article>
                      );
                    })}
                  </div>
                </section>
              ))}
              <div className="loop-ladder__bottom">
                Start here · lower challenge
              </div>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
