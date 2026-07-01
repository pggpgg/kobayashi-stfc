import { useEffect, useMemo, useState } from "react";
import RosterProfileAttributesCard from "../components/RosterProfileAttributesCard";
import RosterProfileBonusesTab from "../components/RosterProfileBonusesTab";
import RosterProfileBuildingsSummary from "../components/RosterProfileBuildingsSummary";
import RosterProfileImportTab from "../components/RosterProfileImportTab";
import RosterProfileModSyncBanner from "../components/RosterProfileModSyncBanner";
import RosterProfileResearchSummary from "../components/RosterProfileResearchSummary";
import RosterProfileTechSlot from "../components/RosterProfileTechSlot";
import { useProfile } from "../contexts/ProfileContext";
import type {
  BuildingCombatSummary,
  ForbiddenTechCatalogItem,
  ForbiddenTechImportedResponse,
  ImportReport,
  PlayerProfile,
  ResearchCombatSummary,
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
import { styles } from "../lib/rosterProfileStyles";

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

      <RosterProfileModSyncBanner
        modSyncUtc={modSyncUtc}
        modSyncError={modSyncError}
      />

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
          <RosterProfileAttributesCard activeProfile={activeProfile} />
          <RosterProfileBuildingsSummary
            buildingSummary={buildingSummary}
            buildingSummaryError={buildingSummaryError}
          />
          <RosterProfileResearchSummary
            researchSummary={researchSummary}
            researchSummaryError={researchSummaryError}
            researchScenarioShipId={researchScenarioShipId}
            onResearchScenarioShipIdChange={setResearchScenarioShipId}
            researchScenarioHostileId={researchScenarioHostileId}
            onResearchScenarioHostileIdChange={setResearchScenarioHostileId}
          />
          <RosterProfileTechSlot
            title="Forbidden tech slot"
            description={
              <>
                Equip one item in your ship&apos;s forbidden-tech slot (STFC).
                Options are restricted to rows in your mod-synced{" "}
                <code>forbidden_tech.imported.json</code> that match the catalog
                forbidden lane. Leave empty for no forbidden tech combat
                bonuses.
              </>
            }
            options={forbiddenEquipOptions}
            catalogByFid={catalogByFid}
            equippedFid={profile.equipped_forbidden_fid}
            onChange={(fid) => {
              setProfile((p) => ({ ...p, equipped_forbidden_fid: fid }));
              setProfileDirty(true);
            }}
            emptyMessage={
              <>
                No forbidden-tech rows in sync inventory with a catalog match.
                Push forbidden tech via the Community Mod or check{" "}
                <code>forbidden_tech.imported.json</code>.
              </>
            }
          />
          <RosterProfileTechSlot
            title="Chaos tech slot"
            description={
              <>
                Equip one chaos-tech item (separate in-game slot). Options are
                restricted to synced inventory rows that match catalog chaos
                entries.
              </>
            }
            options={chaosEquipOptions}
            catalogByFid={catalogByFid}
            equippedFid={profile.equipped_chaos_fid}
            onChange={(fid) => {
              setProfile((p) => ({ ...p, equipped_chaos_fid: fid }));
              setProfileDirty(true);
            }}
            emptyMessage="No chaos-tech rows in sync inventory with a catalog match."
          />
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
          {profileError && <div style={styles.errorNote}>{profileError}</div>}
        </section>
      )}

      {tab === "roster" && (
        <RosterProfileImportTab
          paste={paste}
          onPasteChange={setPaste}
          onImport={handleImport}
          importError={importError}
          importResult={importResult}
        />
      )}

      {tab === "bonuses" && (
        <RosterProfileBonusesTab
          bonuses={profile.bonuses}
          onBonusChange={setBonus}
          onSave={handleSaveProfile}
          profileDirty={profileDirty}
          profileError={profileError}
        />
      )}
    </div>
  );
}
