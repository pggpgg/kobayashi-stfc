import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useProfile } from "../contexts/ProfileContext";
import {
  fetchHostiles,
  fetchShips,
  formatApiError,
  type HostileListItem,
  type ShipListItem,
} from "../lib/api";
import {
  hostileLabel,
  LOOP_CATALOG,
  LOOP_GOALS,
  type LoopGoalId,
  loopGoal,
  resolveLoopHostiles,
} from "../lib/loopsCatalog";
import {
  getLoopBestRecord,
  type LoopBestRecord,
  type LoopRunContext,
  listLoopRecords,
  savePendingLoopRun,
} from "../lib/loopsWorkspaceStorage";

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
        <span>{percent(record.winRate)} wins</span>
        <span>{percent(record.roundOneKillRate)} R1</span>
        <span>{percent(record.averageHullRemaining)} hull</span>
      </div>
      <div className="loop-best__ship">
        {record.shipId} · T{record.shipTier} L{record.shipLevel}
      </div>
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
  const loopRecords = allRecords.filter(
    (record) => record.loopId === selectedLoop.id,
  );
  const completedTargets = new Set(
    loopRecords.map((record) => record.targetId),
  );

  const selectLoop = (id: string) => {
    setSelectedLoopId(id);
    setSelectedShipId("");
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
                Optimization goal
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
            </div>
            <span>
              {completedTargets.size} / {loopHostiles.length} targets recorded
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
                            </div>
                            <button
                              type="button"
                              onClick={() => openOptimizer(hostile, best)}
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
