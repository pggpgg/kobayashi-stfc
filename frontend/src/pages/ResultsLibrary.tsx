import { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { fetchPresets, fetchPreset } from '../lib/api';
import type { PresetSummary } from '../lib/api';

export default function ResultsLibrary() {
  const [presets, setPresets] = useState<PresetSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const navigate = useNavigate();

  useEffect(() => {
    let c = false;
    fetchPresets()
      .then((list) => {
        if (!c) setPresets(list);
      })
      .catch((e) => {
        if (!c) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!c) setLoading(false);
      });
    return () => { c = true; };
  }, []);

  const handleLoad = async (id: string) => {
    setError(null);
    try {
      const preset = await fetchPreset(id);
      navigate('/', { state: { preset } });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div>
      <h1 style={{ marginBottom: '1rem' }}>Results Library</h1>
      <p style={{ marginBottom: '1rem', color: 'var(--text-muted)', fontSize: '0.9rem' }}>
        Saved presets. Load one to apply it in the Workspace.
      </p>

      {error && (
        <div style={{ marginBottom: 8, padding: 8, background: 'var(--error)', color: 'white', borderRadius: 6 }}>
          {error}
        </div>
      )}

      {loading && <p>Loading…</p>}

      {!loading && presets.length === 0 && (
        <p style={{ color: 'var(--text-muted)' }}>No saved presets. Save a crew from the Workspace.</p>
      )}

      {!loading && presets.length > 0 && (
        <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
          {presets.map((p) => (
            <li
              key={p.id}
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                padding: '0.75rem',
                border: '1px solid var(--border)',
                borderRadius: 6,
                marginBottom: 8,
                background: 'var(--surface)',
              }}
            >
              <div>
                <strong>{p.name}</strong>
                <span style={{ marginLeft: 8, color: 'var(--text-muted)', fontSize: '0.9rem' }}>
                  {p.ship} / {p.scenario}
                </span>
              </div>
              <button
                type="button"
                onClick={() => handleLoad(p.id)}
                style={{
                  padding: '0.4rem 0.75rem',
                  background: 'var(--accent)',
                  border: 'none',
                  borderRadius: 6,
                  color: 'var(--bg)',
                }}
              >
                Load
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
