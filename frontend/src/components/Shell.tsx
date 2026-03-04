import { ReactNode } from 'react';
import { Link, useLocation } from 'react-router-dom';
import ProfileSwitcher from './ProfileSwitcher';

const NAV_ITEMS = [
  { path: '/', label: 'Workspace' },
  { path: '/results', label: 'Results Library' },
  { path: '/roster', label: 'Roster & Profile' },
  { path: '/data', label: 'Data & Mechanics' },
];

export default function Shell({ children }: { children: ReactNode }) {
  const location = useLocation();

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
