interface OptimizePanelProps {
  collapsed: boolean;
  onToggleCollapsed: () => void;
  crew: import('../lib/types').CrewState;
  loadingOptimize: boolean;
  optimizeCrewsDone: number | null;
  optimizeTotalCrews: number | null;
  maxCandidates: number | null;
  onMaxCandidatesChange: (value: number | null) => void;
  prioritizeBelowDecksAbility: boolean;
  onPrioritizeBelowDecksAbilityChange: (value: boolean) => void;
}

export default function OptimizePanel({
  collapsed,
  onToggleCollapsed,
  loadingOptimize,
  optimizeCrewsDone,
  optimizeTotalCrews,
  maxCandidates,
  onMaxCandidatesChange,
  prioritizeBelowDecksAbility,
  onPrioritizeBelowDecksAbilityChange,
}: OptimizePanelProps) {
  if (collapsed) {
    return (
      <aside
        style={{
          width: 48,
          background: 'var(--surface)',
          borderLeft: '1px solid var(--border)',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          padding: '0.5rem',
        }}
      >
        <button
          type="button"
          onClick={onToggleCollapsed}
          aria-label="Expand panel"
          style={{
            padding: 4,
            background: 'transparent',
            border: 'none',
            color: 'var(--text-muted)',
          }}
        >
          →
        </button>
        <span style={{ fontSize: 10, color: 'var(--text-muted)', marginTop: 8 }}>Strategy</span>
        <span style={{ fontSize: 10, color: 'var(--text-muted)' }}>—</span>
      </aside>
    );
  }

  return (
    <aside
      style={{
        width: 280,
        minWidth: 240,
        background: 'var(--surface)',
        borderLeft: '1px solid var(--border)',
        padding: '1rem',
        display: 'flex',
        flexDirection: 'column',
        gap: '0.75rem',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <h2 style={{ margin: 0, fontSize: '1rem' }}>OptimizePanel</h2>
        <button
          type="button"
          onClick={onToggleCollapsed}
          aria-label="Collapse panel"
          style={{
            padding: 4,
            background: 'transparent',
            border: 'none',
            color: 'var(--text-muted)',
          }}
        >
          ←
        </button>
      </div>
      <label style={{ fontSize: '0.85rem' }}>
        Strategy
        <select
          style={{
            display: 'block',
            marginTop: 4,
            width: '100%',
            padding: '0.4rem',
            background: 'var(--bg)',
            border: '1px solid var(--border)',
            borderRadius: 4,
            color: 'var(--text)',
          }}
        >
          <option>Tiered</option>
          <option>Hill climb</option>
          <option>Genetic</option>
        </select>
      </label>
      <label style={{ fontSize: '0.85rem' }}>
        Primary metric
        <select
          style={{
            display: 'block',
            marginTop: 4,
            width: '100%',
            padding: '0.4rem',
            background: 'var(--bg)',
            border: '1px solid var(--border)',
            borderRadius: 4,
            color: 'var(--text)',
          }}
        >
          <option>Win rate</option>
          <option>Hull remaining</option>
        </select>
      </label>
      <label style={{ fontSize: '0.85rem' }}>
        Max crews (optional)
        <input
          type="number"
          min={1}
          max={2_000_000}
          step={1}
          placeholder="No limit"
          value={maxCandidates ?? ''}
          onChange={(e) => {
            const raw = e.target.value.trim();
            if (raw === '') {
              onMaxCandidatesChange(null);
              return;
            }
            const n = parseInt(raw, 10);
            if (!Number.isNaN(n) && n >= 1) {
              onMaxCandidatesChange(Math.min(n, 2_000_000));
            }
          }}
          style={{
            display: 'block',
            marginTop: 4,
            width: '100%',
            padding: '0.4rem',
            background: 'var(--bg)',
            border: '1px solid var(--border)',
            borderRadius: 4,
            color: 'var(--text)',
          }}
        />
      </label>
      <label style={{ fontSize: '0.85rem', display: 'flex', alignItems: 'center', gap: '0.5rem', cursor: 'pointer' }}>
        <input
          type="checkbox"
          checked={prioritizeBelowDecksAbility}
          onChange={(e) => onPrioritizeBelowDecksAbilityChange(e.target.checked)}
          style={{ margin: 0 }}
        />
        <span>Only below-decks officers with ability</span>
      </label>
      <p style={{ margin: 0, fontSize: '0.8rem', color: 'var(--text-muted)' }}>
        {loadingOptimize &&
        optimizeCrewsDone != null &&
        optimizeTotalCrews != null &&
        optimizeTotalCrews > 0
          ? `Live status: ${optimizeCrewsDone} / ${optimizeTotalCrews} crews`
          : 'Live status: — sims, — sims/sec'}
      </p>
    </aside>
  );
}
