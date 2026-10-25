#!/usr/bin/env node
/** Bind pre-flag normalized assets to matching cached native critical-damage values. */
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export function weaponCriticalValues(components) {
  return (components ?? [])
    .map((component, index) => ({ component, index,
      order: Number.isFinite(component.order) && component.order >= 0
        ? component.order : Number.MAX_SAFE_INTEGER }))
    .sort((a, b) => a.order - b.order || a.index - b.index)
    .map(({ component }) => component.data ?? component)
    .filter((data) => data.tag?.toLowerCase() === "weapon")
    .map((data) => data.crit_modifier ?? data.crit_damage ?? null);
}

export function hostileCriticalSourceMatches(normalized, raw) {
  if (String(raw.id) !== normalized.id || !raw.stats) return false;
  if (!Number.isFinite(raw.stats.critical_damage)
      || normalized.crit_damage !== raw.stats.critical_damage) return false;
  const source = weaponCriticalValues(raw.components);
  const current = weaponCriticalValues(normalized.components);
  return source.length > 0 && source.every(Number.isFinite)
    && JSON.stringify(source) === JSON.stringify(current);
}

export function shipCriticalSourceMatches(normalized, raw) {
  if (!normalized.tiers?.length) return false;
  return normalized.tiers.every((tier) => {
    const source = raw.tiers?.find((candidate) => candidate.tier === tier.tier);
    if (!source) return false;
    const values = weaponCriticalValues(source.components);
    return values.length > 0 && values.every(Number.isFinite)
      && tier.crit_damage === values[0]
      && JSON.stringify((tier.weapons ?? []).map((weapon) => weapon.crit_multiplier ?? null))
        === JSON.stringify(values);
  });
}

export function buildManifest(root) {
  const read = (relative) => JSON.parse(fs.readFileSync(path.join(root, relative), "utf8"));
  const shipIds = [];
  const hostileIds = [];
  const bindings = [];
  const registry = read("data/upstream/data-stfc-space/ship_id_registry.json");
  for (const ship of [...registry.ships].sort((a, b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0)) {
    const normalizedPath = `data/ships_extended/${ship.id}.json`;
    const sourcePath = `data/upstream/data-stfc-space/ships/${ship.numeric_id}.json`;
    if (!fs.existsSync(path.join(root, normalizedPath)) || !fs.existsSync(path.join(root, sourcePath))) continue;
    const normalized = read(normalizedPath), raw = read(sourcePath);
    if (!shipCriticalSourceMatches(normalized, raw)) continue;
    shipIds.push(ship.id);
    bindings.push([normalizedPath, sourcePath, normalized.tiers.map((tier) =>
      [tier.tier, tier.crit_damage, tier.weapons.map((weapon) => weapon.crit_multiplier)])]);
  }
  for (const file of fs.readdirSync(path.join(root, "data/hostiles")).sort()) {
    if (!/^\d+\.json$/.test(file)) continue;
    const normalizedPath = `data/hostiles/${file}`;
    const sourcePath = `data/upstream/data-stfc-space/hostiles/${file}`;
    if (!fs.existsSync(path.join(root, sourcePath))) continue;
    const normalized = read(normalizedPath), raw = read(sourcePath);
    if (!hostileCriticalSourceMatches(normalized, raw)) continue;
    hostileIds.push(normalized.id);
    bindings.push([normalizedPath, sourcePath, normalized.crit_damage,
      weaponCriticalValues(normalized.components)]);
  }
  return {
    schema_version: 1,
    critical_damage_units: "additive_bonus_over_noncritical_hit",
    source_note: "Existing normalized assets whose scalar and every weapon critical value match their cached native source; explicit record flags take precedence. Native ThreatConfig DPR uses base*(1+chance*critical_bonus).",
    source_bindings_sha256: crypto.createHash("sha256").update(JSON.stringify(bindings)).digest("hex"),
    ship_ids: shipIds,
    hostile_ids: hostileIds,
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const manifest = buildManifest(root);
  fs.writeFileSync(path.join(root, "data/native_critical_damage_sources.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`Native critical sources: ${manifest.ship_ids.length} ships, ${manifest.hostile_ids.length} hostiles`);
}
