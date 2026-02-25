import { useState } from 'react';
import type { SimulateStats } from '../lib/api';
import type { CrewRecommendation } from '../lib/api';

interface SimResultsProps {
  simResult: SimulateStats | null;
  recommendations: CrewRecommendation[];
  loadingSim: boolean;
  loadingOptimize: boolean;
}

export default function SimResults({
  simResult,
  recommendations,
  loadingSim,
  loadingOptimize,
}: SimResultsProps) {
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const hasSim = simResult != null;
  const hasRecs = recommendations.length > 0;

  const toggleSelect = (i: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(i)) next.delete(i);
      else if (next.size < 5) next.add(i);
      return next;
    });
  };

  const selectedList = Array.from(selected).sort((a, b) => a - b);
  const showCompare = selectedList.length >= 2 && selectedList.length <= 5;

  return (
    <section
      style={{
        padding: '1rem',
        background: 'var(--surface)',
        border: '1px solid var(--border)',
        borderRadius: 8,
        overflow: 'auto',
      }}
    >
      <h2 style={{ margin: '0 0 0.75rem', fontSize: '1rem' }}>SimResults</h2>

      {(loadingSim || loadingOptimize) && (
        <p style={{ margin: 0, color: 'var(--text-muted)' }}>
          {loadingOptimize
            ? 'Optimization in progress… This may take a minute depending on scenario.'
            : 'Running…'}
        </p>
      )}

      {hasSim && !loadingSim && (
        <div
          style={{
            marginBottom: '1rem',
            padding: '0.75rem',
            background: 'var(--bg)',
            borderRadius: 6,
          }}
        >
          <strong>Last sim (current crew)</strong>
          <div style={{ marginTop: 4 }}>
            Win rate: {(simResult.win_rate * 100).toFixed(2)}% (n={simResult.n})
            {simResult.win_rate_95_ci && (
              <span style={{ marginLeft: 8, fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                95% CI: [{simResult.win_rate_95_ci[0].toFixed(3)}, {simResult.win_rate_95_ci[1].toFixed(3)}]
              </span>
            )}
          </div>
          <div>Avg hull remaining: {(simResult.avg_hull_remaining * 100).toFixed(2)}%</div>
        </div>
      )}

      {hasRecs && (
        <>
          <p style={{ margin: '0 0 0.5rem', fontSize: '0.85rem', color: 'var(--text-muted)' }}>
            Select 2–5 rows to compare.
          </p>
          <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.9rem' }}>
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border)' }}>
                <th style={{ textAlign: 'left', padding: '0.4rem', width: 32 }} />
                <th style={{ textAlign: 'left', padding: '0.4rem' }}>#</th>
                <th style={{ textAlign: 'left', padding: '0.4rem' }}>Captain</th>
                <th style={{ textAlign: 'left', padding: '0.4rem' }}>Bridge</th>
                <th style={{ textAlign: 'left', padding: '0.4rem' }}>Below Deck</th>
                <th style={{ textAlign: 'right', padding: '0.4rem' }}>Win %</th>
                <th style={{ textAlign: 'right', padding: '0.4rem' }}>Hull %</th>
              </tr>
            </thead>
            <tbody>
              {recommendations.map((r, i) => (
                <tr
                  key={i}
                  style={{
                    borderBottom: '1px solid var(--border)',
                    background: selected.has(i) ? 'rgba(232,149,46,0.1)' : undefined,
                  }}
                >
                  <td style={{ padding: '0.4rem' }}>
                    <input
                      type="checkbox"
                      checked={selected.has(i)}
                      onChange={() => toggleSelect(i)}
                      aria-label={`Select row ${i + 1}`}
                    />
                  </td>
                  <td style={{ padding: '0.4rem' }}>{i + 1}</td>
                  <td style={{ padding: '0.4rem' }}>{r.captain}</td>
                  <td style={{ padding: '0.4rem' }}>{r.bridge}</td>
                  <td style={{ padding: '0.4rem' }}>{r.below_decks}</td>
                  <td style={{ padding: '0.4rem', textAlign: 'right' }}>
                    {(r.win_rate * 100).toFixed(2)}
                  </td>
                  <td style={{ padding: '0.4rem', textAlign: 'right' }}>
                    {(r.avg_hull_remaining * 100).toFixed(2)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>

          {showCompare && (
            <div
              style={{
                marginTop: '1rem',
                padding: '0.75rem',
                background: 'var(--bg)',
                border: '1px solid var(--border)',
                borderRadius: 6,
              }}
            >
              <strong>Compare (delta)</strong>
              <div style={{ marginTop: 8, display: 'flex', flexDirection: 'column', gap: 4 }}>
                {selectedList.map((idx, j) => {
                  const r = recommendations[idx];
                  const prev = j === 0 ? null : recommendations[selectedList[j - 1]];
                  const deltaWin = prev != null ? (r.win_rate - prev.win_rate) * 100 : 0;
                  const deltaHull = prev != null ? (r.avg_hull_remaining - prev.avg_hull_remaining) * 100 : 0;
                  return (
                    <div key={idx} style={{ fontSize: '0.85rem' }}>
                      <span style={{ fontWeight: 600 }}>#{idx + 1}</span> {r.captain} / {r.bridge} / {r.below_decks}
                      {prev != null && (
                        <span style={{ marginLeft: 8, color: 'var(--text-muted)' }}>
                          Δ Win {deltaWin >= 0 ? '+' : ''}{deltaWin.toFixed(2)}%, Δ Hull {deltaHull >= 0 ? '+' : ''}{deltaHull.toFixed(2)}%
                        </span>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </>
      )}

      {!hasSim && !hasRecs && !loadingSim && !loadingOptimize && (
        <p style={{ margin: 0, color: 'var(--text-muted)', fontSize: '0.9rem' }}>
          Run Sim for current crew or Run Optimize for ranked recommendations.
        </p>
      )}
    </section>
  );
}
