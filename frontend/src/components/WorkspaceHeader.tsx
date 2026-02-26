import { useState, useEffect } from 'react';
import { fetchShips, fetchHostiles } from '../lib/api';
import type { ShipListItem, HostileListItem, OptimizeEstimate } from '../lib/api';
import type { CrewState } from '../lib/types';

const SIMS_PRESETS = [1000, 5000, 10000, 50000] as const;

interface WorkspaceHeaderProps {
  shipId: string;
  scenarioId: string;
  onShipIdChange: (id: string) => void;
  onScenarioIdChange: (id: string) => void;
  shipLevel: number;
  onShipLevelChange: (level: number) => void;
  crew: CrewState;
  simsPerCrew: number;
  onSimsPerCrewChange: (n: number) => void;
  estimate: OptimizeEstimate | null;
  lastOptimizeDurationMs: number | null;
  onRunSim: () => void;
  onRunOptimize: () => void;
  onSavePreset: () => void;
  loadingSim: boolean;
  loadingOptimize: boolean;
  optimizeProgress: number | null;
  optimizeCrewsDone: number | null;
  optimizeTotalCrews: number | null;
}

export default function WorkspaceHeader({
  shipId,
  scenarioId,
  onShipIdChange,
  onScenarioIdChange,
  shipLevel,
  onShipLevelChange,
  simsPerCrew,
  onSimsPerCrewChange,
  estimate,
  lastOptimizeDurationMs,
  onRunSim,
  onRunOptimize,
  onSavePreset,
  loadingSim,
  loadingOptimize,
  optimizeProgress,
  optimizeCrewsDone,
  optimizeTotalCrews,
}: WorkspaceHeaderProps) {
  const [ships, setShips] = useState<ShipListItem[]>([]);
  const [hostiles, setHostiles] = useState<HostileListItem[]>([]);

  useEffect(() => {
    let c = false;
    fetchShips().then((list) => {
      if (!c) setShips(list);
      if (list.length && !shipId) onShipIdChange(list[0]?.id ?? '');
    });
    return () => { c = true; };
  }, []);
  useEffect(() => {
    let c = false;
    fetchHostiles().then((list) => {
      if (!c) setHostiles(list);
      if (list.length && !scenarioId) onScenarioIdChange(list[0]?.id ?? '');
    });
    return () => { c = true; };
  }, []);

  return (
    <header
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: '1rem',
        padding: '0.75rem 1rem',
        background: 'var(--surface)',
        borderBottom: '1px solid var(--border)',
        flexWrap: 'wrap',
      }}
    >
      <select
        aria-label="Ship"
        value={shipId}
        onChange={(e) => onShipIdChange(e.target.value)}
        style={{
          padding: '0.4rem 0.6rem',
          background: 'var(--bg)',
          border: '1px solid var(--border)',
          borderRadius: 6,
          color: 'var(--text)',
        }}
      >
        {ships.length === 0 && <option>Loading…</option>}
        {ships.map((s) => (
          <option key={s.id} value={s.id}>
            {s.ship_name}
          </option>
        ))}
      </select>
      <select
        aria-label="Ship level"
        value={shipLevel}
        onChange={(e) => onShipLevelChange(Number(e.target.value))}
        style={{
          padding: '0.4rem 0.6rem',
          background: 'var(--bg)',
          border: '1px solid var(--border)',
          borderRadius: 6,
          color: 'var(--text)',
        }}
      >
        {[1, 10, 20, 30, 40, 50, 60].map((l) => (
          <option key={l} value={l}>
            Lvl {l}
          </option>
        ))}
      </select>
      <select
        aria-label="Scenario"
        value={scenarioId}
        onChange={(e) => onScenarioIdChange(e.target.value)}
        style={{
          padding: '0.4rem 0.6rem',
          background: 'var(--bg)',
          border: '1px solid var(--border)',
          borderRadius: 6,
          color: 'var(--text)',
        }}
      >
        {hostiles.length === 0 && <option>Loading…</option>}
        {hostiles.map((h) => (
          <option key={h.id} value={h.id}>
            {h.hostile_name} {h.level}
          </option>
        ))}
      </select>
      <select
        aria-label="Preset"
        style={{
          padding: '0.4rem 0.6rem',
          background: 'var(--bg)',
          border: '1px solid var(--border)',
          borderRadius: 6,
          color: 'var(--text)',
        }}
      >
        <option>Preset</option>
      </select>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: '0.5rem',
          flexWrap: 'wrap',
        }}
      >
        <label style={{ fontSize: '0.8rem', color: 'var(--text-muted)', display: 'flex', alignItems: 'center', gap: 6 }}>
          Fight iterations/crew
          <input
            type="number"
            min={1}
            max={100000}
            value={simsPerCrew}
            onChange={(e) => onSimsPerCrewChange(Math.max(1, Math.min(100000, Number(e.target.value) || 1000)))}
            style={{
              width: 72,
              padding: '0.35rem 0.5rem',
              background: 'var(--bg)',
              border: '1px solid var(--border)',
              borderRadius: 4,
              color: 'var(--text)',
            }}
          />
        </label>
        {SIMS_PRESETS.map((n) => (
          <button
            key={n}
            type="button"
            onClick={() => onSimsPerCrewChange(n)}
            style={{
              padding: '0.35rem 0.5rem',
              background: simsPerCrew === n ? 'var(--accent)' : 'var(--surface)',
              border: '1px solid var(--border)',
              borderRadius: 4,
              color: simsPerCrew === n ? 'var(--bg)' : 'var(--text)',
              fontSize: '0.8rem',
            }}
          >
            {n >= 1000 ? `${n / 1000}k` : n}
          </button>
        ))}
      </div>
      {estimate != null && (
        <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
          Est. ~{estimate.estimated_seconds < 1 ? '<1' : estimate.estimated_seconds.toFixed(1)} s
          {estimate.estimated_candidates > 0 && ` (${estimate.estimated_candidates} crews)`}
        </span>
      )}
      {lastOptimizeDurationMs != null && (
        <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
          Completed in {(lastOptimizeDurationMs / 1000).toFixed(1)} s
        </span>
      )}
      {loadingOptimize && optimizeProgress != null && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 120 }}>
          <div
            style={{
              flex: 1,
              height: 6,
              background: 'var(--border)',
              borderRadius: 3,
              overflow: 'hidden',
            }}
          >
            <div
              style={{
                width: `${optimizeProgress}%`,
                height: '100%',
                background: 'var(--accent)',
                borderRadius: 3,
                transition: 'width 0.2s ease',
              }}
            />
          </div>
          <span style={{ fontSize: '0.75rem', color: 'var(--text-muted)', whiteSpace: 'nowrap' }}>
            {optimizeTotalCrews != null && optimizeCrewsDone != null && optimizeTotalCrews > 0
              ? `${optimizeCrewsDone}/${optimizeTotalCrews} (${optimizeProgress}%)`
              : `${optimizeProgress}%`}
          </span>
        </div>
      )}
      <div style={{ flex: 1, minWidth: 8 }} />
      <button
        type="button"
        onClick={onSavePreset}
        style={{
          padding: '0.5rem 1rem',
          background: 'var(--surface)',
          border: '1px solid var(--border)',
          borderRadius: 6,
          color: 'var(--text)',
        }}
      >
        Save as Preset
      </button>
      <button
        type="button"
        onClick={onRunSim}
        disabled={loadingSim || loadingOptimize}
        style={{
          padding: '0.5rem 1rem',
          background: 'var(--accent-dim)',
          border: 'none',
          borderRadius: 6,
          color: 'var(--text)',
        }}
      >
        {loadingSim ? 'Running…' : 'Run Sim'}
      </button>
      <button
        type="button"
        onClick={onRunOptimize}
        disabled={loadingSim || loadingOptimize}
        style={{
          padding: '0.5rem 1rem',
          background: 'var(--accent)',
          border: 'none',
          borderRadius: 6,
          color: 'var(--bg)',
        }}
      >
        {loadingOptimize
          ? (optimizeProgress != null ? `Optimizing… ${optimizeProgress}%` : 'Running…')
          : 'Run Optimize'}
      </button>
    </header>
  );
}
