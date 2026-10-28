/** Client-native families for which prose/display flags must not decide units or scope.
 * Enum facts: client build272 metadata SHA ab0ccdc3e9e09e59d16a13431e2a0722255a71d5ff373cc09d95a47cd130ca04.
 * ClientModifierType fields60921..60931/60984..60985; BuffOperation; BuffCondition.Values fields56113..56116/56125.
 * Native FleetService.ApplySingleBuffToStatContainer(0x1817fe0c0) puts op1 in ADD and op3 in MULT without /100.
 */
export const CLIENT_STAT_FAMILIES = new Set([
  "hull_hp", "shield_hp", "hull_hp_flat", "shield_hp_flat", "weapon_damage", "weapon_damage_flat",
  "crit_chance", "crit_damage", "crit_damage_flat", "isolytic_damage", "isolytic_defense",
  "armor", "shield_deflection", "dodge", "officer_defense_below_decks", "apex_barrier",
]);
const CLASSES = new Map([[0, "explorer"], [1, "interceptor"], [2, "battleship"], [3, "survey"], [116, "sentinel"]]);
const MIN_SYNDICATE = new Map([[203, 30], [204, 33], [205, 35]]);
const VERIFIED_ROUND_START_RANKS = [0.5, 0.75, 1.15, 1.6, 2.05, 2.6, 3.15, 3.75, 4.35, 5];
const VERIFIED_ROUND_START_MITIGATION = new Map([
  [679782642, { condition_code: 38, gate: "requires_defender_burning",
    sha256: "9bbc95f81b971ccf5df0abfa8db6960abcad71c81c80e0a707b476f3799c917f" }],
  [1142346453, { condition_code: 33, gate: "requires_morale",
    sha256: "f0dcb95cb67dce201b37aea3c100b4744cd46dabb878cc17ac63f9cf1223278a" }],
  [4033066544, { condition_code: 37, gate: "requires_defender_hull_breach",
    sha256: "1c83ac6da9de0cbf275cdfb29675602249855c08e8bd14b7e20b3dbed4832aa6" }],
]);
export const VERIFIED_ORLOP_DEFENSE_BUFF_ID = 1895868262;
const VERIFIED_ORLOP_DEFENSE_RANKS = [0.2, 0.26, 0.33, 0.41, 0.5, 0.59, 0.69, 0.79, 0.89, 1, 0];
const VERIFIED_ECLIPSE_DEFENSE_RANKS = [0.5, 1, 1.6, 2.1, 2.8, 3.6, 4.4, 5.2, 6, 7, 0];

/** Exact native group-66 Eclipse hull defense source; runtime-only because its predicate is unresolved generically. */
export function isVerifiedEclipseDefenseBuff(buffId, row) {
  return Number(buffId) === 1238121830 && row?.sha256 === "4f04a8d92ca23d5966f461f6e9339ec8a403d14c8a755e617765750f8bce1013" &&
    row.modifier_code === 73 && row.operation_code === 3 && row.target_code === 1 && row.trigger_code === 25 &&
    row.num_rounds === 0 && row.faction_id === 1750120904 && row.ranked_value_type === 0 &&
    row.ranked_value_count === 99 && row.condition_codes?.length === 1 && row.condition_codes[0] === 17 &&
    row.attribute_fields?.length === 1 && row.attribute_fields[0] === 8 && row.unknown_fields?.length === 0 &&
    row.ranked_values?.length === VERIFIED_ECLIPSE_DEFENSE_RANKS.length &&
    row.ranked_values.every((value, i) => value === VERIFIED_ECLIPSE_DEFENSE_RANKS[i]) &&
    row.ranked_chances?.length === 1 && row.ranked_chances[0] === 1 && row.ranked_chance_count === 99;
}

/** Project and localization linkage is evidence for this row only, not a generic condition-17 rule. */
export function isVerifiedEclipseDefenseResearchDetail(detail, row) {
  const buff = detail?.buffs?.[0];
  return detail?.id === 3260398513 && detail?.loca_id === 317 && buff?.id === 1238121830 &&
    buff.loca_id === 317 && buff.value_is_percentage === true && buff.show_percentage === true &&
    buff.value_type === 0 && buff.ranked_is_value_chance === 0 && Array.isArray(buff.values) &&
    buff.values.length === 99 && buff.values.every((rank, i) => rank?.chance === 1 &&
      rank.value === VERIFIED_ECLIPSE_DEFENSE_RANKS[Math.min(i, VERIFIED_ECLIPSE_DEFENSE_RANKS.length - 1)]) &&
    isVerifiedEclipseDefenseBuff(buff.id, row);
}

/** Exact native modifier 57 / SelfBelowDeck row. This does not select modifier 57 generally. */
export function mapVerifiedOrlopDefenseBuff(buffId, row) {
  if (Number(buffId) !== VERIFIED_ORLOP_DEFENSE_BUFF_ID || !row ||
      row.sha256 !== "90eb257edcaceff8659184bcf2cdf1b41d4ce616dded4db5487b3ad806ec11ac" ||
      row.modifier_code !== 57 || row.operation_code !== 3 ||
      row.target_code !== 11 || row.trigger_code !== 24 ||
      row.ranked_value_type !== 0 || row.ranked_value_count !== 99 ||
      !Array.isArray(row.condition_codes) || row.condition_codes.length !== 0 ||
      !Array.isArray(row.attribute_fields) || row.attribute_fields.length !== 0 ||
      !Array.isArray(row.unknown_fields) || row.unknown_fields.length !== 0 ||
      !Array.isArray(row.ranked_values) ||
      row.ranked_values.length !== VERIFIED_ORLOP_DEFENSE_RANKS.length ||
      row.ranked_values.some((value, i) => value !== VERIFIED_ORLOP_DEFENSE_RANKS[i]) ||
      !Array.isArray(row.ranked_chances) || row.ranked_chances.length !== 1 ||
      row.ranked_chances[0] !== 1 || row.ranked_chance_count !== 99) return [];
  return [{ stat: "officer_defense_below_decks", operator: "add", value_unit: "fraction",
    client_verified: true, client_ranked_values: row.ranked_values }];
}

/** Upstream project identity and rank curve must agree with the selected native row. */
export function isVerifiedOrlopResearchDetail(ctx, buff, projectLocaId) {
  if (ctx.researchProjectId !== 282179003 || ctx.researchDetailId !== 282179003 ||
      projectLocaId !== 43049 || buff?.id !== VERIFIED_ORLOP_DEFENSE_BUFF_ID ||
      buff.loca_id !== 43049 || buff.value_is_percentage !== true ||
      buff.show_percentage !== true || buff.value_type !== 0 ||
      buff.ranked_is_value_chance !== 0 || buff.index !== null ||
      !Array.isArray(buff.values) || buff.values.length !== 99) return false;
  return buff.values.every((rank, i) => rank?.chance === 1 &&
    rank.value === VERIFIED_ORLOP_DEFENSE_RANKS[Math.min(i, VERIFIED_ORLOP_DEFENSE_RANKS.length - 1)]);
}

/** Three separately reviewed one-round status effects retain their catalog fallback. */
export function mapVerifiedRoundStartMitigationBuff(buffId, row) {
  const verified = VERIFIED_ROUND_START_MITIGATION.get(Number(buffId));
  if (!verified || !row || row.sha256 !== verified.sha256 ||
      row.modifier_code !== 73 || row.operation_code !== 3 ||
      row.target_code !== 1 || row.trigger_code !== 18 ||
      row.ranked_value_type !== 0 || row.num_rounds !== 1 || row.faction_id !== -1 ||
      !Array.isArray(row.condition_codes) || row.condition_codes.length !== 1 ||
      row.condition_codes[0] !== verified.condition_code ||
      !Array.isArray(row.attribute_fields) || row.attribute_fields.length !== 2 ||
      row.attribute_fields[0] !== 2 || row.attribute_fields[1] !== 8 ||
      !Array.isArray(row.unknown_fields) || row.unknown_fields.length !== 0 ||
      !Array.isArray(row.ranked_values) || row.ranked_values.length !== VERIFIED_ROUND_START_RANKS.length ||
      row.ranked_value_count !== 99 ||
      row.ranked_values.some((v, i) => v !== VERIFIED_ROUND_START_RANKS[i]) ||
      !Array.isArray(row.ranked_chances) || row.ranked_chances.length !== 1 ||
      row.ranked_chances[0] !== 1 || row.ranked_chance_count !== 99) return [];
  return [{ stat: "base_mitigation_stats_bonus", operator: "add", value_unit: "fraction",
    [verified.gate]: true, client_verified: true, client_ranked_values: row.ranked_values }];
}

/** A stable reason for quarantining a definition; undefined means supported. */
export function clientResearchBuffExclusion(row, options = {}) {
  if (!row) return "missing_definition";
  const officerStatModifier = [56, 57, 58, 59].includes(row.modifier_code);
  if (row.target_code !== 1 && !(officerStatModifier && row.target_code === 5)) return "unsupported_target";
  if (row.ranked_value_type !== 0) return "unsupported_ranked_value_type";
  if (!Array.isArray(row.ranked_values) || !row.ranked_values.length ||
      row.ranked_values.some((v) => !Number.isFinite(v))) return "invalid_ranked_values";
  if (!Array.isArray(row.ranked_chances) || !row.ranked_chances.length ||
      row.ranked_chances.some((v) => v !== 1)) return "unsupported_chance";
  if (!Array.isArray(row.attribute_fields) || (row.unknown_fields?.length ?? 0)) return "unknown_fields";
  const codes = row.condition_codes;
  const directHitGradeGate = row.modifier_code === 74 && codes?.length === 1 && codes[0] === 123;
  const simpleClassGate = codes?.length === 1 && CLASSES.has(codes[0]);
  if (row.modifier_code === 74 && codes?.length && !directHitGradeGate && !simpleClassGate) return "unsupported_condition";
  if (!Array.isArray(codes) || (codes.length && !directHitGradeGate && (codes.length !== 1 ||
      !(CLASSES.has(codes[0]) || (options.allowMinSyndicate && MIN_SYNDICATE.has(codes[0])))))) {
    return "unsupported_condition";
  }
  if ([11, 12, 13, 73].includes(row.modifier_code)
      ? row.trigger_code !== 24 : ![24, 25].includes(row.trigger_code)) return "unsupported_trigger";
  if (officerStatModifier && codes.length) return "unsupported_condition";
  if (row.attribute_fields.length) return "unsupported_attributes";
  if ([0, 1].includes(row.modifier_code)) return "unsupported_weapon_subtype";
  const validOperation = [60, 61].includes(row.modifier_code) ? [1, 3].includes(row.operation_code)
    : row.modifier_code === 2 ? row.operation_code === 3
      : [707, 808, 67001].includes(row.modifier_code) ? row.operation_code === 1
      : [6, 7, 8, 11, 12, 13, 56, 57, 58, 59, 73, 74].includes(row.modifier_code) ? row.operation_code === 3
      : [9, 10].includes(row.modifier_code) && row.operation_code === 1;
  if (!validOperation) return "unsupported_operation_or_modifier";
}

/** Empty means intentionally unsupported, never fall back to a name/description guess. */
export function mapClientResearchBuff(row, options = {}) {
  if (clientResearchBuffExclusion(row, options)) return [];
  const condition = {};
  if (row.condition_codes.length) {
    const code = row.condition_codes[0];
    if (code === 123 && row.modifier_code === 74) condition.attacker_min_grade = 4;
    else if (CLASSES.has(code)) condition.attacker_ship_class = CLASSES.get(code);
    else condition.attacker_min_syndicate_level = MIN_SYNDICATE.get(code);
  }
  if (row.modifier_code === 67001) {
    return [{ stat: "apex_barrier", operator: "add", value_unit: "absolute", ...condition,
      client_verified: true, client_ranked_values: row.ranked_values }];
  }
  if ([11, 12, 13, 73].includes(row.modifier_code)) {
    const stats = row.modifier_code === 73 ? ["armor", "shield_deflection", "dodge"]
      : [{ 11: "dodge", 12: "armor", 13: "shield_deflection" }[row.modifier_code]];
    return stats.map(stat => ({ stat, operator: "add", value_unit: "fraction", ...condition,
      client_verified: true, client_ranked_values: row.ranked_values }));
  }
  if ([6, 7, 8].includes(row.modifier_code) && row.operation_code === 3) {
    const stat = { 6: "accuracy", 7: "armor_piercing", 8: "shield_piercing" }[row.modifier_code];
    return [{ stat, operator: "add", value_unit: "fraction", ...condition,
      client_verified: true, client_ranked_values: row.ranked_values }];
  }
  if (row.modifier_code === 74 && row.operation_code === 3) {
    return ["accuracy", "armor_piercing", "shield_piercing"].map(stat => ({
      stat, operator: "add", value_unit: "fraction", ...condition,
      client_verified: true, client_ranked_values: row.ranked_values,
    }));
  }
  if ([56, 57, 58, 59].includes(row.modifier_code) && row.operation_code === 3 && row.target_code === 5) {
    const stats = row.modifier_code === 59 ? ["officer_attack", "officer_defense", "officer_health"]
      : [{ 56: "officer_attack", 57: "officer_defense", 58: "officer_health" }[row.modifier_code]];
    return stats.map(stat => ({ stat, operator: "add", value_unit: "fraction",
      client_verified: true, client_ranked_values: row.ranked_values }));
  }
  let stat, value_unit = "fraction";
  if ([60, 61].includes(row.modifier_code) && [1, 3].includes(row.operation_code)) {
    stat = row.modifier_code === 60 ? "shield_hp" : "hull_hp";
    if (row.operation_code === 1) { stat += "_flat"; value_unit = "absolute"; }
  } else if (row.modifier_code === 2 && row.operation_code === 3) stat = "weapon_damage";
  else if (row.modifier_code === 707 && row.operation_code === 1) stat = "isolytic_damage";
  else if (row.modifier_code === 808 && row.operation_code === 1) stat = "isolytic_defense";
  else if ([9, 10].includes(row.modifier_code) && row.operation_code === 1) {
    // Native ADD adds to the base critical multiplier; the legacy crit_damage
    // profile key instead multiplies it by (1 + value).
    stat = row.modifier_code === 9 ? "crit_chance" : "crit_damage_flat";
  } else return [];
  return [{ stat, operator: "add", value_unit, ...condition,
    client_verified: true, client_ranked_values: row.ranked_values }];
}

/** Static coverage only. References are public research project IDs and names. */
export function researchClientBuffCoverage(manifest, references = [], options = {}) {
  const byBuff = new Map();
  for (const { buff_id, rid, name } of references) {
    if (!byBuff.has(buff_id)) byBuff.set(buff_id, []);
    byBuff.get(buff_id).push({ rid, name });
  }
  const reasons = {}, stats = {};
  const rows = Object.entries(manifest.buffs).map(([id, row]) => {
    const verifiedRoundStart = mapVerifiedRoundStartMitigationBuff(id, row);
    const verifiedOrlop = mapVerifiedOrlopDefenseBuff(id, row);
    const exclusion = verifiedRoundStart.length || verifiedOrlop.length ? undefined
      : clientResearchBuffExclusion(row, options);
    const mappings = verifiedRoundStart.length ? verifiedRoundStart
      : verifiedOrlop.length ? verifiedOrlop : mapClientResearchBuff(row, options);
    if (exclusion) reasons[exclusion] = (reasons[exclusion] ?? 0) + 1;
    for (const mapping of mappings) stats[mapping.stat] = (stats[mapping.stat] ?? 0) + 1;
    return { buff_id: Number(id), modifier_code: row.modifier_code, operation_code: row.operation_code,
      target_code: row.target_code, trigger_code: row.trigger_code, condition_codes: row.condition_codes,
      ...(exclusion ? { exclusion_reason: exclusion } : {
        mappings: mappings.map(({ client_ranked_values, client_verified, ...mapping }) => mapping),
      }), projects: (byBuff.get(Number(id)) ?? []).sort((a, b) => a.rid - b.rid) };
  }).sort((a, b) => a.buff_id - b.buff_id);
  const quarantined = Object.values(reasons).reduce((a, b) => a + b, 0);
  return { schema_version: 1, source_cache_sha256: manifest.provenance.cache_sha256,
    limitation: "Counts cover selected research BuffSpecs only. Quarantine means unsupported scope or semantics, not an absent in-game bonus. Canonical status effects are reviewed separately.",
    summary: { selected_buff_count: rows.length, supported_buff_count: rows.length - quarantined,
      quarantined_buff_count: quarantined, quarantine_reasons: reasons, supported_stats: stats }, rows };
}
