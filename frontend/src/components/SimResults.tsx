import { useState, useEffect } from 'react';
import type { SimulateStats } from '../lib/api';
import type { CrewRecommendation } from '../lib/api';

const PER_PAGE_OPTIONS = [50, 100, 200, 500] as const;
const DEFAULT_PER_PAGE = 50;

/** Normalize captain/bridge/below_decks for display: API may return string[]; join with ", ". */
function formatCrewCell(value: string | string[] | null | undefined): string {
  if (value == null) return '';
  if (Array.isArray(value)) return value.filter(Boolean).join(', ');
  return String(value);
}

interface SimResultsProps {
  simResult: SimulateStats | null;
  recommendations: CrewRecommendation[];
  loadingSim: boolean;
  loadingOptimize: boolean;
  optimizeProgress: number | null;
  optimizeCrewsDone: number | null;
  optimizeTotalCrews: number | null;
}

export default function SimResults({
  simResult,
  recommendations,
  loadingSim,
  loadingOptimize,
  optimizeProgress,
  optimizeCrewsDone,
  optimizeTotalCrews,
}: SimResultsProps) {
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [page, setPage] = useState(1);
  const [perPage, setPerPage] = useState(DEFAULT_PER_PAGE);
  const hasSim = simResult != null;
  const hasRecs = recommendations.length > 0;

  const total = recommendations.length;
  const totalPages = Math.max(1, Math.ceil(total / perPage));
  const safePage = Math.min(page, totalPages);
  const start = (safePage - 1) * perPage;
  const pageRecs = recommendations.slice(start, start + perPage);

  // Reset to page 1 when recommendations change (e.g. new optimize run) or when current page is out of range
  useEffect(() => {
    if (page > totalPages && totalPages >= 1) setPage(1);
  }, [totalPages, total]);

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
        <div style={{ marginBottom: '0.75rem' }}>
          <p style={{ margin: 0, color: 'var(--text-muted)' }}>
            {loadingOptimize
              ? 'Optimization in progress… This may take a minute depending on scenario.'
              : 'Running…'}
          </p>
          {loadingOptimize && optimizeProgress != null && (
            <div style={{ marginTop: 8 }}>
              <div
                style={{
                  height: 10,
                  background: 'var(--border)',
                  borderRadius: 5,
                  overflow: 'hidden',
                }}
              >
                <div
                  style={{
                    width: `${optimizeProgress}%`,
                    height: '100%',
                    background: 'var(--accent)',
                    borderRadius: 5,
                    transition: 'width 0.2s ease',
                  }}
                />
              </div>
              <p style={{ margin: '4px 0 0', fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                {optimizeTotalCrews != null && optimizeCrewsDone != null
                  ? `${optimizeCrewsDone} / ${optimizeTotalCrews} crews (${optimizeProgress}%)`
                  : `${optimizeProgress}%`}
              </p>
            </div>
          )}
        </div>
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
          <div style={{ marginTop: 4, display: 'flex', flexWrap: 'wrap', gap: '0.5rem 1.5rem' }}>
            <span>Win rate: {(simResult.win_rate * 100).toFixed(2)}%</span>
            <span>Stall rate: {(simResult.stall_rate * 100).toFixed(2)}%</span>
            <span>Loss rate: {(simResult.loss_rate * 100).toFixed(2)}%</span>
            <span>Avg hull remaining: {(simResult.avg_hull_remaining * 100).toFixed(2)}%</span>
            <span style={{ color: 'var(--text-muted)' }}>(n={simResult.n})</span>
            {simResult.win_rate_95_ci && (
              <span style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                95% CI: [{simResult.win_rate_95_ci[0].toFixed(3)}, {simResult.win_rate_95_ci[1].toFixed(3)}]
              </span>
            )}
          </div>
        </div>
      )}

      {hasRecs && (
        <>
          <p style={{ margin: '0 0 0.5rem', fontSize: '0.85rem', color: 'var(--text-muted)' }}>
            Select 2–5 rows to compare.
          </p>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '1rem',
              flexWrap: 'wrap',
              marginBottom: '0.5rem',
              fontSize: '0.85rem',
            }}
          >
            <label style={{ display: 'flex', alignItems: 'center', gap: 6, color: 'var(--text-muted)' }}>
              Results per page
              <select
                value={perPage}
                onChange={(e) => {
                  const n = Number(e.target.value);
                  setPerPage(n);
                  setPage((p) => Math.min(p, Math.max(1, Math.ceil(total / n))));
                }}
                style={{
                  padding: '0.25rem 0.5rem',
                  background: 'var(--bg)',
                  border: '1px solid var(--border)',
                  borderRadius: 4,
                  color: 'var(--text)',
                }}
                aria-label="Results per page"
              >
                {PER_PAGE_OPTIONS.map((n) => (
                  <option key={n} value={n}>
                    {n}
                  </option>
                ))}
              </select>
            </label>
            <span style={{ color: 'var(--text-muted)' }}>
              Showing {start + 1}–{Math.min(start + perPage, total)} of {total}
            </span>
            {totalPages > 1 && (
              <span style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                <button
                  type="button"
                  onClick={() => setPage((p) => Math.max(1, p - 1))}
                  disabled={safePage <= 1}
                  aria-label="Previous page"
                  style={{
                    padding: '0.25rem 0.5rem',
                    background: 'var(--bg)',
                    border: '1px solid var(--border)',
                    borderRadius: 4,
                    color: 'var(--text)',
                    cursor: safePage <= 1 ? 'not-allowed' : 'pointer',
                    opacity: safePage <= 1 ? 0.6 : 1,
                  }}
                >
                  Prev
                </button>
                <span style={{ color: 'var(--text-muted)', minWidth: 80, textAlign: 'center' }}>
                  Page {safePage} of {totalPages}
                </span>
                <button
                  type="button"
                  onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
                  disabled={safePage >= totalPages}
                  aria-label="Next page"
                  style={{
                    padding: '0.25rem 0.5rem',
                    background: 'var(--bg)',
                    border: '1px solid var(--border)',
                    borderRadius: 4,
                    color: 'var(--text)',
                    cursor: safePage >= totalPages ? 'not-allowed' : 'pointer',
                    opacity: safePage >= totalPages ? 0.6 : 1,
                  }}
                >
                  Next
                </button>
              </span>
            )}
          </div>
          <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.9rem' }}>
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border)' }}>
                <th style={{ textAlign: 'left', padding: '0.4rem', width: 32 }} />
                <th style={{ textAlign: 'left', padding: '0.4rem' }}>#</th>
                <th style={{ textAlign: 'left', padding: '0.4rem' }}>Captain</th>
                <th style={{ textAlign: 'left', padding: '0.4rem' }}>Bridge</th>
                <th style={{ textAlign: 'left', padding: '0.4rem' }}>Below Deck</th>
                <th style={{ textAlign: 'right', padding: '0.4rem' }}>Win %</th>
                <th style={{ textAlign: 'right', padding: '0.4rem' }}>Stall %</th>
                <th style={{ textAlign: 'right', padding: '0.4rem' }}>Loss %</th>
                <th style={{ textAlign: 'right', padding: '0.4rem' }}>Hull %</th>
              </tr>
            </thead>
            <tbody>
              {pageRecs.map((r, i) => {
                const globalIndex = start + i;
                return (
                  <tr
                    key={globalIndex}
                    style={{
                      borderBottom: '1px solid var(--border)',
                      background: selected.has(globalIndex) ? 'rgba(232,149,46,0.1)' : undefined,
                    }}
                  >
                    <td style={{ padding: '0.4rem' }}>
                      <input
                        type="checkbox"
                        checked={selected.has(globalIndex)}
                        onChange={() => toggleSelect(globalIndex)}
                        aria-label={`Select row ${globalIndex + 1}`}
                      />
                    </td>
                    <td style={{ padding: '0.4rem' }}>{globalIndex + 1}</td>
                    <td style={{ padding: '0.4rem' }}>{formatCrewCell(r.captain)}</td>
                    <td style={{ padding: '0.4rem' }}>{formatCrewCell(r.bridge)}</td>
                    <td style={{ padding: '0.4rem' }}>{formatCrewCell(r.below_decks)}</td>
                    <td style={{ padding: '0.4rem', textAlign: 'right' }}>
                      {(r.win_rate * 100).toFixed(2)}
                    </td>
                    <td style={{ padding: '0.4rem', textAlign: 'right' }}>
                      {(r.stall_rate * 100).toFixed(2)}
                    </td>
                    <td style={{ padding: '0.4rem', textAlign: 'right' }}>
                      {(r.loss_rate * 100).toFixed(2)}
                    </td>
                    <td style={{ padding: '0.4rem', textAlign: 'right' }}>
                      {(r.avg_hull_remaining * 100).toFixed(2)}
                    </td>
                  </tr>
                );
              })}
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
                  const deltaStall = prev != null ? (r.stall_rate - prev.stall_rate) * 100 : 0;
                  const deltaLoss = prev != null ? (r.loss_rate - prev.loss_rate) * 100 : 0;
                  const deltaHull = prev != null ? (r.avg_hull_remaining - prev.avg_hull_remaining) * 100 : 0;
                  return (
                    <div key={idx} style={{ fontSize: '0.85rem' }}>
                      <span style={{ fontWeight: 600 }}>#{idx + 1}</span> {formatCrewCell(r.captain)} / {formatCrewCell(r.bridge)} / {formatCrewCell(r.below_decks)}
                      {prev != null && (
                        <span style={{ marginLeft: 8, color: 'var(--text-muted)' }}>
                          Δ Win {deltaWin >= 0 ? '+' : ''}{deltaWin.toFixed(2)}%, Δ Stall {deltaStall >= 0 ? '+' : ''}{deltaStall.toFixed(2)}%, Δ Loss {deltaLoss >= 0 ? '+' : ''}{deltaLoss.toFixed(2)}%, Δ Hull {deltaHull >= 0 ? '+' : ''}{deltaHull.toFixed(2)}%
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
