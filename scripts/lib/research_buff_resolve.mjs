/**
 * Shared research buff id → engine stat entry resolution (maps + loca + text inference).
 * Used by import_stfcspace_research.mjs and maintenance scripts.
 */

import {
  inferCombatStatFromDescription,
  inferCombatStatFromProjectName,
} from "./research_stat_inference.mjs";

export function statEntryFromJsonValue(v, defaultOp) {
  if (v == null) return null;
  if (typeof v === "string") return { stat: v, operator: defaultOp ?? "add" };
  if (typeof v === "object" && !Array.isArray(v) && typeof v.stat === "string") {
    const out = { stat: v.stat, operator: v.operator ?? defaultOp ?? "add" };
    if (v.requires_defender_burning) out.requires_defender_burning = true;
    if (v.requires_morale) out.requires_morale = true;
    if (v.requires_defender_hull_breach) out.requires_defender_hull_breach = true;
    if (typeof v.defender_ship_class === "string") out.defender_ship_class = v.defender_ship_class;
    if (typeof v.defender_faction === "string") out.defender_faction = v.defender_faction;
    if (typeof v.attacker_faction === "string") out.attacker_faction = v.attacker_faction;
    if (Array.isArray(v.attacker_factions) && v.attacker_factions.length) {
      out.attacker_factions = v.attacker_factions.map(String).filter(Boolean);
    }
    return out;
  }
  return null;
}

/** One buff id may map to several engine stats (e.g. armor + shield_deflection + dodge). */
export function statEntriesFromJsonValue(v, defaultOp) {
  if (v == null) return [];
  if (Array.isArray(v)) {
    const out = [];
    for (const item of v) {
      const e = statEntryFromJsonValue(item, defaultOp);
      if (e) out.push(e);
    }
    return out;
  }
  const single = statEntryFromJsonValue(v, defaultOp);
  return single ? [single] : [];
}

/** Normalize inferCombatStatFromDescription / inferCombatStatFromProjectName return values. */
export function coerceStatMapping(inferred) {
  if (inferred == null) return null;
  if (typeof inferred === "string") return { stat: inferred, operator: "add" };
  return {
    stat: inferred.stat,
    operator: inferred.operator ?? "add",
    ...(inferred.requires_defender_burning ? { requires_defender_burning: true } : {}),
    ...(inferred.requires_morale ? { requires_morale: true } : {}),
    ...(inferred.requires_defender_hull_breach ? { requires_defender_hull_breach: true } : {}),
    ...(typeof inferred.defender_ship_class === "string"
      ? { defender_ship_class: inferred.defender_ship_class }
      : {}),
    ...(typeof inferred.defender_faction === "string" ? { defender_faction: inferred.defender_faction } : {}),
    ...(typeof inferred.attacker_faction === "string" ? { attacker_faction: inferred.attacker_faction } : {}),
    ...(Array.isArray(inferred.attacker_factions) && inferred.attacker_factions.length
      ? {
          attacker_factions: inferred.attacker_factions.map(String).filter(Boolean),
        }
      : {}),
  };
}

/**
 * @typedef {object} ResearchBuffResolveContext
 * @property {Record<string|number, unknown>} [researchBuffMapping] optional inline overrides by buff id
 * @property {Record<string, unknown>} researchBuffById data/research/buff_id_to_stat.json
 * @property {Record<string, unknown>} commonBuffNormalization data/buildings/buff_id_to_stat.json
 * @property {Record<string, unknown>} locaIdToStat data/research/loca_id_to_stat.json — buff loca id keys
 * @property {Map<number,string>} [descriptionByLocaId] research_project_description text by loca id
 * @property {Map<number,string>} [projectNamesByLocaId] research_project_name by loca id
 */

/**
 * Resolve engine stat mappings for one buff row from upstream research detail JSON.
 * @param {ResearchBuffResolveContext} ctx
 * @param {{ id?: number, loca_id?: number }} buff
 * @param {number | null | undefined} projectLocaId detail.loca_id
 */
export function resolveBuffStatMappings(ctx, buff, projectLocaId) {
  if (!buff || typeof buff.id !== "number") return [];
  const buffId = buff.id;
  const key = String(buffId);

  const inline = ctx.researchBuffMapping?.[buffId] ?? ctx.researchBuffMapping?.[key];
  if (inline) {
    const entries = statEntriesFromJsonValue(inline, "add");
    if (entries.length > 0) return entries;
  }

  const researchExplicit = statEntriesFromJsonValue(ctx.researchBuffById[key], "add");
  if (researchExplicit.length > 0) return researchExplicit;

  const fromBuildings = statEntriesFromJsonValue(ctx.commonBuffNormalization[key], "add");
  if (fromBuildings.length > 0) return fromBuildings;

  if (typeof buff.loca_id === "number") {
    const fromLoca = statEntriesFromJsonValue(ctx.locaIdToStat[String(buff.loca_id)], "add");
    if (fromLoca.length > 0) return fromLoca;
  }

  const descriptionByLocaId = ctx.descriptionByLocaId;
  const projectNamesByLocaId = ctx.projectNamesByLocaId;

  if (descriptionByLocaId && descriptionByLocaId.size > 0) {
    if (typeof buff.loca_id === "number") {
      const t = descriptionByLocaId.get(buff.loca_id);
      const inferred = inferCombatStatFromDescription(t);
      const coerced = coerceStatMapping(inferred);
      if (coerced) return [coerced];
    }
    if (typeof projectLocaId === "number") {
      const t = descriptionByLocaId.get(projectLocaId);
      const inferred = inferCombatStatFromDescription(t);
      const coerced = coerceStatMapping(inferred);
      if (coerced) return [coerced];
    }
  }

  if (projectNamesByLocaId && projectNamesByLocaId.size > 0) {
    if (typeof buff.loca_id === "number") {
      const inferred = inferCombatStatFromProjectName(projectNamesByLocaId.get(buff.loca_id));
      const coerced = coerceStatMapping(inferred);
      if (coerced) return [coerced];
    }
    if (typeof projectLocaId === "number") {
      const inferred = inferCombatStatFromProjectName(projectNamesByLocaId.get(projectLocaId));
      const coerced = coerceStatMapping(inferred);
      if (coerced) return [coerced];
    }
  }

  return [];
}
