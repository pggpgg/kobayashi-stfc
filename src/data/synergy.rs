//! Community/mechanical synergy metadata (see `docs/DESIGN.md` §7).
//!
//! **Matchup priors:** [`crate::optimizer::matchup_priors`] can weight crews by encounter gates and
//! warm-start / optimize-history overlap. A catalog-backed **captain+bridge synergy bump** is
//! deferred until this module (or companion JSON) exposes stable officer-pair rows the optimizer
//! can load without LCARS graph drift.

#[derive(Debug, Clone)]
pub struct SynergyTag {
    pub mechanism: String,
}
