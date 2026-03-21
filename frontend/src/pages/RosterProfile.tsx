import { useState, useEffect } from 'react';
import {
  importRoster,
  fetchProfile,
  updateProfile,
  fetchForbiddenTech,
  fetchBuildingCombatSummary,
  fetchResearchCombatSummary,
  formatApiError,
} from '../lib/api';
import type {
  ImportReport,
  PlayerProfile,
  ForbiddenTechCatalogItem,
  BuildingCombatSummary,
  ResearchCombatSummary,
} from '../lib/api';
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
  const [forbiddenTechCatalog, setForbiddenTechCatalog] = useState<
    ForbiddenTechCatalogItem[]
  >([]);
  const [buildingSummary, setBuildingSummary] = useState<BuildingCombatSummary | null>(null);
  const [buildingSummaryError, setBuildingSummaryError] = useState<string | null>(null);
  const [researchSummary, setResearchSummary] = useState<ResearchCombatSummary | null>(null);
  const [researchSummaryError, setResearchSummaryError] = useState<string | null>(null);

  useEffect(() => {
    let c = false;
    fetchProfile(activeProfileId).then((p) => {
      if (!c) setProfile(p);
    }).catch(() => {});
    return () => { c = true; };
  }, [activeProfileId]);

  useEffect(() => {
    let c = false;
    fetchForbiddenTech().then((items) => {
      if (!c) setForbiddenTechCatalog(items);
    }).catch(() => {});
    return () => { c = true; };
  }, []);

  useEffect(() => {
    let c = false;
    setBuildingSummaryError(null);
    fetchBuildingCombatSummary(activeProfileId)
      .then((s) => {
        if (!c) setBuildingSummary(s);
      })
      .catch((e) => {
        if (!c) {
          setBuildingSummary(null);
          setBuildingSummaryError(formatApiError(e));
        }
      });
    return () => { c = true; };
  }, [activeProfileId]);

  useEffect(() => {
    let c = false;
    setResearchSummaryError(null);
    fetchResearchCombatSummary(activeProfileId)
      .then((s) => {
        if (!c) setResearchSummary(s);
      })
      .catch((e) => {
        if (!c) {
          setResearchSummary(null);
          setResearchSummaryError(formatApiError(e));
        }
      });
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

  type TechMode = 'synced' | 'none' | 'custom';
  const forbiddenTechMode: TechMode =
    profile.forbidden_tech_override === undefined ||
    profile.forbidden_tech_override === null
      ? 'synced'
      : profile.forbidden_tech_override.length === 0
        ? 'none'
        : 'custom';
  const setForbiddenTechMode = (mode: TechMode) => {
    setProfile((p) => ({
      ...p,
      forbidden_tech_override:
        mode === 'synced' ? undefined : mode === 'none' ? [] : p.forbidden_tech_override ?? [],
    }));
    setProfileDirty(true);
  };
  const setForbiddenTechOverride = (fids: number[]) => {
    setProfile((p) => ({ ...p, forbidden_tech_override: fids }));
    setProfileDirty(true);
  };
  const toggleForbiddenTechFid = (fid: number) => {
    const current = profile.forbidden_tech_override ?? [];
    if (current.includes(fid)) {
      setForbiddenTechOverride(current.filter((id) => id !== fid));
    } else {
      setForbiddenTechOverride([...current, fid]);
    }
  };

  const chaosTechMode: TechMode =
    profile.chaos_tech_override === undefined || profile.chaos_tech_override === null
      ? 'synced'
      : profile.chaos_tech_override.length === 0
        ? 'none'
        : 'custom';
  const setChaosTechMode = (mode: TechMode) => {
    setProfile((p) => ({
      ...p,
      chaos_tech_override:
        mode === 'synced' ? undefined : mode === 'none' ? [] : p.chaos_tech_override ?? [],
    }));
    setProfileDirty(true);
  };
  const setChaosTechOverride = (fids: number[]) => {
    setProfile((p) => ({ ...p, chaos_tech_override: fids }));
    setProfileDirty(true);
  };
  const toggleChaosTechFid = (fid: number) => {
    const current = profile.chaos_tech_override ?? [];
    if (current.includes(fid)) {
      setChaosTechOverride(current.filter((id) => id !== fid));
    } else {
      setChaosTechOverride([...current, fid]);
    }
  };

  const forbiddenTechItems = forbiddenTechCatalog.filter(
    (i) => i.fid != null && (i.tech_type === 'forbidden' || !i.tech_type),
  );
  const chaosTechItems = forbiddenTechCatalog.filter(
    (i) => i.fid != null && i.tech_type?.toLowerCase() === 'chaos',
  );

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

          <h3 style={{ margin: '1.5rem 0 0.5rem', fontSize: '0.95rem', fontWeight: 600 }}>
            Buildings (sync → combat)
          </h3>
          <p style={{ margin: '0 0 0.5rem', fontSize: '0.85rem', color: 'var(--text-muted)' }}>
            Starbase modules from sync (<code>buildings.imported.json</code>) and the combat stat bonuses they contribute in ship combat (same rules as simulate/optimize). Set ops level override under Player Bonuses if you need it without sync.
          </p>
          {buildingSummaryError && (
            <p style={{ margin: '0 0 0.5rem', fontSize: '0.85rem', color: 'var(--error, #c44)' }}>
              {buildingSummaryError}
            </p>
          )}
          {buildingSummary && (
            <div style={{ marginBottom: '1rem', fontSize: '0.85rem' }}>
              {buildingSummary.error && (
                <p style={{ margin: '0 0 0.5rem', color: 'var(--error, #c44)' }}>{buildingSummary.error}</p>
              )}
              <dl style={{ margin: '0 0 0.75rem', display: 'grid', gap: '0.35rem 1rem', gridTemplateColumns: 'auto 1fr', maxWidth: 520 }}>
                <dt style={{ color: 'var(--text-muted)' }}>Synced rows</dt>
                <dd style={{ margin: 0 }}>{buildingSummary.synced_building_count}</dd>
                <dt style={{ color: 'var(--text-muted)' }}>Ops (profile override)</dt>
                <dd style={{ margin: 0 }}>{buildingSummary.ops_level_profile_override ?? '—'}</dd>
                <dt style={{ color: 'var(--text-muted)' }}>Ops (inferred from sync)</dt>
                <dd style={{ margin: 0 }}>{buildingSummary.ops_level_inferred_from_sync ?? '—'}</dd>
                <dt style={{ color: 'var(--text-muted)' }}>Ops (effective)</dt>
                <dd style={{ margin: 0 }}>{buildingSummary.ops_level_effective ?? '—'}</dd>
              </dl>
              {buildingSummary.unmapped_bids.length > 0 && (
                <p style={{ margin: '0 0 0.5rem', color: 'var(--text-muted)' }}>
                  Unmapped game <code>bid</code> values (no catalog entry):{' '}
                  {buildingSummary.unmapped_bids.join(', ')}
                </p>
              )}
              {buildingSummary.combat_bonuses_from_buildings &&
                Object.keys(buildingSummary.combat_bonuses_from_buildings).length > 0 && (
                  <div style={{ marginBottom: '0.75rem' }}>
                    <div style={{ fontWeight: 600, marginBottom: 4 }}>Combat bonuses from buildings</div>
                    <ul style={{ margin: 0, paddingLeft: '1.25rem' }}>
                      {Object.entries(buildingSummary.combat_bonuses_from_buildings)
                        .sort(([a], [b]) => a.localeCompare(b))
                        .map(([k, v]) => (
                          <li key={k}>
                            <code>{k}</code>: {(v * 100).toFixed(2)}% additive
                          </li>
                        ))}
                    </ul>
                  </div>
                )}
              {buildingSummary.buildings.length > 0 && (
                <div style={{ overflowX: 'auto', maxHeight: 240, overflowY: 'auto', border: '1px solid var(--border)', borderRadius: 6 }}>
                  <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.8rem' }}>
                    <thead>
                      <tr style={{ textAlign: 'left', borderBottom: '1px solid var(--border)' }}>
                        <th style={{ padding: '6px 8px' }}>bid</th>
                        <th style={{ padding: '6px 8px' }}>Level</th>
                        <th style={{ padding: '6px 8px' }}>Building</th>
                        <th style={{ padding: '6px 8px' }}>Catalog</th>
                      </tr>
                    </thead>
                    <tbody>
                      {buildingSummary.buildings.map((row) => (
                        <tr key={row.bid} style={{ borderBottom: '1px solid var(--border)' }}>
                          <td style={{ padding: '6px 8px', fontFamily: 'monospace' }}>{row.bid}</td>
                          <td style={{ padding: '6px 8px' }}>{row.level}</td>
                          <td style={{ padding: '6px 8px' }}>
                            {row.building_name ?? row.kobayashi_building_id ?? '—'}
                          </td>
                          <td style={{ padding: '6px 8px' }}>{row.catalog_record_present ? 'yes' : 'no'}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}

          <h3 style={{ margin: '1.5rem 0 0.5rem', fontSize: '0.95rem', fontWeight: 600 }}>
            Research (sync → combat)
          </h3>
          <p style={{ margin: '0 0 0.5rem', fontSize: '0.85rem', color: 'var(--text-muted)' }}>
            Research levels from sync (<code>research.imported.json</code>) and the combat stat bonuses they contribute in ship combat (same rules as simulate/optimize).
          </p>
          {researchSummaryError && (
            <p style={{ margin: '0 0 0.5rem', fontSize: '0.85rem', color: 'var(--error, #c44)' }}>
              {researchSummaryError}
            </p>
          )}
          {researchSummary && (
            <div style={{ marginBottom: '1rem', fontSize: '0.85rem' }}>
              {researchSummary.error && (
                <p style={{ margin: '0 0 0.5rem', color: 'var(--error, #c44)' }}>{researchSummary.error}</p>
              )}
              <dl style={{ margin: '0 0 0.75rem', display: 'grid', gap: '0.35rem 1rem', gridTemplateColumns: 'auto 1fr', maxWidth: 520 }}>
                <dt style={{ color: 'var(--text-muted)' }}>Synced rows</dt>
                <dd style={{ margin: 0 }}>{researchSummary.synced_research_count}</dd>
              </dl>
              {researchSummary.unmapped_rids.length > 0 && (
                <p style={{ margin: '0 0 0.5rem', color: 'var(--text-muted)' }}>
                  Unmapped game <code>rid</code> values (no catalog entry):{' '}
                  {researchSummary.unmapped_rids.join(', ')}
                </p>
              )}
              {researchSummary.combat_bonuses_from_research &&
                Object.keys(researchSummary.combat_bonuses_from_research).length > 0 && (
                  <div style={{ marginBottom: '0.75rem' }}>
                    <div style={{ fontWeight: 600, marginBottom: 4 }}>Combat bonuses from research (total)</div>
                    <ul style={{ margin: 0, paddingLeft: '1.25rem' }}>
                      {Object.entries(researchSummary.combat_bonuses_from_research)
                        .sort(([a], [b]) => a.localeCompare(b))
                        .map(([k, v]) => (
                          <li key={k}>
                            <code>{k}</code>: {(v * 100).toFixed(2)}% additive
                          </li>
                        ))}
                    </ul>
                  </div>
                )}
              {researchSummary.research.length > 0 && (
                <div style={{ overflowX: 'auto', maxHeight: 280, overflowY: 'auto', border: '1px solid var(--border)', borderRadius: 6 }}>
                  <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.8rem' }}>
                    <thead>
                      <tr style={{ textAlign: 'left', borderBottom: '1px solid var(--border)' }}>
                        <th style={{ padding: '6px 8px' }}>rid</th>
                        <th style={{ padding: '6px 8px' }}>Level</th>
                        <th style={{ padding: '6px 8px' }}>Research</th>
                        <th style={{ padding: '6px 8px' }}>Catalog</th>
                        <th style={{ padding: '6px 8px' }}>Combat from row</th>
                      </tr>
                    </thead>
                    <tbody>
                      {researchSummary.research.map((row, idx) => (
                        <tr key={`${row.rid}-${idx}`} style={{ borderBottom: '1px solid var(--border)' }}>
                          <td style={{ padding: '6px 8px', fontFamily: 'monospace' }}>{row.rid}</td>
                          <td style={{ padding: '6px 8px' }}>{row.level}</td>
                          <td style={{ padding: '6px 8px' }}>
                            {row.research_name ?? '—'}
                          </td>
                          <td style={{ padding: '6px 8px' }}>{row.catalog_record_present ? 'yes' : 'no'}</td>
                          <td style={{ padding: '6px 8px', fontFamily: 'monospace', fontSize: '0.75rem' }}>
                            {row.combat_bonuses_from_row &&
                            Object.keys(row.combat_bonuses_from_row).length > 0
                              ? Object.entries(row.combat_bonuses_from_row)
                                  .sort(([a], [b]) => a.localeCompare(b))
                                  .map(([k, v]) => `${k} +${(v * 100).toFixed(2)}%`)
                                  .join('; ')
                              : '—'}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}

          <h3 style={{ margin: '1.5rem 0 0.5rem', fontSize: '0.95rem', fontWeight: 600 }}>
            Forbidden tech
          </h3>
          <p style={{ margin: '0 0 0.5rem', fontSize: '0.85rem', color: 'var(--text-muted)' }}>
            Choose which forbidden tech bonuses apply for simulate and optimize.
          </p>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8, maxWidth: 400 }}>
            <label style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <span style={{ width: 120 }}>Source</span>
              <select
                value={forbiddenTechMode}
                onChange={(e) => setForbiddenTechMode(e.target.value as TechMode)}
                style={{
                  padding: '0.4rem 0.6rem',
                  background: 'var(--bg)',
                  border: '1px solid var(--border)',
                  borderRadius: 6,
                  color: 'var(--text)',
                  flex: 1,
                }}
              >
                <option value="synced">Use synced</option>
                <option value="none">None</option>
                <option value="custom">Custom</option>
              </select>
            </label>
            {forbiddenTechMode === 'custom' && (
              <div style={{ marginTop: 4 }}>
                <span style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                  Select tech to apply (items with game ID only):
                </span>
                <div
                  style={{
                    marginTop: 6,
                    maxHeight: 200,
                    overflowY: 'auto',
                    padding: 8,
                    background: 'var(--bg)',
                    border: '1px solid var(--border)',
                    borderRadius: 6,
                    display: 'flex',
                    flexDirection: 'column',
                    gap: 4,
                  }}
                >
                  {forbiddenTechItems.map((item) => (
                    <label
                      key={item.fid ?? item.name}
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        gap: 8,
                        cursor: 'pointer',
                        fontSize: '0.9rem',
                      }}
                    >
                      <input
                        type="checkbox"
                        checked={(profile.forbidden_tech_override ?? []).includes(item.fid!)}
                        onChange={() => toggleForbiddenTechFid(item.fid!)}
                      />
                      {item.name}
                    </label>
                  ))}
                  {forbiddenTechItems.length === 0 && (
                    <span style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                      No forbidden tech items with game ID. Add fid in data/import/forbidden_chaos_tech.csv and re-run import.
                    </span>
                  )}
                </div>
              </div>
            )}
          </div>

          <h3 style={{ margin: '1.5rem 0 0.5rem', fontSize: '0.95rem', fontWeight: 600 }}>
            Chaos tech
          </h3>
          <p style={{ margin: '0 0 0.5rem', fontSize: '0.85rem', color: 'var(--text-muted)' }}>
            Choose which chaos tech bonuses apply for simulate and optimize.
          </p>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8, maxWidth: 400 }}>
            <label style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <span style={{ width: 120 }}>Source</span>
              <select
                value={chaosTechMode}
                onChange={(e) => setChaosTechMode(e.target.value as TechMode)}
                style={{
                  padding: '0.4rem 0.6rem',
                  background: 'var(--bg)',
                  border: '1px solid var(--border)',
                  borderRadius: 6,
                  color: 'var(--text)',
                  flex: 1,
                }}
              >
                <option value="synced">Use synced</option>
                <option value="none">None</option>
                <option value="custom">Custom</option>
              </select>
            </label>
            {chaosTechMode === 'custom' && (
              <div style={{ marginTop: 4 }}>
                <span style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                  Select tech to apply (items with game ID only):
                </span>
                <div
                  style={{
                    marginTop: 6,
                    maxHeight: 200,
                    overflowY: 'auto',
                    padding: 8,
                    background: 'var(--bg)',
                    border: '1px solid var(--border)',
                    borderRadius: 6,
                    display: 'flex',
                    flexDirection: 'column',
                    gap: 4,
                  }}
                >
                  {chaosTechItems.map((item) => (
                    <label
                      key={item.fid ?? item.name}
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        gap: 8,
                        cursor: 'pointer',
                        fontSize: '0.9rem',
                      }}
                    >
                      <input
                        type="checkbox"
                        checked={(profile.chaos_tech_override ?? []).includes(item.fid!)}
                        onChange={() => toggleChaosTechFid(item.fid!)}
                      />
                      {item.name}
                    </label>
                  ))}
                  {chaosTechItems.length === 0 && (
                    <span style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                      No chaos tech items with game ID. Add fid in data/import/forbidden_chaos_tech.csv and re-run import.
                    </span>
                  )}
                </div>
              </div>
            )}
          </div>
          <button
            type="button"
            onClick={handleSaveProfile}
            disabled={!profileDirty}
            style={{
              marginTop: 16,
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
