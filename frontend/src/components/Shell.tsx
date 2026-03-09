import { ReactNode } from 'react';
import { Link, useLocation } from 'react-router-dom';
import ProfileSwitcher from './ProfileSwitcher';
import { useWorkspaceMode } from '../contexts/WorkspaceModeContext';

const NAV_ITEMS = [
  { path: '/', label: 'Workspace' },
  { path: '/results', label: 'Results Library' },
  { path: '/roster', label: 'Roster & Profile' },
  { path: '/data', label: 'Data & Mechanics' },
];

export default function Shell({ children }: { children: ReactNode }) {
  const location = useLocation();
  const { mode, setMode } = useWorkspaceMode();

  return (
    <div style={{ display: 'flex', minHeight: '100vh' }}>
      <aside
        className="rail"
        style={{
          width: 200,
          background: 'var(--surface)',
          borderRight: '1px solid var(--border)',
          padding: '1rem 0',
        }}
      >
        <div style={{ padding: '0 1rem 0.75rem', borderBottom: '1px solid var(--border)', marginBottom: '0.5rem' }}>
          <div style={{ fontSize: '0.7rem', color: 'var(--text-muted)', marginBottom: 4, textTransform: 'uppercase' }}>
            Mode
          </div>
          <div style={{ display: 'flex', gap: 4 }}>
            {(['roster', 'sandbox'] as const).map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => setMode(m)}
                style={{
                  flex: 1,
                  padding: '0.35rem 0.5rem',
                  fontSize: '0.8rem',
                  background: mode === m ? 'var(--accent)' : 'var(--bg)',
                  border: '1px solid var(--border)',
                  borderRadius: 4,
                  color: mode === m ? 'var(--bg)' : 'var(--text)',
                  cursor: 'pointer',
                }}
              >
                {m === 'roster' ? 'Roster' : 'Sandbox'}
              </button>
            ))}
          </div>
        </div>
        <nav style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
          {NAV_ITEMS.map(({ path, label }) => {
            const active = location.pathname === path;
            return (
              <Link
                key={path}
                to={path}
                className={active ? 'active' : ''}
                style={{
                  padding: '0.5rem 1rem',
                  color: active ? 'var(--accent)' : 'var(--text)',
                  textDecoration: 'none',
                  borderRadius: 4,
                  marginLeft: 8,
                  marginRight: 8,
                }}
              >
                {label}
              </Link>
            );
          })}
        </nav>
      </aside>
      <div style={{ flex: 1, minWidth: 0, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
        <header
          style={{
            height: 48,
            padding: '0 1rem',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            borderBottom: '1px solid var(--border)',
            background: 'var(--surface)',
          }}
        >
          <span style={{ fontSize: '1rem', fontWeight: 600, color: 'var(--text)' }}>
            Kobayashi
          </span>
          <ProfileSwitcher />
        </header>
        <main style={{ flex: 1, minWidth: 0, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
          {children}
        </main>
      </div>
    </div>
  );
}
