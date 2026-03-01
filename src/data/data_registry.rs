//! Startup-loaded data cache (DataRegistry) for the server.
//! Load once at startup, pass via Arc to handlers and optimizer to avoid reloading on every request.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::data::forbidden_chaos::{
    load_forbidden_chaos, ForbiddenChaosList, DEFAULT_FORBIDDEN_CHAOS_PATH,
};
use crate::data::hostile::{load_hostile_index, HostileRecord, HostileIndex, DEFAULT_HOSTILES_INDEX_PATH};
use crate::data::loader::{resolve_hostile_with_index, resolve_ship_with_index};
use crate::data::officer::{load_canonical_officers, Officer, DEFAULT_CANONICAL_OFFICERS_PATH};
use crate::data::ship::{load_ship_index, ShipIndex, ShipRecord, DEFAULT_SHIPS_INDEX_PATH};
use crate::lcars::{load_lcars_dir, LcarsOfficer};

/// Normalize officer name for lookup: alphanumeric lowercase only (matches monte_carlo lookup).
fn normalize_officer_lookup_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

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
        OfficerCache {
            officers,
            by_name,
        }
    }
}

/// Read-only registry of static game data loaded once at startup.
/// Profile and import roster are intentionally excluded (loaded at use time).
#[derive(Debug)]
pub struct DataRegistry {
    pub officers: OfficerCache,
    pub ship_index: Option<ShipIndex>,
    pub hostile_index: Option<HostileIndex>,
    /// LCARS officers when KOBAYASHI_OFFICER_SOURCE=lcars; used by monte_carlo to resolve abilities.
    pub lcars_officers: Option<Vec<LcarsOfficer>>,
    /// Forbidden/chaos tech catalog for merging into profile with imported player tech.
    pub forbidden_chaos_catalog: Option<ForbiddenChaosList>,
}

impl DataRegistry {
    /// Load all static data from disk. Returns an Arc so it can be shared across handlers and threads.
    /// Officer load failure returns Err; missing ship/hostile indices are allowed (None).
    pub fn load() -> Result<Arc<DataRegistry>, std::io::Error> {
        const DEFAULT_LCARS_OFFICERS_DIR: &str = "data/officers";

        let officers = load_canonical_officers(Path::new(DEFAULT_CANONICAL_OFFICERS_PATH))?;
        let officers = OfficerCache::from_officers(officers);

        let ship_index = load_ship_index(DEFAULT_SHIPS_INDEX_PATH);
        let hostile_index = load_hostile_index(DEFAULT_HOSTILES_INDEX_PATH);

        let lcars_officers = if Self::use_lcars_officer_source() {
            load_lcars_dir(Path::new(DEFAULT_LCARS_OFFICERS_DIR)).ok()
        } else {
            None
        };

        let forbidden_chaos_catalog = load_forbidden_chaos(DEFAULT_FORBIDDEN_CHAOS_PATH);

        Ok(Arc::new(DataRegistry {
            officers,
            ship_index,
            hostile_index,
            lcars_officers,
            forbidden_chaos_catalog,
        }))
    }

    fn use_lcars_officer_source() -> bool {
        std::env::var("KOBAYASHI_OFFICER_SOURCE")
            .map(|v| v.eq_ignore_ascii_case("lcars"))
            .unwrap_or(false)
    }

    /// LCARS officers when KOBAYASHI_OFFICER_SOURCE=lcars. Monte Carlo builds by_id/name_to_id from this.
    pub fn lcars_officers(&self) -> Option<&[LcarsOfficer]> {
        self.lcars_officers.as_deref()
    }

    /// Forbidden/chaos tech catalog for merging with imported player tech into profile.
    pub fn forbidden_chaos_catalog(&self) -> Option<&ForbiddenChaosList> {
        self.forbidden_chaos_catalog.as_ref()
    }

    /// Officer list for API listing and crew generator pool building.
    pub fn officers(&self) -> &[Officer] {
        &self.officers.officers
    }

    /// Officer index by normalized name for monte_carlo and resolution.
    pub fn officer_index(&self) -> &HashMap<String, Officer> {
        &self.officers.by_name
    }

    /// Ship index for listing and resolution (resolve still may need per-record load unless cached).
    pub fn ship_index(&self) -> Option<&ShipIndex> {
        self.ship_index.as_ref()
    }

    /// Hostile index for listing and resolution.
    pub fn hostile_index(&self) -> Option<&HostileIndex> {
        self.hostile_index.as_ref()
    }

    /// Resolve ship by id or name using cached index. Per-record file still read from disk.
    pub fn resolve_ship(&self, name_or_id: &str) -> Option<ShipRecord> {
        let index = self.ship_index.as_ref()?;
        let data_dir = Path::new(DEFAULT_SHIPS_INDEX_PATH).parent()?;
        resolve_ship_with_index(index, data_dir, name_or_id)
    }

    /// Resolve hostile by id or name/level using cached index. Per-record file still read from disk.
    pub fn resolve_hostile(&self, name_or_id: &str) -> Option<HostileRecord> {
        let index = self.hostile_index.as_ref()?;
        let data_dir = Path::new(DEFAULT_HOSTILES_INDEX_PATH).parent()?;
        resolve_hostile_with_index(index, data_dir, name_or_id)
    }
}
