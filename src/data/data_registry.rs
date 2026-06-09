//! Startup-loaded data cache (DataRegistry) for the server.
//! Load once at startup, pass via Arc to handlers and optimizer to avoid reloading on every request.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::{Arc, Mutex};

use lru::LruCache;

use crate::data::forbidden_chaos::{
    load_forbidden_chaos, ForbiddenChaosList, DEFAULT_FORBIDDEN_CHAOS_PATH,
};
use crate::data::hostile::{
    load_hostile_index, load_hostile_record, HostileIndex, HostileRecord,
    DEFAULT_HOSTILES_INDEX_PATH,
};
use crate::data::hostile_loca::load_hostile_loca_display_names;
use crate::data::loader::normalize_lookup;
use crate::data::officer::{
    load_canonical_officers, normalize_officer_lookup_key, Officer, DEFAULT_CANONICAL_OFFICERS_PATH,
};
use crate::data::research::{
    load_research_catalog, ResearchCatalog, DEFAULT_RESEARCH_CATALOG_PATH,
};
use crate::data::ship::{
    load_extended_ship_index, load_extended_ship_record, CrewSlotUnlock, ExtendedShipIndex,
    ShipRecord, DEFAULT_SHIPS_EXTENDED_DIR,
};
use crate::data::support_buffs::{
    load_support_buff_catalog, SupportBuffCatalog, DEFAULT_SUPPORT_BUFFS_PATH,
};
use crate::lcars::LcarsOfficer;

use super::ship::ExtendedShipRecord;

/// Cached officer list and name-index for fast lookup. Built at startup.
#[derive(Debug, Clone)]
pub struct OfficerCache {
    /// All officers in canonical order.
    pub officers: Vec<Officer>,
    /// Normalized name -> officer (used by monte_carlo and crew resolution).
    pub by_name: HashMap<String, Officer>,
}

impl OfficerCache {
    fn from_officers(officers: Vec<Officer>) -> Self {
        let by_name = officers
            .iter()
            .map(|o| (normalize_officer_lookup_key(&o.name), o.clone()))
            .collect();
        OfficerCache { officers, by_name }
    }
}

/// Read-only registry of static game data loaded once at startup.
/// Profile and import roster are intentionally excluded (loaded at use time).
#[derive(Debug)]
pub struct DataRegistry {
    pub officers: OfficerCache,
    pub ship_index: Option<ExtendedShipIndex>,
    pub hostile_index: Option<HostileIndex>,
    /// `loca_id` → display name from data.stfc.space translation exports (for API / UI).
    pub hostile_loca_display: HashMap<u64, String>,
    /// LCARS officers — the sole ability source; `None` only if the data failed to load.
    /// Used by monte_carlo to resolve abilities.
    pub lcars_officers: Option<Vec<LcarsOfficer>>,
    /// Forbidden/chaos tech catalog for merging into profile with imported player tech.
    pub forbidden_chaos_catalog: Option<ForbiddenChaosList>,
    /// Research catalog for merging into profile with synced research levels.
    pub research_catalog: Option<ResearchCatalog>,
    /// Support buff definitions (alliance / ship toggles from API); optional if file missing.
    pub support_buffs_catalog: Option<Arc<SupportBuffCatalog>>,
    /// LRU cache for per-hostile record JSON files to avoid repeated disk I/O.
    hostile_record_cache: Mutex<LruCache<String, HostileRecord>>,
    /// LRU cache for per-ship extended record JSON files to avoid repeated disk I/O.
    ship_record_cache: Mutex<LruCache<String, ExtendedShipRecord>>,
}

impl DataRegistry {
    /// Load all static data from disk. Returns an Arc so it can be shared across handlers and threads.
    /// Officer load failure returns Err; missing ship/hostile indices are allowed (None).
    pub fn load() -> Result<Arc<DataRegistry>, std::io::Error> {
        let officers = load_canonical_officers(Path::new(DEFAULT_CANONICAL_OFFICERS_PATH))?;
        let officers = OfficerCache::from_officers(officers);

        let ship_index = Path::new(DEFAULT_SHIPS_EXTENDED_DIR)
            .is_dir()
            .then(|| load_extended_ship_index(Path::new(DEFAULT_SHIPS_EXTENDED_DIR)))
            .flatten();
        let hostile_index = load_hostile_index(DEFAULT_HOSTILES_INDEX_PATH);
        let hostile_loca_display =
            load_hostile_loca_display_names(Path::new(env!("CARGO_MANIFEST_DIR")));

        // LCARS is the sole officer ability source, built in-process from canonical (no committed
        // monolith); `None` only if the source data fails to load.
        let lcars_officers = crate::lcars::build_officer_model_default().ok();

        let forbidden_chaos_catalog = load_forbidden_chaos(DEFAULT_FORBIDDEN_CHAOS_PATH);
        let research_catalog = load_research_catalog(DEFAULT_RESEARCH_CATALOG_PATH);
        let support_buffs_catalog = load_support_buff_catalog(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_SUPPORT_BUFFS_PATH),
        )
        .or_else(|| load_support_buff_catalog(Path::new(DEFAULT_SUPPORT_BUFFS_PATH)));

        Ok(Arc::new(DataRegistry {
            officers,
            ship_index,
            hostile_index,
            hostile_loca_display,
            lcars_officers,
            forbidden_chaos_catalog,
            research_catalog,
            support_buffs_catalog,
            hostile_record_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(256).expect("256 > 0"),
            )),
            ship_record_cache: Mutex::new(LruCache::new(NonZeroUsize::new(128).expect("128 > 0"))),
        }))
    }

    /// LCARS officers (the sole ability source; `None` only on load failure). Monte Carlo builds
    /// by_id/name_to_id from this.
    pub fn lcars_officers(&self) -> Option<&[LcarsOfficer]> {
        self.lcars_officers.as_deref()
    }

    /// Forbidden/chaos tech catalog for merging with imported player tech into profile.
    pub fn forbidden_chaos_catalog(&self) -> Option<&ForbiddenChaosList> {
        self.forbidden_chaos_catalog.as_ref()
    }

    /// Research catalog for merging with synced research levels into profile.
    pub fn research_catalog(&self) -> Option<&ResearchCatalog> {
        self.research_catalog.as_ref()
    }

    /// Support buff catalog for API-selected alliance/ship buffs.
    pub fn support_buffs_catalog(&self) -> Option<&SupportBuffCatalog> {
        self.support_buffs_catalog.as_deref()
    }

    /// Officer list for API listing and crew generator pool building.
    pub fn officers(&self) -> &[Officer] {
        &self.officers.officers
    }

    /// Officer index by normalized name for monte_carlo and resolution.
    pub fn officer_index(&self) -> &HashMap<String, Officer> {
        &self.officers.by_name
    }

    /// Ship index for listing and resolution (from data/ships_extended).
    pub fn ship_index(&self) -> Option<&ExtendedShipIndex> {
        self.ship_index.as_ref()
    }

    /// Hostile index for listing and resolution.
    pub fn hostile_index(&self) -> Option<&HostileIndex> {
        self.hostile_index.as_ref()
    }

    /// Loca id → English name for hostile list labels (from bundled stfc.space translations).
    pub fn hostile_loca_display(&self) -> &HashMap<u64, String> {
        &self.hostile_loca_display
    }

    /// Load a hostile record from disk, caching in the LRU to avoid repeated I/O.
    fn load_hostile_record_cached(&self, data_dir: &Path, id: &str) -> Option<HostileRecord> {
        let mut cache = self.hostile_record_cache.lock().unwrap();
        if let Some(record) = cache.get(id) {
            return Some(record.clone());
        }
        let record = load_hostile_record(data_dir, id)?;
        cache.put(id.to_string(), record.clone());
        Some(record)
    }

    /// Load an extended ship record from disk, caching in the LRU to avoid repeated I/O.
    fn load_extended_ship_record_cached(
        &self,
        extended_dir: &Path,
        id: &str,
    ) -> Option<ExtendedShipRecord> {
        let mut cache = self.ship_record_cache.lock().unwrap();
        if let Some(record) = cache.get(id) {
            return Some(record.clone());
        }
        let record = load_extended_ship_record(extended_dir, id)?;
        cache.put(id.to_string(), record.clone());
        Some(record)
    }

    /// Resolve ship by id or name. Uses data/ships_extended with tier=1, level=1 when not specified.
    /// Uses cached ship index and per-record LRU cache to avoid re-reading index.json or record files.
    pub fn resolve_ship(&self, name_or_id: &str) -> Option<ShipRecord> {
        let index = self.ship_index.as_ref()?;
        let extended_dir = Path::new(DEFAULT_SHIPS_EXTENDED_DIR);
        let normalized = normalize_lookup(name_or_id);
        let id = index
            .ships
            .iter()
            .find(|e| {
                normalize_lookup(&e.id) == normalized
                    || normalize_lookup(&e.ship_name) == normalized
            })
            .map(|e| e.id.as_str())?;
        let extended = self.load_extended_ship_record_cached(extended_dir, id)?;
        extended.to_ship_record(Some(1), Some(1))
    }

    /// Resolve ship with optional tier and level (1-based). Uses data/ships_extended only.
    /// Uses cached ship index and per-record LRU cache to avoid re-reading index.json or record files.
    pub fn resolve_ship_with_tier_level(
        &self,
        name_or_id: &str,
        tier: Option<u32>,
        level: Option<u32>,
    ) -> Option<ShipRecord> {
        let index = self.ship_index.as_ref()?;
        let extended_dir = Path::new(DEFAULT_SHIPS_EXTENDED_DIR);
        let normalized = normalize_lookup(name_or_id);
        let id = index
            .ships
            .iter()
            .find(|e| {
                normalize_lookup(&e.id) == normalized
                    || normalize_lookup(&e.ship_name) == normalized
            })
            .map(|e| e.id.as_str())?;
        let extended = self.load_extended_ship_record_cached(extended_dir, id)?;
        extended.to_ship_record(tier.or(Some(1)), level.or(Some(1)))
    }

    /// Return available tier/level numbers plus below-decks unlock schedule for a ship.
    /// Uses cached ship index and per-record LRU cache to avoid re-reading index.json or record files.
    pub fn ship_tiers_levels_and_crew_slots(
        &self,
        name_or_id: &str,
    ) -> Option<(Vec<u32>, Vec<u32>, Vec<CrewSlotUnlock>)> {
        let index = self.ship_index.as_ref()?;
        let extended_dir = Path::new(DEFAULT_SHIPS_EXTENDED_DIR);
        let normalized = normalize_lookup(name_or_id);
        let id = index
            .ships
            .iter()
            .find(|e| {
                normalize_lookup(&e.id) == normalized
                    || normalize_lookup(&e.ship_name) == normalized
            })
            .map(|e| e.id.as_str())?;
        let extended = self.load_extended_ship_record_cached(extended_dir, id)?;
        let tiers: Vec<u32> = extended.tiers.iter().map(|t| t.tier).collect();
        let levels: Vec<u32> = extended.levels.iter().map(|l| l.level).collect();
        Some((tiers, levels, extended.crew_slots))
    }

    /// Resolve hostile by id or name/level using cached index and per-record LRU cache.
    pub fn resolve_hostile(&self, name_or_id: &str) -> Option<HostileRecord> {
        let index = self.hostile_index.as_ref()?;
        let data_dir = Path::new(DEFAULT_HOSTILES_INDEX_PATH).parent()?;

        let normalized = normalize_lookup(name_or_id);

        if let Some(entry) = index
            .hostiles
            .iter()
            .find(|e| normalize_lookup(&e.id) == normalized)
        {
            return self.load_hostile_record_cached(data_dir, &entry.id);
        }
        for entry in &index.hostiles {
            let name_level = format!("{}_{}", normalize_lookup(&entry.hostile_name), entry.level);
            if name_level == normalized {
                return self.load_hostile_record_cached(data_dir, &entry.id);
            }
            let name_space_level =
                format!("{} {}", normalize_lookup(&entry.hostile_name), entry.level);
            if normalize_lookup(&name_space_level) == normalized {
                return self.load_hostile_record_cached(data_dir, &entry.id);
            }
        }
        let by_name: Vec<_> = index
            .hostiles
            .iter()
            .filter(|e| normalize_lookup(&e.hostile_name) == normalized)
            .collect();
        if by_name.len() == 1 {
            return self.load_hostile_record_cached(data_dir, &by_name[0].id);
        }
        None
    }
}
