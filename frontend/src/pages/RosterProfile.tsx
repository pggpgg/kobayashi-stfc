import { useEffect, useMemo, useState } from "react";
import { useProfile } from "../contexts/ProfileContext";
import type {
  BuildingCombatSummary,
  ForbiddenTechCatalogItem,
  ForbiddenTechImportedResponse,
  ImportReport,
  PlayerProfile,
  ResearchCombatSummary,
  ResearchConditionalBonusLine,
} from "../lib/api";
import {
  fetchBuildingCombatSummary,
  fetchForbiddenTech,
  fetchForbiddenTechImported,
  fetchModSyncStatus,
  fetchProfile,
  fetchResearchCombatSummary,
  formatApiError,
  importRoster,
  updateProfile,
} from "../lib/api";

import {
  formatProfileCombatBonusDelta,
  formatProfileCombatBonusEntry,
  formatProfileCombatBonusListValue,
} from "../lib/profileCombatBonusDisplay";

/** Mod sync older than this is shown in red (stale). */
const MOD_SYNC_STALE_AFTER_MS = 24 * 60 * 60 * 1000;

function formatResearchBonusMap(m?: Record<string, number>): string {
  if (!m || Object.keys(m).length === 0) return "—";
  return Object.entries(m)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([k, v]) => formatProfileCombatBonusEntry(k, v))
    .join("; ");
}

function formatOwnerFactionResearch(
  m?: Record<string, Record<string, number>>,
): string {
  if (!m || Object.keys(m).length === 0) return "—";
  return Object.entries(m)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([faction, inner]) => {
      const stats = Object.entries(inner)
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([k, v]) => formatProfileCombatBonusEntry(k, v))
        .join(", ");
      return `${faction}: ${stats}`;
    })
    .join("; ");
}

function formatConditionalResearch(
  lines?: ResearchConditionalBonusLine[],
): string {
  if (!lines || lines.length === 0) return "—";
  return lines
    .map((line) => {
      const gate = line.condition_label ?? "conditional";
      const runtime = line.requires_runtime_state ? " (runtime)" : "";
      return `${line.stat} ${formatProfileCombatBonusDelta(line.stat, line.value)} [${gate}]${runtime}`;
    })
    .join("; ");
}

function researchCombatKindLabel(kind: string): string {
  switch (kind) {
    case "flat":
      return "flat";
    case "owner_faction":
      return "owner hull";
    case "conditional":
      return "conditional";
    case "mixed":
      return "mixed";
    case "support_buff_gated":
      return "support buff";
    case "non_combat":
      return "no combat";
    case "unmapped":
      return "unmapped";
    default:
      return kind;
  }
}

type Tab = "profile" | "roster" | "bonuses";

export default function RosterProfile() {
  const { activeProfileId, profiles } = useProfile();
  const [tab, setTab] = useState<Tab>("profile");
  const [paste, setPaste] = useState("");
  const [importResult, setImportResult] = useState<ImportReport | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [profile, setProfile] = useState<PlayerProfile>({ bonuses: {} });
  const [profileDirty, setProfileDirty] = useState(false);
  const [profileError, setProfileError] = useState<string | null>(null);
  const [forbiddenTechCatalog, setForbiddenTechCatalog] = useState<
    ForbiddenTechCatalogItem[]
  >([]);
  const [buildingSummary, setBuildingSummary] =
    useState<BuildingCombatSummary | null>(null);
  const [buildingSummaryError, setBuildingSummaryError] = useState<
    string | null
  >(null);
  const [researchSummary, setResearchSummary] =
    useState<ResearchCombatSummary | null>(null);
  const [researchSummaryError, setResearchSummaryError] = useState<
    string | null
  >(null);
  const [researchScenarioShipId, setResearchScenarioShipId] = useState("");
  const [researchScenarioHostileId, setResearchScenarioHostileId] =
    useState("");
  const [modSyncUtc, setModSyncUtc] = useState<string | null | undefined>(
    undefined,
  );
  const [modSyncError, setModSyncError] = useState<string | null>(null);
  const [forbiddenImported, setForbiddenImported] =
    useState<ForbiddenTechImportedResponse | null>(null);

  useEffect(() => {
    let c = false;
    fetchProfile(activeProfileId)
      .then((p) => {
        if (!c) setProfile(p);
      })
      .catch(() => {});
    return () => {
      c = true;
    };
  }, [activeProfileId]);

  useEffect(() => {
    let c = false;
    fetchForbiddenTech()
      .then((items) => {
        if (!c) setForbiddenTechCatalog(items);
      })
      .catch(() => {});
    return () => {
      c = true;
    };
  }, []);

  useEffect(() => {
    let c = false;
    fetchForbiddenTechImported(activeProfileId)
      .then((r) => {
        if (!c) setForbiddenImported(r);
      })
      .catch(() => {
        if (!c) setForbiddenImported(null);
      });
    return () => {
      c = true;
    };
  }, [activeProfileId]);

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
    return () => {
      c = true;
    };
  }, [activeProfileId]);

  useEffect(() => {
    let c = false;
    setResearchSummaryError(null);
    fetchResearchCombatSummary(activeProfileId, {
      shipId: researchScenarioShipId,
      hostileId: researchScenarioHostileId,
    })
      .then((s) => {
        if (!c) setResearchSummary(s);
      })
      .catch((e) => {
        if (!c) {
          setResearchSummary(null);
          setResearchSummaryError(formatApiError(e));
        }
      });
    return () => {
      c = true;
    };
  }, [activeProfileId, researchScenarioShipId, researchScenarioHostileId]);

  useEffect(() => {
    let cancelled = false;
    const load = () => {
      fetchModSyncStatus(activeProfileId)
        .then((s) => {
          if (!cancelled) {
            setModSyncUtc(s.last_mod_sync_utc);
            setModSyncError(null);
          }
        })
        .catch((e) => {
          if (!cancelled) {
            setModSyncUtc(null);
            setModSyncError(formatApiError(e));
          }
        });
    };
    load();
    const interval = window.setInterval(load, 30_000);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
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
      await updateProfile(
        {
          ...profile,
          forbidden_tech_override: null,
          chaos_tech_override: null,
        },
        activeProfileId,
      );
      setProfile((p) => ({
        ...p,
        forbidden_tech_override: null,
        chaos_tech_override: null,
      }));
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

  const catalogByFid = useMemo(() => {
    const m = new Map<number, ForbiddenTechCatalogItem>();
    for (const it of forbiddenTechCatalog) {
      if (it.fid != null) m.set(Number(it.fid), it);
    }
    return m;
  }, [forbiddenTechCatalog]);

  const isForbiddenLane = (fid: number) => {
    const c = catalogByFid.get(fid);
    if (!c) return false;
    const t = (c.tech_type ?? "").trim().toLowerCase();
    return t === "" || t === "forbidden";
  };

  const isChaosLane = (fid: number) =>
    catalogByFid.get(fid)?.tech_type?.trim().toLowerCase() === "chaos";

  const forbiddenEquipOptions =
    forbiddenImported?.forbidden_tech.filter((e) => isForbiddenLane(e.fid)) ??
    [];
  const chaosEquipOptions =
    forbiddenImported?.forbidden_tech.filter((e) => isChaosLane(e.fid)) ?? [];

  const activeProfile = profiles.find((p) => p.id === activeProfileId);

  const modSyncStatusBanner = (
    <div
      role="status"
      aria-live="polite"
      style={{
        marginBottom: "0.75rem",
        padding: "0.5rem 0.75rem",
        fontSize: "0.9rem",
        fontWeight: 500,
        background: "var(--surface)",
        border: "1px solid var(--border)",
        borderRadius: 8,
      }}
    >
      {modSyncError ? (
        <span style={{ color: "var(--error)" }}>{modSyncError}</span>
      ) : modSyncUtc === undefined ? (
        <span style={{ color: "var(--text-muted)" }}>
          Checking community mod sync…
        </span>
      ) : modSyncUtc === null ? (
        <span style={{ color: "var(--text-muted)" }}>
          No community mod sync recorded yet for this profile. Use the STFC
          Community Mod in-game to push roster, buildings, research, and other
          data to Kobayashi.
        </span>
      ) : (
        (() => {
          const t = Date.parse(modSyncUtc);
          const ok =
            !Number.isNaN(t) &&
            Date.now() - t >= 0 &&
            Date.now() - t < MOD_SYNC_STALE_AFTER_MS;
          const when = Number.isNaN(t)
            ? modSyncUtc
            : new Date(t).toLocaleString(undefined, {
                dateStyle: "short",
                timeStyle: "medium",
              });
          return (
            <span
              style={{
                color: ok ? "var(--success)" : "var(--error)",
              }}
            >
              Last community mod sync received: {when}
              {!Number.isNaN(t) && !ok ? " (stale)" : ""}
            </span>
          );
        })()
      )}
    </div>
  );

  return (
    <div>
      <h1 style={{ marginBottom: "0.5rem" }}>
        Roster & Profile
        {activeProfile && (
          <span
            style={{
              marginLeft: 8,
              fontSize: "0.85rem",
              fontWeight: 400,
              color: "var(--text-muted)",
            }}
          >
            ({activeProfile.name})
          </span>
        )}
      </h1>

      {modSyncStatusBanner}

      <div style={{ display: "flex", gap: 8, marginBottom: "1rem" }}>
        <button
          type="button"
          onClick={() => setTab("profile")}
          style={{
            padding: "0.5rem 1rem",
            background: tab === "profile" ? "var(--accent)" : "var(--surface)",
            border: "1px solid var(--border)",
            borderRadius: 6,
            color: tab === "profile" ? "var(--bg)" : "var(--text)",
          }}
        >
          Profile
        </button>
        <button
          type="button"
          onClick={() => setTab("roster")}
          style={{
            padding: "0.5rem 1rem",
            background: tab === "roster" ? "var(--accent)" : "var(--surface)",
            border: "1px solid var(--border)",
            borderRadius: 6,
            color: tab === "roster" ? "var(--bg)" : "var(--text)",
          }}
        >
          Roster Import
        </button>
        <button
          type="button"
          onClick={() => setTab("bonuses")}
          style={{
            padding: "0.5rem 1rem",
            background: tab === "bonuses" ? "var(--accent)" : "var(--surface)",
            border: "1px solid var(--border)",
            borderRadius: 6,
            color: tab === "bonuses" ? "var(--bg)" : "var(--text)",
          }}
        >
          Player Bonuses
        </button>
      </div>

      {tab === "profile" && activeProfile && (
        <section
          style={{
            padding: "1rem",
            background: "var(--surface)",
            border: "1px solid var(--border)",
            borderRadius: 8,
          }}
        >
          <h2 style={{ margin: "0 0 1rem", fontSize: "1rem", fontWeight: 600 }}>
            Player profile attributes
          </h2>
          <dl
            style={{
              margin: 0,
              display: "grid",
              gap: "0.75rem 1rem",
              gridTemplateColumns: "auto 1fr",
              maxWidth: 560,
            }}
          >
            <dt style={{ color: "var(--text-muted)", fontWeight: 500 }}>
              Name
            </dt>
            <dd style={{ margin: 0 }}>{activeProfile.name}</dd>

            <dt style={{ color: "var(--text-muted)", fontWeight: 500 }}>
              Profile ID
            </dt>
            <dd style={{ margin: 0 }}>
              <code
                style={{
                  padding: "0.2rem 0.4rem",
                  background: "var(--bg)",
                  borderRadius: 4,
                  fontSize: "0.85rem",
                  fontFamily: "monospace",
                }}
              >
                {activeProfile.id}
              </code>
            </dd>

            <dt style={{ color: "var(--text-muted)", fontWeight: 500 }}>
              Sync token (UUID)
            </dt>
            <dd
              style={{
                margin: 0,
                display: "flex",
                alignItems: "center",
                gap: 8,
              }}
            >
              <code
                style={{
                  padding: "0.35rem 0.5rem",
                  background: "var(--bg)",
                  borderRadius: 4,
                  fontSize: "0.8rem",
                  fontFamily: "monospace",
                  wordBreak: "break-all",
                }}
              >
                {activeProfile.sync_token}
              </code>
              <button
                type="button"
                onClick={() =>
                  navigator.clipboard.writeText(activeProfile.sync_token)
                }
                style={{
                  padding: "0.35rem 0.6rem",
                  background: "var(--accent)",
                  border: "none",
                  borderRadius: 4,
                  color: "var(--bg)",
                  fontSize: "0.8rem",
                  cursor: "pointer",
                  flexShrink: 0,
                }}
              >
                Copy
              </button>
            </dd>
          </dl>
          <p
            style={{
              marginTop: "1rem",
              marginBottom: "0.75rem",
              fontSize: "0.85rem",
              color: "var(--text-muted)",
            }}
          >
            Add this to your <code>community_patch_settings.toml</code> to sync
            stfc-mod data to this profile:
          </p>
          <div
            style={{
              position: "relative",
              background: "var(--bg)",
              border: "1px solid var(--border)",
              borderRadius: 6,
              padding: "1rem",
              fontFamily: "monospace",
              fontSize: "0.85rem",
              overflow: "auto",
            }}
          >
            <pre
              style={{
                margin: 0,
                paddingRight: 60,
                whiteSpace: "pre-wrap",
                wordBreak: "break-all",
              }}
            >
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
                position: "absolute",
                top: 8,
                right: 8,
                padding: "0.35rem 0.6rem",
                background: "var(--accent)",
                border: "none",
                borderRadius: 4,
                color: "var(--bg)",
                fontSize: "0.8rem",
                cursor: "pointer",
              }}
            >
              Copy
            </button>
          </div>

          <h3
            style={{
              margin: "1.5rem 0 0.5rem",
              fontSize: "0.95rem",
              fontWeight: 600,
            }}
          >
            Buildings (sync → combat)
          </h3>
          <p
            style={{
              margin: "0 0 0.5rem",
              fontSize: "0.85rem",
              color: "var(--text-muted)",
            }}
          >
            Starbase modules from sync (<code>buildings.imported.json</code>)
            and the combat stat bonuses they contribute in ship combat (same
            rules as simulate/optimize). Set ops level override under Player
            Bonuses if you need it without sync.
          </p>
          {buildingSummaryError && (
            <p
              style={{
                margin: "0 0 0.5rem",
                fontSize: "0.85rem",
                color: "var(--error, #c44)",
              }}
            >
              {buildingSummaryError}
            </p>
          )}
          {buildingSummary && (
            <div style={{ marginBottom: "1rem", fontSize: "0.85rem" }}>
              {buildingSummary.error && (
                <p
                  style={{ margin: "0 0 0.5rem", color: "var(--error, #c44)" }}
                >
                  {buildingSummary.error}
                </p>
              )}
              <dl
                style={{
                  margin: "0 0 0.75rem",
                  display: "grid",
                  gap: "0.35rem 1rem",
                  gridTemplateColumns: "auto 1fr",
                  maxWidth: 520,
                }}
              >
                <dt style={{ color: "var(--text-muted)" }}>Synced rows</dt>
                <dd style={{ margin: 0 }}>
                  {buildingSummary.synced_building_count}
                </dd>
                <dt style={{ color: "var(--text-muted)" }}>
                  Ops (profile override)
                </dt>
                <dd style={{ margin: 0 }}>
                  {buildingSummary.ops_level_profile_override ?? "—"}
                </dd>
                <dt style={{ color: "var(--text-muted)" }}>
                  Ops (inferred from sync)
                </dt>
                <dd style={{ margin: 0 }}>
                  {buildingSummary.ops_level_inferred_from_sync ?? "—"}
                </dd>
                <dt style={{ color: "var(--text-muted)" }}>Ops (effective)</dt>
                <dd style={{ margin: 0 }}>
                  {buildingSummary.ops_level_effective ?? "—"}
                </dd>
              </dl>
              {buildingSummary.unmapped_bids.length > 0 && (
                <p style={{ margin: "0 0 0.5rem", color: "var(--text-muted)" }}>
                  Unmapped game <code>bid</code> values (no catalog entry):{" "}
                  {buildingSummary.unmapped_bids.join(", ")}
                </p>
              )}
              {buildingSummary.combat_bonuses_from_buildings &&
                Object.keys(buildingSummary.combat_bonuses_from_buildings)
                  .length > 0 && (
                  <div style={{ marginBottom: "0.75rem" }}>
                    <div style={{ fontWeight: 600, marginBottom: 4 }}>
                      Combat bonuses from buildings
                    </div>
                    <ul style={{ margin: 0, paddingLeft: "1.25rem" }}>
                      {Object.entries(
                        buildingSummary.combat_bonuses_from_buildings,
                      )
                        .sort(([a], [b]) => a.localeCompare(b))
                        .map(([k, v]) => (
                          <li key={k}>
                            <code>{k}</code>:{" "}
                            {formatProfileCombatBonusListValue(k, v)}
                          </li>
                        ))}
                    </ul>
                  </div>
                )}
              {buildingSummary.buildings.length > 0 && (
                <div
                  style={{
                    overflowX: "auto",
                    maxHeight: 240,
                    overflowY: "auto",
                    border: "1px solid var(--border)",
                    borderRadius: 6,
                  }}
                >
                  <table
                    style={{
                      width: "100%",
                      borderCollapse: "collapse",
                      fontSize: "0.8rem",
                    }}
                  >
                    <thead>
                      <tr
                        style={{
                          textAlign: "left",
                          borderBottom: "1px solid var(--border)",
                        }}
                      >
                        <th style={{ padding: "6px 8px" }}>bid</th>
                        <th style={{ padding: "6px 8px" }}>Level</th>
                        <th style={{ padding: "6px 8px" }}>Building</th>
                        <th style={{ padding: "6px 8px" }}>Catalog</th>
                      </tr>
                    </thead>
                    <tbody>
                      {buildingSummary.buildings.map((row) => (
                        <tr
                          key={row.bid}
                          style={{ borderBottom: "1px solid var(--border)" }}
                        >
                          <td
                            style={{
                              padding: "6px 8px",
                              fontFamily: "monospace",
                            }}
                          >
                            {row.bid}
                          </td>
                          <td style={{ padding: "6px 8px" }}>{row.level}</td>
                          <td style={{ padding: "6px 8px" }}>
                            {row.building_name ??
                              row.kobayashi_building_id ??
                              "—"}
                          </td>
                          <td style={{ padding: "6px 8px" }}>
                            {row.catalog_record_present ? "yes" : "no"}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}

          <h3
            style={{
              margin: "1.5rem 0 0.5rem",
              fontSize: "0.95rem",
              fontWeight: 600,
            }}
          >
            Research (sync → combat)
          </h3>
          <p
            style={{
              margin: "0 0 0.5rem",
              fontSize: "0.85rem",
              color: "var(--text-muted)",
            }}
          >
            Sync stores every <code>rid</code> + level in{" "}
            <code>research.imported.json</code> (full savepoint). The maintainer
            catalog (<code>research_catalog.json</code>) only decides which rows
            add ship combat stats in simulate/optimize; unmapped rows stay on
            disk for later mapping.
          </p>
          {researchSummaryError && (
            <p
              style={{
                margin: "0 0 0.5rem",
                fontSize: "0.85rem",
                color: "var(--error, #c44)",
              }}
            >
              {researchSummaryError}
            </p>
          )}
          {researchSummary && (
            <div style={{ marginBottom: "1rem", fontSize: "0.85rem" }}>
              {researchSummary.error && (
                <p
                  style={{ margin: "0 0 0.5rem", color: "var(--error, #c44)" }}
                >
                  {researchSummary.error}
                </p>
              )}
              <dl
                style={{
                  margin: "0 0 0.75rem",
                  display: "grid",
                  gap: "0.35rem 1rem",
                  gridTemplateColumns: "auto 1fr",
                  maxWidth: 520,
                }}
              >
                <dt style={{ color: "var(--text-muted)" }}>Synced rows</dt>
                <dd style={{ margin: 0 }}>
                  {researchSummary.synced_research_count}
                </dd>
              </dl>
              <div
                style={{
                  marginBottom: "0.75rem",
                  display: "grid",
                  gap: "0.5rem",
                  maxWidth: 520,
                }}
              >
                <div style={{ fontWeight: 600, fontSize: "0.85rem" }}>
                  Scenario lens (optional)
                </div>
                <label style={{ display: "grid", gap: 4, fontSize: "0.8rem" }}>
                  <span style={{ color: "var(--text-muted)" }}>ship_id</span>
                  <input
                    type="text"
                    value={researchScenarioShipId}
                    onChange={(e) => setResearchScenarioShipId(e.target.value)}
                    placeholder="e.g. uss_voyager"
                    style={{ padding: "6px 8px", fontSize: "0.85rem" }}
                  />
                </label>
                <label style={{ display: "grid", gap: 4, fontSize: "0.8rem" }}>
                  <span style={{ color: "var(--text-muted)" }}>hostile_id</span>
                  <input
                    type="text"
                    value={researchScenarioHostileId}
                    onChange={(e) =>
                      setResearchScenarioHostileId(e.target.value)
                    }
                    placeholder="e.g. hostile id or name"
                    style={{ padding: "6px 8px", fontSize: "0.85rem" }}
                  />
                </label>
                {researchSummary.scenario_context && (
                  <p
                    style={{
                      margin: 0,
                      color: "var(--text-muted)",
                      fontSize: "0.8rem",
                    }}
                  >
                    Effective for {researchSummary.scenario_context.ship_id} vs{" "}
                    {researchSummary.scenario_context.hostile_id}
                    {researchSummary.scenario_context.ship_faction
                      ? ` (${researchSummary.scenario_context.ship_faction} hull vs ${researchSummary.scenario_context.defender_faction} ${researchSummary.scenario_context.defender_ship_class})`
                      : ""}
                  </p>
                )}
              </div>
              {researchSummary.unmapped_research &&
                researchSummary.unmapped_research.length > 0 && (
                  <div style={{ marginBottom: "0.75rem" }}>
                    <div style={{ fontWeight: 600, marginBottom: 4 }}>
                      Unmapped research (by level)
                    </div>
                    <ul
                      style={{
                        margin: 0,
                        paddingLeft: "1.25rem",
                        fontSize: "0.8rem",
                      }}
                    >
                      {researchSummary.unmapped_research.map((row) => (
                        <li key={row.rid}>
                          <code>{row.rid}</code> @ level {row.level}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
              {researchSummary.combat_bonuses_from_research &&
                Object.keys(researchSummary.combat_bonuses_from_research)
                  .length > 0 && (
                  <div style={{ marginBottom: "0.75rem" }}>
                    <div style={{ fontWeight: 600, marginBottom: 4 }}>
                      Flat combat bonuses (global)
                    </div>
                    <ul style={{ margin: 0, paddingLeft: "1.25rem" }}>
                      {Object.entries(
                        researchSummary.combat_bonuses_from_research,
                      )
                        .sort(([a], [b]) => a.localeCompare(b))
                        .map(([k, v]) => (
                          <li key={k}>
                            <code>{k}</code>:{" "}
                            {formatProfileCombatBonusListValue(k, v)}
                          </li>
                        ))}
                    </ul>
                  </div>
                )}
              {researchSummary.combat_owner_faction_bonuses_from_research &&
                Object.keys(
                  researchSummary.combat_owner_faction_bonuses_from_research,
                ).length > 0 && (
                  <div style={{ marginBottom: "0.75rem" }}>
                    <div style={{ fontWeight: 600, marginBottom: 4 }}>
                      Owner-hull faction bonuses
                    </div>
                    <ul style={{ margin: 0, paddingLeft: "1.25rem" }}>
                      {Object.entries(
                        researchSummary.combat_owner_faction_bonuses_from_research,
                      )
                        .sort(([a], [b]) => a.localeCompare(b))
                        .map(([faction, inner]) => (
                          <li key={faction}>
                            <code>{faction}</code>:{" "}
                            {Object.entries(inner)
                              .sort(([a], [b]) => a.localeCompare(b))
                              .map(([k, v]) =>
                                formatProfileCombatBonusEntry(k, v),
                              )
                              .join(", ")}
                          </li>
                        ))}
                    </ul>
                  </div>
                )}
              {researchSummary.combat_conditional_bonuses_from_research &&
                researchSummary.combat_conditional_bonuses_from_research
                  .length > 0 && (
                  <div style={{ marginBottom: "0.75rem" }}>
                    <div style={{ fontWeight: 600, marginBottom: 4 }}>
                      Conditional bonuses (attack-phase seats)
                    </div>
                    <ul
                      style={{
                        margin: 0,
                        paddingLeft: "1.25rem",
                        fontSize: "0.8rem",
                      }}
                    >
                      {researchSummary.combat_conditional_bonuses_from_research.map(
                        (line, idx) => (
                          <li key={`${line.stat}-${idx}`}>
                            <code>{line.stat}</code>{" "}
                            {formatProfileCombatBonusDelta(
                              line.stat,
                              line.value,
                            )}{" "}
                            — {line.condition_label ?? "conditional"}
                            {line.requires_runtime_state
                              ? " (needs morale/burning/HB in fight)"
                              : ""}
                          </li>
                        ),
                      )}
                    </ul>
                  </div>
                )}
              {researchSummary.combat_bonuses_scenario_effective &&
                Object.keys(researchSummary.combat_bonuses_scenario_effective)
                  .length > 0 && (
                  <div style={{ marginBottom: "0.75rem" }}>
                    <div style={{ fontWeight: 600, marginBottom: 4 }}>
                      Scenario-effective flat totals
                    </div>
                    <ul style={{ margin: 0, paddingLeft: "1.25rem" }}>
                      {Object.entries(
                        researchSummary.combat_bonuses_scenario_effective,
                      )
                        .sort(([a], [b]) => a.localeCompare(b))
                        .map(([k, v]) => (
                          <li key={k}>
                            <code>{k}</code>:{" "}
                            {formatProfileCombatBonusListValue(k, v)}
                          </li>
                        ))}
                    </ul>
                  </div>
                )}
              {researchSummary.combat_conditional_scenario_active &&
                researchSummary.combat_conditional_scenario_active.length >
                  0 && (
                  <div style={{ marginBottom: "0.75rem" }}>
                    <div style={{ fontWeight: 600, marginBottom: 4 }}>
                      Conditional active for scenario (static gates)
                    </div>
                    <ul
                      style={{
                        margin: 0,
                        paddingLeft: "1.25rem",
                        fontSize: "0.8rem",
                      }}
                    >
                      {researchSummary.combat_conditional_scenario_active.map(
                        (line, idx) => (
                          <li key={`sc-${line.stat}-${idx}`}>
                            <code>{line.stat}</code>{" "}
                            {formatProfileCombatBonusDelta(
                              line.stat,
                              line.value,
                            )}{" "}
                            — {line.condition_label ?? "conditional"}
                            {line.requires_runtime_state
                              ? " (runtime gate)"
                              : ""}
                          </li>
                        ),
                      )}
                    </ul>
                  </div>
                )}
              {researchSummary.research.length > 0 && (
                <div
                  style={{
                    overflowX: "auto",
                    maxHeight: 280,
                    overflowY: "auto",
                    border: "1px solid var(--border)",
                    borderRadius: 6,
                  }}
                >
                  <table
                    style={{
                      width: "100%",
                      borderCollapse: "collapse",
                      fontSize: "0.8rem",
                    }}
                  >
                    <thead>
                      <tr
                        style={{
                          textAlign: "left",
                          borderBottom: "1px solid var(--border)",
                        }}
                      >
                        <th style={{ padding: "6px 8px" }}>rid</th>
                        <th style={{ padding: "6px 8px" }}>Level</th>
                        <th style={{ padding: "6px 8px" }}>Research</th>
                        <th style={{ padding: "6px 8px" }}>Kind</th>
                        <th style={{ padding: "6px 8px" }}>Flat</th>
                        <th style={{ padding: "6px 8px" }}>Owner hull</th>
                        <th style={{ padding: "6px 8px" }}>Conditional</th>
                      </tr>
                    </thead>
                    <tbody>
                      {researchSummary.research.map((row, idx) => (
                        <tr
                          key={`${row.rid}-${idx}`}
                          style={{ borderBottom: "1px solid var(--border)" }}
                        >
                          <td
                            style={{
                              padding: "6px 8px",
                              fontFamily: "monospace",
                            }}
                          >
                            {row.rid}
                          </td>
                          <td style={{ padding: "6px 8px" }}>{row.level}</td>
                          <td style={{ padding: "6px 8px" }}>
                            {row.research_name ?? "—"}
                          </td>
                          <td style={{ padding: "6px 8px" }}>
                            {researchCombatKindLabel(row.combat_kind)}
                          </td>
                          <td
                            style={{
                              padding: "6px 8px",
                              fontFamily: "monospace",
                              fontSize: "0.75rem",
                            }}
                          >
                            {formatResearchBonusMap(
                              row.combat_bonuses_from_row,
                            )}
                          </td>
                          <td
                            style={{
                              padding: "6px 8px",
                              fontFamily: "monospace",
                              fontSize: "0.75rem",
                            }}
                          >
                            {formatOwnerFactionResearch(
                              row.combat_owner_faction_bonuses_from_row,
                            )}
                          </td>
                          <td
                            style={{
                              padding: "6px 8px",
                              fontFamily: "monospace",
                              fontSize: "0.75rem",
                            }}
                          >
                            {formatConditionalResearch(
                              row.combat_conditional_bonuses_from_row,
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}

          <h3
            style={{
              margin: "1.5rem 0 0.5rem",
              fontSize: "0.95rem",
              fontWeight: 600,
            }}
          >
            Forbidden tech slot
          </h3>
          <p
            style={{
              margin: "0 0 0.5rem",
              fontSize: "0.85rem",
              color: "var(--text-muted)",
            }}
          >
            Equip one item in your ship&apos;s forbidden-tech slot (STFC).
            Options are restricted to rows in your mod-synced{" "}
            <code>forbidden_tech.imported.json</code> that match the catalog
            forbidden lane. Leave empty for no forbidden tech combat bonuses.
          </p>
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 8,
              maxWidth: 420,
            }}
          >
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span style={{ width: 140 }}>Equipped</span>
              <select
                value={
                  profile.equipped_forbidden_fid != null
                    ? String(profile.equipped_forbidden_fid)
                    : ""
                }
                onChange={(e) => {
                  const v = e.target.value;
                  setProfile((p) => ({
                    ...p,
                    equipped_forbidden_fid:
                      v === "" ? null : Number.parseInt(v, 10),
                  }));
                  setProfileDirty(true);
                }}
                style={{
                  padding: "0.4rem 0.6rem",
                  background: "var(--bg)",
                  border: "1px solid var(--border)",
                  borderRadius: 6,
                  color: "var(--text)",
                  flex: 1,
                }}
              >
                <option value="">(empty)</option>
                {forbiddenEquipOptions.map((e) => (
                  <option key={e.fid} value={e.fid}>
                    {(catalogByFid.get(e.fid)?.name ?? `fid ${e.fid}`) +
                      ` — T${e.tier} L${e.level}`}
                  </option>
                ))}
              </select>
            </label>
            {forbiddenEquipOptions.length === 0 && (
              <span
                style={{
                  fontSize: "0.85rem",
                  color: "var(--text-muted)",
                }}
              >
                No forbidden-tech rows in sync inventory with a catalog match.
                Push forbidden tech via the Community Mod or check{" "}
                <code>forbidden_tech.imported.json</code>.
              </span>
            )}
          </div>

          <h3
            style={{
              margin: "1.5rem 0 0.5rem",
              fontSize: "0.95rem",
              fontWeight: 600,
            }}
          >
            Chaos tech slot
          </h3>
          <p
            style={{
              margin: "0 0 0.5rem",
              fontSize: "0.85rem",
              color: "var(--text-muted)",
            }}
          >
            Equip one chaos-tech item (separate in-game slot). Options are
            restricted to synced inventory rows that match catalog chaos
            entries.
          </p>
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 8,
              maxWidth: 420,
            }}
          >
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span style={{ width: 140 }}>Equipped</span>
              <select
                value={
                  profile.equipped_chaos_fid != null
                    ? String(profile.equipped_chaos_fid)
                    : ""
                }
                onChange={(e) => {
                  const v = e.target.value;
                  setProfile((p) => ({
                    ...p,
                    equipped_chaos_fid:
                      v === "" ? null : Number.parseInt(v, 10),
                  }));
                  setProfileDirty(true);
                }}
                style={{
                  padding: "0.4rem 0.6rem",
                  background: "var(--bg)",
                  border: "1px solid var(--border)",
                  borderRadius: 6,
                  color: "var(--text)",
                  flex: 1,
                }}
              >
                <option value="">(empty)</option>
                {chaosEquipOptions.map((e) => (
                  <option key={e.fid} value={e.fid}>
                    {(catalogByFid.get(e.fid)?.name ?? `fid ${e.fid}`) +
                      ` — T${e.tier} L${e.level}`}
                  </option>
                ))}
              </select>
            </label>
            {chaosEquipOptions.length === 0 && (
              <span
                style={{
                  fontSize: "0.85rem",
                  color: "var(--text-muted)",
                }}
              >
                No chaos-tech rows in sync inventory with a catalog match.
              </span>
            )}
          </div>
          <button
            type="button"
            onClick={handleSaveProfile}
            disabled={!profileDirty}
            style={{
              marginTop: 16,
              padding: "0.5rem 1rem",
              background: profileDirty ? "var(--accent)" : "var(--border)",
              border: "none",
              borderRadius: 6,
              color: "var(--bg)",
            }}
          >
            Save profile
          </button>
          {profileError && (
            <div style={{ marginTop: 8, color: "var(--error)" }}>
              {profileError}
            </div>
          )}
        </section>
      )}

      {tab === "roster" && (
        <section
          style={{
            padding: "1rem",
            background: "var(--surface)",
            border: "1px solid var(--border)",
            borderRadius: 8,
          }}
        >
          <p
            style={{
              margin: "0 0 0.5rem",
              fontSize: "0.9rem",
              color: "var(--text-muted)",
            }}
          >
            Paste Spocks.club export (JSON) or CSV (name,tier,level per line).
          </p>
          <textarea
            value={paste}
            onChange={(e) => setPaste(e.target.value)}
            placeholder="Paste JSON or CSV here..."
            rows={12}
            style={{
              width: "100%",
              padding: 8,
              background: "var(--bg)",
              border: "1px solid var(--border)",
              borderRadius: 6,
              color: "var(--text)",
              fontFamily: "monospace",
              fontSize: "0.85rem",
            }}
          />
          <button
            type="button"
            onClick={handleImport}
            style={{
              marginTop: 8,
              padding: "0.5rem 1rem",
              background: "var(--accent)",
              border: "none",
              borderRadius: 6,
              color: "var(--bg)",
            }}
          >
            Import
          </button>
          {importError && (
            <div style={{ marginTop: 8, color: "var(--error)" }}>
              {importError}
            </div>
          )}
          {importResult && (
            <div
              style={{
                marginTop: 12,
                padding: 8,
                background: "var(--bg)",
                borderRadius: 6,
              }}
            >
              <strong>Import result</strong>
              <div>
                Matched: {importResult.matched_records}, written:{" "}
                {importResult.roster_entries_written}
                {importResult.critical_failures != null &&
                  importResult.critical_failures > 0 && (
                    <span style={{ color: "var(--error)", marginLeft: 8 }}>
                      ({importResult.critical_failures} blocking issue
                      {importResult.critical_failures === 1 ? "" : "s"})
                    </span>
                  )}
              </div>
              {importResult.diagnostics &&
                importResult.diagnostics.length > 0 && (
                  <div style={{ marginTop: 8, fontSize: "0.85rem" }}>
                    <strong style={{ color: "var(--text-muted)" }}>
                      Warnings (tier / level)
                    </strong>
                    <ul style={{ margin: "4px 0 0", paddingLeft: 18 }}>
                      {importResult.diagnostics.map((d, i) => (
                        <li key={i} style={{ marginBottom: 4 }}>
                          Row {d.record_index + 1} ({d.input_name}): {d.message}
                          {d.hint && (
                            <div
                              style={{
                                color: "var(--text-muted)",
                                marginTop: 2,
                              }}
                            >
                              {d.hint}
                            </div>
                          )}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
              {importResult.unresolved &&
                importResult.unresolved.length > 0 && (
                  <div style={{ marginTop: 8, fontSize: "0.85rem" }}>
                    <strong style={{ color: "var(--error)" }}>
                      Unresolved names
                    </strong>
                    <ul style={{ margin: "4px 0 0", paddingLeft: 18 }}>
                      {importResult.unresolved.map((u, i) => (
                        <li key={i} style={{ marginBottom: 6 }}>
                          Row {u.record_index + 1}: &quot;{u.input_name}&quot; —{" "}
                          {u.reason}
                          {u.suggested_matches &&
                            u.suggested_matches.length > 0 && (
                              <div style={{ marginTop: 2 }}>
                                Similar canonical names:{" "}
                                {u.suggested_matches.join(", ")}
                              </div>
                            )}
                          {u.hint && (
                            <div
                              style={{
                                color: "var(--text-muted)",
                                marginTop: 2,
                              }}
                            >
                              {u.hint}
                            </div>
                          )}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
            </div>
          )}
        </section>
      )}

      {tab === "bonuses" && (
        <section
          style={{
            padding: "1rem",
            background: "var(--surface)",
            border: "1px solid var(--border)",
            borderRadius: 8,
          }}
        >
          <p
            style={{
              margin: "0 0 0.75rem",
              fontSize: "0.9rem",
              color: "var(--text-muted)",
            }}
          >
            Quick mode: enter effective bonus percentages (e.g. weapon, shield,
            mitigation).
          </p>
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 8,
              maxWidth: 400,
            }}
          >
            {["weapon", "shield", "mitigation", "hull"].map((key) => (
              <label
                key={key}
                style={{ display: "flex", alignItems: "center", gap: 8 }}
              >
                <span style={{ width: 100 }}>{key} %</span>
                <input
                  type="number"
                  step={0.1}
                  value={profile.bonuses[key] ?? ""}
                  onChange={(e) => setBonus(key, Number(e.target.value) || 0)}
                  style={{
                    padding: "0.4rem",
                    background: "var(--bg)",
                    border: "1px solid var(--border)",
                    borderRadius: 4,
                    color: "var(--text)",
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
              padding: "0.5rem 1rem",
              background: profileDirty ? "var(--accent)" : "var(--border)",
              border: "none",
              borderRadius: 6,
              color: "var(--bg)",
            }}
          >
            Save profile
          </button>
          {profileError && (
            <div style={{ marginTop: 8, color: "var(--error)" }}>
              {profileError}
            </div>
          )}
        </section>
      )}
    </div>
  );
}
