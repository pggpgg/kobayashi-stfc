/**
 * Categorize research project / buff description text for mapping triage.
 * Shared by triage_research_unmapped.mjs, research_mapping_gaps.mjs, and import.
 */

/** Categories where a mapped unconditional combat row is likely overstated globally. */
export const SUSPECT_GLOBAL_SCOPE_CATEGORIES = new Set([
  "station_defense_scope",
  "armada_scope",
  "pvp_scope",
  "ship_specific",
  "wave_defense_scope",
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
  // Non-Armada hostile bonuses apply in default ship-vs-hostile sim — not a global leak.
  if (
    /\bnon[- ]armada\b|\bnot armadas?\b|\bexcluding armadas?\b/.test(d) &&
    /\bhostile/.test(d)
  ) {
    return "non_armada_hostile_scope";
  }
  // Applies to both hostiles and armadas — OK to merge for default hostile-only sim (subset).
  if (/\bhostiles?\s+and\s+armadas?\b|\bhostiles?\s*\/\s*armadas?\b/.test(d)) {
    return "hostile_and_armada_scope";
  }
  if (/\bwave defense\b/.test(d)) {
    return "wave_defense_scope";
  }
  if (/against players|pvp|player ships|opponent player|grade 5\+ opponent/.test(d)) {
    return "pvp_scope";
  }
  if (
    /station defense|defending the station|defense platform|when defending the station|against stations|first round of combat when defending|against defense platforms/.test(
      d
    )
  ) {
    return "station_defense_scope";
  }
  if (
    /\bagainst armadas?\b|\bvs\.?\s*armadas?\b|\bversus armadas?\b|\bsolo armadas?\b|\bgroup armadas?\b|\bexchange armadas?\b|\bwhen defending an armada\b|\bfighting armadas?\b/.test(
      d
    )
  ) {
    return "armada_scope";
  }
  if (/\barmada\b/.test(d) && !/\bhostile/.test(d)) {
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

/**
 * Prefer buff-level description, then project-level (same as mapping gap scan).
 * @param {{ projectLocaId?: number | null, buffLocaId?: number | null, descriptionByLocaId?: Map<number,string> }} opts
 */
export function descriptionForScopeCheck(opts) {
  const { projectLocaId, buffLocaId, descriptionByLocaId } = opts;
  if (descriptionByLocaId && typeof buffLocaId === "number") {
    const t = descriptionByLocaId.get(buffLocaId);
    if (t && t.trim()) return t;
  }
  if (descriptionByLocaId && typeof projectLocaId === "number") {
    return descriptionByLocaId.get(projectLocaId) || "";
  }
  return "";
}

/** @param {object | null | undefined} mapping */
export function mappingHasCombatConditions(mapping) {
  if (!mapping || typeof mapping !== "object") return false;
  return !!(
    mapping.defender_ship_class ||
    mapping.defender_faction ||
    mapping.attacker_faction ||
    (mapping.attacker_factions || []).length ||
    mapping.requires_morale ||
    mapping.requires_defender_burning ||
    mapping.requires_defender_hull_breach
  );
}

/**
 * True when an unconditional mapping must not enter the global research catalog.
 * Conditional mappings (faction gates, morale, etc.) are kept — the gap scan skips them.
 *
 * @param {{ mapping: object, buff?: { loca_id?: number }, projectLocaId?: number | null, descriptionByLocaId?: Map<number,string> }} opts
 */
export function shouldExcludeUnconditionalGlobalMerge(opts) {
  const { mapping, buff, projectLocaId, descriptionByLocaId } = opts;
  if (!mapping || typeof mapping !== "object") return false;
  if (mapping.exclude_global_merge === true) return true;
  if (mappingHasCombatConditions(mapping)) return false;
  const desc = descriptionForScopeCheck({
    projectLocaId,
    buffLocaId: buff?.loca_id,
    descriptionByLocaId,
  });
  const category = categorizeResearchDescription(desc);
  return isSuspectGlobalScopeCategory(category);
}
