/**
 * Categorize building opaque `buff_*` stats for allowlist triage.
 * Reuses research description heuristics; allowlists economy/meta and out-of-simulator-scope combat-adjacent rows.
 */

import {
  categorizeResearchDescription,
  isSuspectGlobalScopeCategory,
} from "./research_scope_categorize.mjs";

/** Categories that may enter opaque_buff_allowlist.json (intentionally not merged into ship-vs-hostile profile). */
export const BUILDING_ALLOWLIST_CATEGORIES = new Set([
  "economy_meta",
  "unlock_meta",
  "reward_meta",
  "cost_reduction_meta",
  "alliance_starbase_assault",
  "defense_platform",
  "defense_platform_damage",
  "armada_slot_meta",
  "outpost_meta",
  "solo_armada_meta",
  "open_armada_unmodeled",
  "crit_mitigation_unmodeled",
  "aggregation_hyperthermic_unmodeled",
]);

/** Building-specific economy patterns not always caught by research categorizer. */
const BUILDING_ECONOMY_EXTRA = new RegExp(
  "drydock|harvester|vault|warehouse|material fragments|fleet commander|exocomp|skill points|one time reward|warchest|protomatter|transogen|plasma|collisional|magnetic plasma|axionic|maverick|faction store|cannot be stolen|protected when your|harvested|claim daily|construction speed|constructing ships|constructing all ships|constructing station modules|research speed|speed of research|station upgrades|7★ crystal|ore for station|credits earned|fkr credits|nova squadron particles|artifact tokens gained|armada countdown speed|countdown speed ups|mining rate|storage indicates|production rate|repair costs|repair speed|maximum amount|cost efficiency|service awards|independent archives|tiering up chaos|gift|bundle|unlock|tier up speed|scrapping speed|scraping speed|cardassian loot|mirror universe|daily challenge|bounty challenge",
  "i",
);

const UNLOCK_PATTERN =
  /unlock|additional .* slots|exocomp|fleet commander|maverick task key|consumables store/;

const REWARD_META_PATTERN =
  /one time reward|rewards obtained|rewards for completing|tokens gained|particles,|skill points|protomatter rewarded|warchest|artifact tokens|credits earned|fkr credits|claim daily|countdown speed ups granted|cardassian loot|increase rewards|daily challenge|bounty challenge|broken ship parts dropped/;

const COST_REDUCTION_PATTERN =
  /cost efficiency|repair costs.*decreased|cost of 7★|reduces the cost|service awards when used for research/;

const ALLIANCE_STARBASE_ASSAULT_PATTERN =
  /assaults against enemy alliance starbases|assault occurs against your alliance starbase|cardassian stations|alliance starbase|damage of the alliance starbase|shield health of the alliance starbase|hull health of the alliance starbase|armor piercing, accuracy and shield piercing of the alliance starbase|armor, dodge and shield deflection of the alliance starbase/;

const DEFENSE_PLATFORM_PATTERN =
  /defense platforms protect your station from other players/;

const DEFENSE_PLATFORM_DAMAGE_PATTERN =
  /defense platform damage|weapon damage dealt by your defense platforms|armor, shield deflection and dodge of your defense platforms/;

const ARMADA_SLOT_META_PATTERN =
  /maximum number of ships that can take part in your armada attacks on hostile|maximum number of ships that can take part in your open armada attacks on hostile/;

const SOLO_ARMADA_LIMIT_PATTERN =
  /solo armadas.*can only be fought with \d+ ship|q's interference/;

const OUTPOST_FLEET_SIZE_PATTERN = /outpost fleet size/;

const OPEN_ARMADA_WEAPON_DAMAGE_PATTERN =
  /open armada attack on a hostile target/i;

const GROUP_ARMADA_WEAPON_DAMAGE_PATTERN =
  /when they take part in an armada attack on a hostile target/i;

const CRIT_MITIGATION_ACAD_PATTERN =
  /critical mitigation against all academy/i;

const HYPERTHERMIC_AGGREGATION_PATTERN =
  /hyperthermic decay against aggregation hostiles/i;

/**
 * @param {string | null | undefined} desc
 * @returns {string}
 */
export function categorizeBuildingBuffDescription(desc) {
  const d = (desc || "").toLowerCase();
  if (!d.trim()) return "no_description";

  if (REWARD_META_PATTERN.test(d) && /broken ship parts dropped/.test(d)) {
    return "reward_meta";
  }
  if (OUTPOST_FLEET_SIZE_PATTERN.test(d)) return "outpost_meta";
  if (OPEN_ARMADA_WEAPON_DAMAGE_PATTERN.test(d)) return "open_armada_unmodeled";
  if (GROUP_ARMADA_WEAPON_DAMAGE_PATTERN.test(d)) return "combat_unmapped";
  if (CRIT_MITIGATION_ACAD_PATTERN.test(d)) return "crit_mitigation_unmodeled";
  if (HYPERTHERMIC_AGGREGATION_PATTERN.test(d)) return "aggregation_hyperthermic_unmodeled";
  if (ARMADA_SLOT_META_PATTERN.test(d)) return "armada_slot_meta";
  if (SOLO_ARMADA_LIMIT_PATTERN.test(d)) return "solo_armada_meta";
  if (DEFENSE_PLATFORM_DAMAGE_PATTERN.test(d)) return "defense_platform_damage";
  if (DEFENSE_PLATFORM_PATTERN.test(d)) return "defense_platform";
  if (ALLIANCE_STARBASE_ASSAULT_PATTERN.test(d)) return "alliance_starbase_assault";

  const base = categorizeResearchDescription(desc);

  // Remaining scoped combat stays actionable unless matched above.
  if (isSuspectGlobalScopeCategory(base)) return base;
  if (base === "non_armada_hostile_scope") return base;
  if (base === "hostile_and_armada_scope") return base;
  if (base === "ship_specific") return base;
  if (base === "officer_stats") return base;

  if (base === "economy_meta" || BUILDING_ECONOMY_EXTRA.test(d)) {
    if (UNLOCK_PATTERN.test(d)) return "unlock_meta";
    if (REWARD_META_PATTERN.test(d)) return "reward_meta";
    if (COST_REDUCTION_PATTERN.test(d)) return "cost_reduction_meta";
    return "economy_meta";
  }

  if (UNLOCK_PATTERN.test(d)) return "unlock_meta";
  if (REWARD_META_PATTERN.test(d)) return "reward_meta";
  if (COST_REDUCTION_PATTERN.test(d)) return "cost_reduction_meta";

  if (
    /weapon damage|hull health|shield health|base hhp|base shp|pierce|crit|mitigation|isolytic|apex|armor|dodge|hyperthermic|accuracy|hostile|armada|defense platform|starbase|outpost|retaliation|players/.test(
      d
    )
  ) {
    return "combat_unmapped";
  }

  if (BUILDING_ECONOMY_EXTRA.test(d)) return "economy_meta";

  return "other_meta";
}

/**
 * @param {string} category
 * @param {string} description
 * @returns {boolean}
 */
export function isAllowlistCandidate(category, description) {
  if (BUILDING_ALLOWLIST_CATEGORIES.has(category)) return true;
  if (category === "no_description") {
    return false;
  }
  if (category === "other_meta") {
    const d = (description || "").toLowerCase();
    if (UNLOCK_PATTERN.test(d)) return true;
    if (REWARD_META_PATTERN.test(d)) return true;
    if (COST_REDUCTION_PATTERN.test(d)) return true;
    if (BUILDING_ECONOMY_EXTRA.test(d) && !/weapon damage|shield health|hull health|pierce|crit|apex|isolytic/.test(d)) {
      return true;
    }
  }
  return false;
}

/**
 * @param {string} category
 * @returns {string}
 */
export function allowlistCategoryForEntry(category) {
  if (BUILDING_ALLOWLIST_CATEGORIES.has(category)) return category;
  if (category === "other_meta") return "economy_meta";
  return "economy_meta";
}

/**
 * @param {string} stat
 * @param {string} description
 * @param {string} buildingName
 * @param {string} category
 * @returns {string}
 */
export function defaultAllowlistReason(stat, description, buildingName, category) {
  const snippet = (description || "").trim().replace(/\s+/g, " ");
  const short = snippet.length > 72 ? `${snippet.slice(0, 69)}…` : snippet;
  const catLabel = {
    economy_meta: "economy/meta",
    unlock_meta: "unlock/meta",
    reward_meta: "reward/meta",
    cost_reduction_meta: "cost reduction",
    alliance_starbase_assault: "alliance starbase / assault",
    defense_platform: "defense platform",
    defense_platform_damage: "defense platform damage",
    armada_slot_meta: "armada slot meta",
    outpost_meta: "outpost meta",
    solo_armada_meta: "solo armada ship limit",
  }[category] ?? category;
  if (short) {
    return `${buildingName}: ${short}; ${catLabel}; intentionally unmapped for ship-vs-hostile sim`;
  }
  return `${buildingName}: ${stat}; ${catLabel}; intentionally unmapped for ship-vs-hostile sim`;
}
