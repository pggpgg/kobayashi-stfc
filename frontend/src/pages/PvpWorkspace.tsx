import { useEffect, useState } from "react";
import CrewBuilder from "../components/CrewBuilder";
import ProfileSwitcher from "../components/ProfileSwitcher";
import SimResults from "../components/SimResults";
import { useProfile } from "../contexts/ProfileContext";
import { useWorkspaceMode } from "../contexts/WorkspaceModeContext";
import type { ShipListItem } from "../lib/api";
import { fetchShips, getShipTiersLevels } from "../lib/api";
import { belowDeckSlotCount } from "../lib/types";
import { usePvpWorkspace } from "../lib/usePvpWorkspace";

const selectStyle = {
  padding: "0.4rem 0.6rem",
  background: "var(--bg)",
  border: "1px solid var(--border)",
  borderRadius: 6,
  color: "var(--text)",
} as const;

function ShipSelectors({
  label,
  shipId,
  onShipIdChange,
  shipTier,
  onShipTierChange,
  shipLevel,
  onShipLevelChange,
  ships,
  onBelowDeckUnlockLevelsChange,
}: {
  label: string;
  shipId: string;
  onShipIdChange: (id: string) => void;
  shipTier: number;
  onShipTierChange: (t: number) => void;
  shipLevel: number;
  onShipLevelChange: (l: number) => void;
  ships: ShipListItem[];
  onBelowDeckUnlockLevelsChange?: (levels: number[]) => void;
}) {
  const [tiers, setTiers] = useState<number[]>([1]);
  const [levels, setLevels] = useState<number[]>([50]);

  useEffect(() => {
    if (!shipId) return;
    let cancelled = false;
    getShipTiersLevels(shipId).then((tl) => {
      if (cancelled || !tl) return;
      setTiers(tl.tiers?.length ? tl.tiers : [1]);
      setLevels(tl.levels?.length ? tl.levels : [50]);
      if (tl.crew_slots?.length && onBelowDeckUnlockLevelsChange) {
        onBelowDeckUnlockLevelsChange(
          tl.crew_slots.map((s) => Number(s.unlock_level)),
        );
      }
    });
    return () => {
      cancelled = true;
    };
  }, [shipId, onBelowDeckUnlockLevelsChange]);

  return (
    <ShipRowDisplay
      label={label}
      shipId={shipId}
      onShipIdChange={onShipIdChange}
      shipTier={shipTier}
      onShipTierChange={onShipTierChange}
      shipLevel={shipLevel}
      onShipLevelChange={onShipLevelChange}
      ships={ships}
      tiers={tiers}
      levels={levels}
    />
  );
}

function ShipRowDisplay({
  label,
  shipId,
  onShipIdChange,
  shipTier,
  onShipTierChange,
  shipLevel,
  onShipLevelChange,
  ships,
  tiers,
  levels,
}: {
  label: string;
  shipId: string;
  onShipIdChange: (id: string) => void;
  shipTier: number;
  onShipTierChange: (t: number) => void;
  shipLevel: number;
  onShipLevelChange: (l: number) => void;
  ships: ShipListItem[];
  tiers: number[];
  levels: number[];
}) {
  return (
    <div
      style={{
        display: "flex",
        flexWrap: "wrap",
        gap: "0.75rem",
        alignItems: "center",
        marginBottom: "0.75rem",
      }}
    >
      <span style={{ fontWeight: 600, minWidth: 80 }}>{label}</span>
      <select
        value={shipId}
        onChange={(e) => onShipIdChange(e.target.value)}
        style={{ ...selectStyle, minWidth: 200 }}
      >
        <option value="">Ship…</option>
        {ships.map((s) => (
          <option key={s.id} value={s.id}>
            {typeof s.name === "string" ? s.name : s.id}
          </option>
        ))}
      </select>
      <label>
        Tier{" "}
        <select
          value={shipTier}
          onChange={(e) => onShipTierChange(Number(e.target.value))}
          style={selectStyle}
        >
          {tiers.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
      </label>
      <label>
        Level{" "}
        <select
          value={shipLevel}
          onChange={(e) => onShipLevelChange(Number(e.target.value))}
          style={selectStyle}
        >
          {levels.map((l) => (
            <option key={l} value={l}>
              {l}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}

export default function PvpWorkspace() {
  const pvp = usePvpWorkspace();
  const { activeProfileId } = useProfile();
  const { ownedOnly } = useWorkspaceMode();
  const [ships, setShips] = useState<ShipListItem[]>([]);
  useEffect(() => {
    fetchShips(ownedOnly, activeProfileId).then(setShips);
  }, [ownedOnly, activeProfileId]);

  return (
    <div
      style={{ display: "flex", flexDirection: "column", minHeight: "100vh" }}
    >
      <header
        style={{
          padding: "1rem 1.25rem",
          borderBottom: "1px solid var(--border)",
          background: "var(--surface)",
        }}
      >
        <h1 style={{ margin: "0 0 0.5rem", fontSize: "1.25rem" }}>PvP</h1>
        <p
          style={{
            margin: "0 0 1rem",
            color: "var(--text-muted)",
            fontSize: "0.9rem",
          }}
        >
          Optimize attacker crews vs a fixed opponent ship and profile.
        </p>
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: "1rem",
            alignItems: "center",
          }}
        >
          <ProfileSwitcher />
          <label>
            Opponent profile{" "}
            <select
              value={pvp.opponentProfileId}
              onChange={(e) => pvp.setOpponentProfileId(e.target.value)}
              style={{ ...selectStyle, minWidth: 160 }}
            >
              <option value="">Select…</option>
              {pvp.profiles.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name ?? p.id}
                </option>
              ))}
            </select>
          </label>
          <label>
            Sims{" "}
            <input
              type="number"
              min={100}
              max={100000}
              value={pvp.simsPerCrew}
              onChange={(e) => pvp.setSimsPerCrew(Number(e.target.value))}
              style={{ ...selectStyle, width: 100 }}
            />
          </label>
          <button
            type="button"
            onClick={() => void pvp.handleRunSim()}
            disabled={!pvp.canRun || pvp.loadingSim}
          >
            {pvp.loadingSim ? "Simulating…" : "Run sim"}
          </button>
          <button
            type="button"
            onClick={() => void pvp.handleRunOptimize()}
            disabled={!pvp.canRun || pvp.loadingOptimize}
          >
            {pvp.loadingOptimize ? "Optimizing…" : "Run optimize"}
          </button>
        </div>
      </header>

      {pvp.error && <ErrorBanner message={pvp.error} />}

      <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
        <main style={{ flex: 1, padding: "1rem 1.25rem", overflow: "auto" }}>
          <section style={{ marginBottom: "1.5rem" }}>
            <h2 style={{ fontSize: "1rem", marginBottom: "0.5rem" }}>
              Attacker (your profile)
            </h2>
            <ShipSelectors
              label="Ship"
              shipId={pvp.attackerShipId}
              onShipIdChange={pvp.setAttackerShipId}
              shipTier={pvp.attackerShipTier}
              onShipTierChange={pvp.setAttackerShipTier}
              shipLevel={pvp.attackerShipLevel}
              onShipLevelChange={pvp.setAttackerShipLevel}
              ships={ships}
              onBelowDeckUnlockLevelsChange={pvp.setBelowDeckUnlockLevels}
            />
            <CrewBuilder
              belowDecksSlots={belowDeckSlotCount(
                pvp.attackerShipLevel,
                pvp.belowDeckUnlockLevels,
              )}
              crew={pvp.attackerCrew}
              onCrewChange={pvp.setAttackerCrew}
              pins={pvp.attackerPins}
              onPinsChange={pvp.setAttackerPins}
            />
          </section>

          <section>
            <h2 style={{ fontSize: "1rem", marginBottom: "0.5rem" }}>
              Defender (opponent)
            </h2>
            <ShipSelectors
              label="Ship"
              shipId={pvp.defenderShipId}
              onShipIdChange={pvp.setDefenderShipId}
              shipTier={pvp.defenderShipTier}
              onShipTierChange={pvp.setDefenderShipTier}
              shipLevel={pvp.defenderShipLevel}
              onShipLevelChange={pvp.setDefenderShipLevel}
              ships={ships}
            />
            <CrewBuilder
              belowDecksSlots={belowDeckSlotCount(
                pvp.defenderShipLevel,
                pvp.belowDeckUnlockLevels,
              )}
              crew={pvp.defenderCrew}
              onCrewChange={pvp.setDefenderCrew}
              pins={pvp.defenderPins}
              onPinsChange={pvp.setDefenderPins}
            />
          </section>
        </main>

        <aside
          style={{
            width: 360,
            borderLeft: "1px solid var(--border)",
            padding: "1rem",
            overflow: "auto",
            background: "var(--surface)",
          }}
        >
          <SimResults
            simResult={pvp.simResult}
            recommendations={pvp.recommendations}
            loadingSim={pvp.loadingSim}
            loadingOptimize={pvp.loadingOptimize}
            optimizeProgress={null}
            optimizeCrewsDone={null}
            optimizeTotalCrews={null}
          />
        </aside>
      </div>
    </div>
  );
}

function ErrorBanner({ message }: { message: string }) {
  return (
    <div
      style={{
        padding: "0.75rem 1.25rem",
        background: "rgba(200, 60, 60, 0.15)",
        borderBottom: "1px solid var(--border)",
      }}
    >
      {message}
    </div>
  );
}
