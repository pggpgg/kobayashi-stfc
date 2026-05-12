/**
 * Shared heuristics: STFC research English text → single engine stat key (or null).
 * Multi-stat lines (e.g. "Mitigation Stats") return null so callers rely on buff_id / loca JSON overrides.
 * Used by import_stfcspace_research.mjs and node:test regressions.
 */

/**
 * When a combat description mentions a ship class constraint (e.g. "Explorers battling Interceptors"),
 * extract the defender ship class slug (explorer, interceptor, battleship, survey).
 */
export function inferShipClassConditional(text, stat) {
  const t = text.toLowerCase();
  const defenderClass = (() => {
    if (/\b(?:battling|against|vs\.?|versus)\s+(?:all\s+)?interceptors?\b/.test(t)) return "interceptor";
    if (/\b(?:battling|against|vs\.?|versus)\s+(?:all\s+)?explorers?\b/.test(t)) return "explorer";
    if (/\b(?:battling|against|vs\.?|versus)\s+(?:all\s+)?battleships?\b/.test(t)) return "battleship";
    if (/\b(?:battling|against|vs\.?|versus)\s+(?:all\s+)?surve(?:y|yor)s?\b/.test(t)) return "survey";
    return null;
  })();
  if (defenderClass) {
    if (/\bmorale\b/i.test(t)) {
      return {
        stat,
        operator: "add",
        defender_ship_class: defenderClass,
        requires_morale: true,
      };
    }
    return { stat, operator: "add", defender_ship_class: defenderClass };
  }
  return stat;
}

/**
 * Infer engine stat from English description text. Conservative: non-ship / economy lines return null.
 * Faction- or mode-specific wording may still map here; catalog is applied as global profile bonuses.
 */
export function inferCombatStatFromDescription(text) {
  if (!text || typeof text !== "string") return null;
  const t = text.toLowerCase();
  if (
    /construction speed|build speed|repair speed|research speed|mining\b|cargo |cargo\.|cost efficiency|unlock|blueprint|dilithium protection|parsteel|tritanium|for components|foundry|lab building|module upgrade|resource generation|away team|away teams|warp speed|tiering up|protected cargo|rewards for defeating|not_convert|get more from hostiles in these systems/.test(
      t
    )
  ) {
    return null;
  }
  if (/\bisolytic\b/.test(t)) {
    if (/\b(defense|defence|resist|mitigation against isolytic)\b/.test(t)) return "isolytic_defense";
    if (
      /\bmorale\b/.test(t) &&
      /\b(damage|attack|potency|offense)\b/.test(t)
    ) {
      return { stat: "isolytic_damage", requires_morale: true };
    }
    if (/\b(damage|attack|potency|offense)\b/.test(t)) return "isolytic_damage";
    return null;
  }
  if (/\bapex barrier\b/i.test(t)) {
    if (/\bmorale\b/i.test(t)) {
      return { stat: "apex_barrier", requires_morale: true };
    }
    return "apex_barrier";
  }
  if (/\bapex shred\b/i.test(t)) {
    if (/\bmorale\b/i.test(t)) {
      return { stat: "apex_shred", requires_morale: true };
    }
    return "apex_shred";
  }
  if (
    /damage reduction|reduces? (the )?damage taken|reduces base damage taken|incoming damage|less damage from/.test(
      t
    )
  ) {
    if (!/defense platform|defensive platform|station defense/i.test(t)) return "damage_reduction";
  }
  if (
    /\baccuracy\b/.test(t) &&
    /\b(increases|increase|increased|improved|bonus|enhanced)\b/.test(t) &&
    !/\bofficer\b/.test(t)
  ) {
    return "accuracy";
  }
  if (/critical damage|crit damage|severity of critical|critical hit damage/.test(t)) {
    if (/battling|vs\.? |versus/.test(t)) {
      return inferShipClassConditional(t, "crit_damage");
    }
    return "crit_damage";
  }
  if (/critical hit chance|critical chance|crit chance|chance to (land|score|deal) (a )?critical/.test(t)) {
    if (/battling|vs\.? |versus/.test(t)) {
      return inferShipClassConditional(t, "crit_chance");
    }
    return "crit_chance";
  }
  if (
    /shield piercing|armor piercing|shield penetration|shield pen\b| pierce |piercing against|pierces the|base piercing stats|shield pierce.*armor pierce/.test(
      t
    )
  ) {
    return "pierce";
  }
  // UI "Mitigation Stats" = armor + shield deflection + dodge (separate rows). Never collapse to shield_mitigation.
  if (/\bmitigation stats\b/.test(t)) return null;
  // Shield Deflection is its own stat; do not conflate with dodge or engine shield_mitigation.
  if (/\bshield deflection\b/.test(t)) return "shield_deflection";
  if (/\bshield mitigation\b/.test(t)) return "shield_mitigation";
  if (/\bdodge\b/.test(t)) return "dodge";
  if (/\barmor\b/.test(t) && !/piercing|pierce/.test(t)) {
    if (/ship|hull|all ships|your ships|franklin|vessel/.test(t)) return "armor";
  }
  if (/hull health|hull hit points|hull points|hull strength|max hull/.test(t)) {
    if (!/defense platform|defensive platform|station/.test(t)) return "hull_hp";
  }
  if (/shield health|shield hit points|shield capacity|shield strength|max shield/.test(t)) {
    if (!/defense platform|defensive platform|station/.test(t)) return "shield_hp";
  }
  if (
    /weapon damage|base damage dealt|damage dealt to hostiles|damage dealt to hostile|offensive damage|increases base damage|increases base weapon|bonus to base weapon damage/.test(
      t
    ) &&
    /opponent has burning|while your opponent has burning|target has burning|while the target has burning/.test(
      t
    ) &&
    !/defense platform|station|away team/.test(t)
  ) {
    return { stat: "weapon_damage", requires_defender_burning: true };
  }
  if (
    /weapon damage|base damage dealt|damage dealt to hostiles|damage dealt to hostile|offensive damage|increases base damage|bonus to base weapon damage/.test(
      t
    )
  ) {
    if (!/defense platform|station|away team/.test(t)) {
      if (/battling|vs\.? |versus/.test(t)) {
        return inferShipClassConditional(t, "weapon_damage");
      }
      return "weapon_damage";
    }
  }
  return null;
}

/**
 * When translations have no research_project_description for this loca_id, use the display name.
 * Conservative: avoids economy/building names; may mis-guess rare ship-specific nodes.
 */
export function inferCombatStatFromProjectName(name) {
  if (!name || typeof name !== "string") return null;
  const t = name.toLowerCase();
  if (
    /construction|mining|cargo\b|repair speed|research speed|warp speed|cost efficiency|unlock|dilithium|parsteel|tritanium|survey|protected cargo|tiering|blueprint|building|module|resource|components\b/.test(
      t
    )
  ) {
    return null;
  }
  if (/\bisolytic\b/.test(t)) {
    if (/\b(defense|defence|resist)\b/.test(t)) return "isolytic_defense";
    if (/\bmorale\b/.test(t)) return { stat: "isolytic_damage", requires_morale: true };
    return "isolytic_damage";
  }
  if (/damage reduction|critical damage reduction|resilience vs/.test(t)) return "damage_reduction";
  if (/critical damage|crit damage/.test(t)) return "crit_damage";
  if (/critical chance|crit chance/.test(t)) return "crit_chance";
  if (/\bdirect hit\b/.test(t)) return "weapon_damage";
  if (/\btargeting array\b/.test(t)) return "accuracy";
  if (/shield piercing|armor piercing|penetration/.test(t)) return "pierce";
  if (/\bshield deflection\b/.test(t)) return "shield_deflection";
  if (/\bshield mitigation\b/.test(t)) return "shield_mitigation";
  if (/\bdodge\b/.test(t)) return "dodge";
  if (/\barmor\b/.test(t) && !/piercing|pierce/.test(t)) return "armor";
  if (/hull density|hull health|hull strength|max hull/.test(t)) return "hull_hp";
  if (/shield health|shield capacity|shield hardening|shield strength/.test(t)) return "shield_hp";
  if (
    /weapon|damage|tactics|assault|offense|firepower|battleship|interceptor|explorer|starship|mayflower|franklin|phindra|turas|talla/.test(
      t
    )
  ) {
    if (!/defense platform|station defense/.test(t)) return "weapon_damage";
  }
  return null;
}
