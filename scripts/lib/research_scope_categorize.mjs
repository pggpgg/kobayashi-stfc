/**
 * Categorize research project / buff description text for mapping triage.
 * Shared by triage_research_unmapped.mjs and research_mapping_gaps.mjs.
 */

/** Categories where a mapped unconditional combat row is likely overstated globally. */
export const SUSPECT_GLOBAL_SCOPE_CATEGORIES = new Set([
  "station_defense_scope",
  "armada_scope",
  "pvp_scope",
  "ship_specific",
]);

/**
 * @param {string | null | undefined} desc
 * @returns {string}
 */
export function categorizeResearchDescription(desc) {
  const d = (desc || "").toLowerCase();
  if (!d.trim()) return "no_description";
  if (
    /construction|mining|cargo|repair speed|research speed|warp|impulse|cost efficiency|unlock|bundle|store|reputation|components|survey|tiering|protected cargo|not_convert|gift|generator|warehouse|production speed|resource protection|points gained|rewards for defeating|parsteel|tritanium|dilithium storage|generation speed/.test(
      d
    )
  ) {
    return "economy_meta";
  }
  if (
    /station defense|defending the station|defense platform|when defending|against stations|first round of combat when defending/.test(
      d
    )
  ) {
    return "station_defense_scope";
  }
  if (/against players|pvp|player ships|opponent player|grade 5\+ opponent/.test(d)) {
    return "pvp_scope";
  }
  if (/armada|when defending an armada/.test(d)) {
    return "armada_scope";
  }
  if (
    /bonus base.*for the |for the u\.s\.s\.|for the stella|for d'vor|for the botany|for all g4 ships/.test(
      d
    )
  ) {
    return "ship_specific";
  }
  if (/officer/.test(d) && /attack|defense|health/.test(d)) {
    return "officer_stats";
  }
  return "other_unmapped";
}

/** @param {string} category */
export function isSuspectGlobalScopeCategory(category) {
  return SUSPECT_GLOBAL_SCOPE_CATEGORIES.has(category);
}
