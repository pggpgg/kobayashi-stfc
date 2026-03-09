import { useState, useEffect } from 'react';
import { importRoster, fetchProfile, updateProfile, formatApiError } from '../lib/api';
import type { ImportReport, PlayerProfile } from '../lib/api';
import { useProfile } from '../contexts/ProfileContext';

type Tab = 'profile' | 'roster' | 'bonuses';

export default function RosterProfile() {
  const { activeProfileId, profiles } = useProfile();
  const [tab, setTab] = useState<Tab>('profile');
  const [paste, setPaste] = useState('');
  const [importResult, setImportResult] = useState<ImportReport | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [profile, setProfile] = useState<PlayerProfile>({ bonuses: {} });
  const [profileDirty, setProfileDirty] = useState(false);
  const [profileError, setProfileError] = useState<string | null>(null);

  useEffect(() => {
    let c = false;
    fetchProfile(activeProfileId).then((p) => {
      if (!c) setProfile(p);
    }).catch(() => {});
    return () => { c = true; };
  }, [activeProfileId]);

  const handleImport = async () => {
    setImportError(null);
    setImportResult(null);
    try {
      const report = await importRoster(paste, activeProfileId);
      setImportResult(report);
    } catch (e) {
      setImportError(formatApiError(e));
    }
  };

  const handleSaveProfile = async () => {
    setProfileError(null);
    try {
      await updateProfile(profile, activeProfileId);
      setProfileDirty(false);
    } catch (e) {
      setProfileError(formatApiError(e));
    }
  };

  const setBonus = (key: string, value: number) => {
    setProfile((p) => ({
      ...p,
      bonuses: { ...p.bonuses, [key]: value },
    }));
    setProfileDirty(true);
  };

  const activeProfile = profiles.find((p) => p.id === activeProfileId);

  return (
    <div>
      <h1 style={{ marginBottom: '1rem' }}>
        Roster & Profile
        {activeProfile && (
          <span style={{ marginLeft: 8, fontSize: '0.85rem', fontWeight: 400, color: 'var(--text-muted)' }}>
            ({activeProfile.name})
          </span>
        )}
      </h1>

      <div style={{ display: 'flex', gap: 8, marginBottom: '1rem' }}>
        <button
          type="button"
          onClick={() => setTab('profile')}
          style={{
            padding: '0.5rem 1rem',
            background: tab === 'profile' ? 'var(--accent)' : 'var(--surface)',
            border: '1px solid var(--border)',
            borderRadius: 6,
            color: tab === 'profile' ? 'var(--bg)' : 'var(--text)',
          }}
        >
          Profile
        </button>
        <button
          type="button"
          onClick={() => setTab('roster')}
          style={{
            padding: '0.5rem 1rem',
            background: tab === 'roster' ? 'var(--accent)' : 'var(--surface)',
            border: '1px solid var(--border)',
            borderRadius: 6,
            color: tab === 'roster' ? 'var(--bg)' : 'var(--text)',
          }}
        >
          Roster Import
        </button>
        <button
          type="button"
          onClick={() => setTab('bonuses')}
          style={{
            padding: '0.5rem 1rem',
            background: tab === 'bonuses' ? 'var(--accent)' : 'var(--surface)',
            border: '1px solid var(--border)',
            borderRadius: 6,
            color: tab === 'bonuses' ? 'var(--bg)' : 'var(--text)',
          }}
        >
          Player Bonuses
        </button>
      </div>

      {tab === 'profile' && activeProfile && (
        <section
          style={{
            padding: '1rem',
            background: 'var(--surface)',
            border: '1px solid var(--border)',
            borderRadius: 8,
          }}
        >
          <h2 style={{ margin: '0 0 1rem', fontSize: '1rem', fontWeight: 600 }}>
            Player profile attributes
          </h2>
          <dl style={{ margin: 0, display: 'grid', gap: '0.75rem 1rem', gridTemplateColumns: 'auto 1fr', maxWidth: 560 }}>
            <dt style={{ color: 'var(--text-muted)', fontWeight: 500 }}>Name</dt>
            <dd style={{ margin: 0 }}>{activeProfile.name}</dd>

            <dt style={{ color: 'var(--text-muted)', fontWeight: 500 }}>Profile ID</dt>
            <dd style={{ margin: 0 }}>
              <code
                style={{
                  padding: '0.2rem 0.4rem',
                  background: 'var(--bg)',
                  borderRadius: 4,
                  fontSize: '0.85rem',
                  fontFamily: 'monospace',
                }}
              >
                {activeProfile.id}
              </code>
            </dd>

            <dt style={{ color: 'var(--text-muted)', fontWeight: 500 }}>Sync token (UUID)</dt>
            <dd style={{ margin: 0, display: 'flex', alignItems: 'center', gap: 8 }}>
              <code
                style={{
                  padding: '0.35rem 0.5rem',
                  background: 'var(--bg)',
                  borderRadius: 4,
                  fontSize: '0.8rem',
                  fontFamily: 'monospace',
                  wordBreak: 'break-all',
                }}
              >
                {activeProfile.sync_token}
              </code>
              <button
                type="button"
                onClick={() => navigator.clipboard.writeText(activeProfile.sync_token)}
                style={{
                  padding: '0.35rem 0.6rem',
                  background: 'var(--accent)',
                  border: 'none',
                  borderRadius: 4,
                  color: 'var(--bg)',
                  fontSize: '0.8rem',
                  cursor: 'pointer',
                  flexShrink: 0,
                }}
              >
                Copy
              </button>
            </dd>
          </dl>
          <p style={{ marginTop: '1rem', marginBottom: '0.75rem', fontSize: '0.85rem', color: 'var(--text-muted)' }}>
            Add this to your <code>community_patch_settings.toml</code> to sync stfc-mod data to this profile:
          </p>
          <div
            style={{
              position: 'relative',
              background: 'var(--bg)',
              border: '1px solid var(--border)',
              borderRadius: 6,
              padding: '1rem',
              fontFamily: 'monospace',
              fontSize: '0.85rem',
              overflow: 'auto',
            }}
          >
            <pre style={{ margin: 0, paddingRight: 60, whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
{`[sync.targets.kobayashi-${activeProfile.id}]
url = "http://localhost:3000/api/sync/ingress"
token = "${activeProfile.sync_token}"`}
            </pre>
            <button
              type="button"
              onClick={() =>
                navigator.clipboard.writeText(
                  `[sync.targets.kobayashi-${activeProfile.id}]\nurl = "http://localhost:3000/api/sync/ingress"\ntoken = "${activeProfile.sync_token}"`,
                )
              }
              style={{
                position: 'absolute',
                top: 8,
                right: 8,
                padding: '0.35rem 0.6rem',
                background: 'var(--accent)',
                border: 'none',
                borderRadius: 4,
                color: 'var(--bg)',
                fontSize: '0.8rem',
                cursor: 'pointer',
              }}
            >
              Copy
            </button>
          </div>
        </section>
      )}

      {tab === 'roster' && (
        <section
          style={{
            padding: '1rem',
            background: 'var(--surface)',
            border: '1px solid var(--border)',
            borderRadius: 8,
          }}
        >
          <p style={{ margin: '0 0 0.5rem', fontSize: '0.9rem', color: 'var(--text-muted)' }}>
            Paste Spocks.club export (JSON) or CSV (name,tier,level per line).
          </p>
          <textarea
            value={paste}
            onChange={(e) => setPaste(e.target.value)}
            placeholder='Paste JSON or CSV here...'
            rows={12}
            style={{
              width: '100%',
              padding: 8,
              background: 'var(--bg)',
              border: '1px solid var(--border)',
              borderRadius: 6,
              color: 'var(--text)',
              fontFamily: 'monospace',
              fontSize: '0.85rem',
            }}
          />
          <button
            type="button"
            onClick={handleImport}
            style={{
              marginTop: 8,
              padding: '0.5rem 1rem',
              background: 'var(--accent)',
              border: 'none',
              borderRadius: 6,
              color: 'var(--bg)',
            }}
          >
            Import
          </button>
          {importError && (
            <div style={{ marginTop: 8, color: 'var(--error)' }}>{importError}</div>
          )}
          {importResult && (
            <div style={{ marginTop: 12, padding: 8, background: 'var(--bg)', borderRadius: 6 }}>
              <strong>Import result</strong>
              <div>Matched: {importResult.matched_records}, written: {importResult.roster_entries_written}</div>
              {importResult.unresolved && importResult.unresolved.length > 0 && (
                <div style={{ marginTop: 4, fontSize: '0.85rem' }}>
                  Unresolved: {importResult.unresolved.length}
                </div>
              )}
            </div>
          )}
        </section>
      )}

      {tab === 'bonuses' && (
        <section
          style={{
            padding: '1rem',
            background: 'var(--surface)',
            border: '1px solid var(--border)',
            borderRadius: 8,
          }}
        >
          <p style={{ margin: '0 0 0.75rem', fontSize: '0.9rem', color: 'var(--text-muted)' }}>
            Quick mode: enter effective bonus percentages (e.g. weapon, shield, mitigation).
          </p>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8, maxWidth: 400 }}>
            {['weapon', 'shield', 'mitigation', 'hull'].map((key) => (
              <label key={key} style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span style={{ width: 100 }}>{key} %</span>
                <input
                  type="number"
                  step={0.1}
                  value={profile.bonuses[key] ?? ''}
                  onChange={(e) => setBonus(key, Number(e.target.value) || 0)}
                  style={{
                    padding: '0.4rem',
                    background: 'var(--bg)',
                    border: '1px solid var(--border)',
                    borderRadius: 4,
                    color: 'var(--text)',
                  }}
                />
              </label>
            ))}
          </div>
          <button
            type="button"
            onClick={handleSaveProfile}
            disabled={!profileDirty}
            style={{
              marginTop: 12,
              padding: '0.5rem 1rem',
              background: profileDirty ? 'var(--accent)' : 'var(--border)',
              border: 'none',
              borderRadius: 6,
              color: 'var(--bg)',
            }}
          >
            Save profile
          </button>
          {profileError && (
            <div style={{ marginTop: 8, color: 'var(--error)' }}>{profileError}</div>
          )}
        </section>
      )}
    </div>
  );
}
