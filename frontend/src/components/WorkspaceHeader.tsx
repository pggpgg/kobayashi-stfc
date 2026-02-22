import { useState, useEffect } from 'react';
import { fetchShips, fetchHostiles } from '../lib/api';
import type { ShipListItem, HostileListItem } from '../lib/api';
import type { CrewState } from '../lib/types';

interface WorkspaceHeaderProps {
  shipId: string;
  scenarioId: string;
  onShipIdChange: (id: string) => void;
  onScenarioIdChange: (id: string) => void;
  shipLevel: number;
  onShipLevelChange: (level: number) => void;
  crew: CrewState;
  onRunSim: () => void;
  onRunOptimize: () => void;
  onSavePreset: () => void;
  loadingSim: boolean;
  loadingOptimize: boolean;
}

export default function WorkspaceHeader({
  shipId,
  scenarioId,
  onShipIdChange,
  onScenarioIdChange,
  shipLevel,
  onShipLevelChange,
  onRunSim,
  onRunOptimize,
  onSavePreset,
  loadingSim,
  loadingOptimize,
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
        {loadingOptimize ? 'Running…' : 'Run Optimize'}
      </button>
    </header>
  );
}
