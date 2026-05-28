#!/usr/bin/env node
/**
 * Inventory conditional research catalog rows by combat routing path.
 * Mirrors merge rules in src/data/research.rs + research_effect_spec_adapter.rs.
 *
 * Usage (repo root):
 *   node scripts/inventory_research_conditional.mjs [--json] [--markdown]
 *
 * With --markdown, writes docs/research_conditional_inventory.md (counts + flagship table).
 */

import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const CATALOG_PATH = path.join(REPO_ROOT, "data", "research_catalog.json");
const CANONICAL_PATH = path.join(REPO_ROOT, "data", "research_canonical.json");
const OUT_MD = path.join(REPO_ROOT, "docs", "research_conditional_inventory.md");

const DEFENDER_FACTION_SEAT_STATS = new Set([
  "armor",
  "shield_deflection",
  "dodge",
  "pierce",
  "shield_mitigation",
  "accuracy",
  "isolytic_damage",
  "isolytic_defense",
  "apex_shred",
  "apex_barrier",
]);

const ATTACK_SEAT_STATS = new Set(["weapon_damage", "crit_chance", "crit_damage"]);

function bonusCondition(b) {
  return {
    defender_ship_class: b.defender_ship_class ?? null,
    defender_faction: b.defender_faction ?? null,
    attacker_faction: b.attacker_faction ?? null,
    attacker_factions: b.attacker_factions ?? [],
    requires_morale: Boolean(b.requires_morale),
    requires_defender_burning: Boolean(b.requires_defender_burning),
    requires_defender_hull_breach: Boolean(b.requires_defender_hull_breach),
  };
}

function isConditional(c) {
  return (
    c.defender_ship_class ||
    c.defender_faction ||
    c.attacker_faction ||
    c.attacker_factions.length > 0 ||
    c.requires_morale ||
    c.requires_defender_burning ||
    c.requires_defender_hull_breach
  );
}

function isOwnerFactionGated(c) {
  if (c.attacker_faction?.trim()) return true;
  return c.attacker_factions.some((s) => s?.trim());
}

function skipOwnerFactionMergeForDefenderGatedHullShield(b, c) {
  return (
    isOwnerFactionGated(c) &&
    c.defender_faction?.trim() &&
    (b.stat === "hull_hp" || b.stat === "shield_hp")
  );
}

function dualGateHullShieldScenarioApply(c) {
  return (
    c.defender_faction?.trim() &&
    !c.requires_morale &&
    !c.requires_defender_burning &&
    !c.requires_defender_hull_breach &&
    !c.defender_ship_class
  );
}

function defenderContextForAttackSeat(c) {
  return (
    c.defender_faction ||
    c.defender_ship_class ||
    c.requires_defender_burning ||
    c.requires_defender_hull_breach ||
    c.requires_morale
  );
}

function skippedFromFlatProfileMerge(b, c) {
  if (!isConditional(c)) return false;
  if (skipOwnerFactionMergeForDefenderGatedHullShield(b, c)) return true;
  if (c.defender_faction && DEFENDER_FACTION_SEAT_STATS.has(b.stat)) return true;
  if (
    ATTACK_SEAT_STATS.has(b.stat) &&
    (isOwnerFactionGated(c) || defenderContextForAttackSeat(c))
  ) {
    return true;
  }
  if (b.stat === "isolytic_damage" && isConditional(c)) return true;
  if (b.stat === "apex_barrier" && c.requires_morale) return true;
  return false;
}

function classifyBonus(b, rid, hasCanonical) {
  const c = bonusCondition(b);
  if (!isConditional(c)) return "unconditional_flat";

  if (hasCanonical) return "canonical_priority";

  if (
    skipOwnerFactionMergeForDefenderGatedHullShield(b, c) &&
    dualGateHullShieldScenarioApply(c)
  ) {
    return "dual_gate_hull_shield";
  }

  if (isOwnerFactionGated(c) && !c.defender_faction) {
    return "owner_faction";
  }

  if (b.stat === "isolytic_damage" && c.requires_morale) {
    return "seat_round_start_isolytic";
  }

  if (b.stat === "apex_barrier" && c.requires_morale) {
    return "seat_round_start_apex_barrier";
  }

  if (
    b.stat === "isolytic_cascade_damage" ||
    b.stat === "isolytic_cascade" ||
    (b.stat === "isolytic_damage" && isConditional(c))
  ) {
    return "seat_attack_phase_isolytic";
  }

  if (ATTACK_SEAT_STATS.has(b.stat) && defenderContextForAttackSeat(c)) {
    return "seat_attack_phase_weapon_crit";
  }

  if (c.defender_faction && DEFENDER_FACTION_SEAT_STATS.has(b.stat)) {
    return "seat_attack_phase_defender_faction";
  }

  if (skippedFromFlatProfileMerge(b, c)) {
    return "seat_other";
  }

  return "flat_merge_risk";
}

function flagshipRow(catalog, canonical, rid) {
  const rec = catalog.items.find((x) => x.rid === rid);
  const canon = canonical.overrides.find((x) => x.rid === rid);
  const routes = new Set();
  for (const lvl of rec?.levels ?? []) {
    for (const b of lvl.bonuses ?? []) {
      routes.add(classifyBonus(b, rid, Boolean(canon)));
    }
  }
  return {
    rid,
    name: rec?.name ?? canon?.name ?? null,
    canonical: Boolean(canon),
    catalog_routes: [...routes],
    snapshot_by_level: Boolean(
      canon?.effects?.some((e) => e.snapshot_by_level)
    ),
    incoming_mitigation_rounds: canon?.effects?.find(
      (e) => e.incoming_shield_mitigation_rounds
    )?.incoming_shield_mitigation_rounds,
  };
}

async function main() {
  const asJson = process.argv.includes("--json");
  const writeMd = process.argv.includes("--markdown");

  const catalog = JSON.parse(await fs.readFile(CATALOG_PATH, "utf8"));
  const canonical = JSON.parse(await fs.readFile(CANONICAL_PATH, "utf8"));
  const canonRids = new Set(canonical.overrides.map((o) => o.rid));

  const counts = {};
  const flatMergeRisks = [];
  const byRidRoute = new Map();

  for (const rec of catalog.items) {
    const hasCanon = canonRids.has(rec.rid);
    for (const lvl of rec.levels ?? []) {
      for (const b of lvl.bonuses ?? []) {
        const route = classifyBonus(b, rec.rid, hasCanon);
        counts[route] = (counts[route] ?? 0) + 1;
        if (route === "flat_merge_risk") {
          flatMergeRisks.push({
            rid: rec.rid,
            name: rec.name,
            level: lvl.level,
            stat: b.stat,
            condition: bonusCondition(b),
          });
        }
        if (!byRidRoute.has(rec.rid)) byRidRoute.set(rec.rid, new Set());
        byRidRoute.get(rec.rid).add(route);
      }
    }
  }

  const flagshipRids = [
    365419690, 2580836593, 2047743532, 4133019450, 535909811, 1995496344,
    1233598019, 3288570685, 3407808029, 851540444, 2392190200,
  ];

  const report = {
    generated: new Date().toISOString().slice(0, 10),
    catalog_projects: catalog.items.length,
    bonus_rows_total: Object.values(counts).reduce((a, n) => a + n, 0),
    canonical_overrides: canonical.overrides.length,
    routing_counts: counts,
    flat_merge_risk_count: flatMergeRisks.length,
    flat_merge_risk_sample: flatMergeRisks.slice(0, 15),
    dual_gate_hull_shield_projects: [...byRidRoute.entries()]
      .filter(([, routes]) => routes.has("dual_gate_hull_shield"))
      .map(([rid]) => rid),
    flagship_trees: flagshipRids.map((rid) => flagshipRow(catalog, canonical, rid)),
  };

  if (asJson) {
    console.log(JSON.stringify(report, null, 2));
    return;
  }

  console.error(
    `Catalog: ${report.catalog_projects} projects, ${report.bonus_rows_total} bonus rows; ` +
      `${report.canonical_overrides} canonical overrides; flat_merge_risk=${report.flat_merge_risk_count}`
  );
  console.error("Routing:", counts);

  if (writeMd) {
    const lines = [
      "# Research conditional routing inventory",
      "",
      `Auto-generated by \`node scripts/inventory_research_conditional.mjs --markdown\` on ${report.generated}.`,
      "See [research_conditional_routing.md](research_conditional_routing.md) for engine rules.",
      "",
      "## Routing counts",
      "",
      "| Route | Bonus rows |",
      "|-------|----------:|",
      ...Object.entries(counts)
        .sort((a, b) => b[1] - a[1])
        .map(([k, v]) => `| \`${k}\` | ${v} |`),
      "",
      "## Flagship conditional trees",
      "",
      "| rid | name | canonical | catalog routes | snapshot_by_level | incoming SM rounds |",
      "|----:|------|:---------:|----------------|:-----------------:|:------------------:|",
      ...report.flagship_trees.map(
        (f) =>
          `| ${f.rid} | ${f.name ?? "—"} | ${f.canonical ? "yes" : "no"} | ${f.catalog_routes.join(", ")} | ${f.snapshot_by_level ? "yes" : "—"} | ${f.incoming_mitigation_rounds ?? "—"} |`
      ),
      "",
      "## Flat-merge risks",
      "",
      report.flat_merge_risk_count === 0
        ? "None — every conditional row is skipped from unconditional `profile.bonuses` merge or handled via canonical override."
        : `**${report.flat_merge_risk_count}** rows may still flat-merge incorrectly. Sample:\n\n` +
          report.flat_merge_risk_sample
            .map(
              (r) =>
                `- rid \`${r.rid}\` (${r.name ?? "?"}), level ${r.level}, stat \`${r.stat}\`, condition ${JSON.stringify(r.condition)}`
            )
            .join("\n"),
      "",
      "## Dual-gate hull/shield projects",
      "",
      report.dual_gate_hull_shield_projects.length
        ? report.dual_gate_hull_shield_projects.map((rid) => `- \`${rid}\``).join("\n")
        : "None in catalog today (owner+defender faction on `hull_hp`/`shield_hp` requires explicit buff mappings).",
      "",
    ];
    await fs.writeFile(OUT_MD, `${lines.join("\n")}\n`, "utf8");
    console.error(`Wrote ${OUT_MD}`);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
