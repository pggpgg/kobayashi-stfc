/** Refresh only optimizer schemas from export_optimizer_schemas JSON. Run at repo root. */
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";

const requireFrontend = createRequire(path.resolve("frontend/package.json"));
const yaml = requireFrontend("js-yaml");
const input = process.argv[2];
if (!input) throw new Error("Usage: node scripts/refresh_optimizer_openapi.mjs <optimizer-schemas.json>");
const schemas = JSON.parse(fs.readFileSync(input, "utf8").replace(/^\uFEFF/, ""));
const specPath = "docs/openapi/kobayashi-openapi.yaml";
let text = fs.readFileSync(specPath, "utf8").replace(/\r\n/g, "\n");
const old = yaml.load(text).components.schemas;
const oldRow = old.OptimizeResponse.properties.recommendations.items;
const previous = {
  ...old,
  ScenarioSummary: old.ScenarioSummary ?? old.OptimizeResponse.properties.scenario,
  CrewRecommendation: old.CrewRecommendation ?? oldRow,
  RefinementDetail: old.RefinementDetail ?? oldRow.properties?.refinement,
  RefinementSlotChange: old.RefinementSlotChange ?? oldRow.properties?.refinement?.properties?.changed_slots?.items,
  OptimizerFunnelTelemetry: old.OptimizerFunnelTelemetry ?? old.OptimizeResponse.properties.scenario.properties?.optimizer_funnel,
};
const outputOnly = new Set([
  "OptimizeResponse", "OptimizeStatusResponse", "ScenarioSummary", "CrewRecommendation",
  "RefinementDetail", "RefinementSlotChange", "OptimizerFunnelTelemetry", "OptimizerRolePoolTelemetry",
]);

// These response DTOs omit None rather than serializing null. Schemars models
// Option in a schema that also serves input types; keep request nullability.
function omitOutputNull(property) {
  if (Array.isArray(property.type)) property.type = property.type.filter(t => t !== "null");
  if (Array.isArray(property.type) && property.type.length === 1) property.type = property.type[0];
  if (property.anyOf?.some(s => s.type === "null")) {
    const choices = property.anyOf.filter(s => s.type !== "null");
    if (choices.length === 1) { delete property.anyOf; Object.assign(property, choices[0]); }
  }
  if (property.default === null) delete property.default;
}

function retainAnnotations(fresh, prior) {
  if (!prior) return;
  for (const key of ["description", "minimum", "maximum", "minItems", "maxItems", "maxLength", "pattern", "enum"]) {
    if (prior[key] !== undefined) fresh[key] = prior[key];
  }
  if (fresh.enum && (fresh.type?.includes?.("null") || fresh.anyOf?.some(s => s.type === "null")) && !fresh.enum.includes(null)) fresh.enum = [...fresh.enum, null];
  for (const [key, field] of Object.entries(fresh.properties ?? {})) retainAnnotations(field, prior.properties?.[key]);
  if (fresh.items && typeof fresh.items === "object") retainAnnotations(fresh.items, prior.items);
}

for (const [name, schema] of Object.entries(schemas)) {
  retainAnnotations(schema, previous[name]);
  if (outputOnly.has(name)) for (const field of Object.values(schema.properties ?? {})) omitOutputNull(field);
}
schemas.OptimizeRequest.properties.below_decks_slots.minimum = 0;
schemas.OptimizeRequest.properties.below_decks_slots.maximum = 7;
// Preserve the documented enum for recommendation view labels.
if (previous.CrewRecommendation.properties?.pareto_tags?.items?.enum) schemas.CrewRecommendation.properties.pareto_tags.items.enum = previous.CrewRecommendation.properties.pareto_tags.items.enum;

for (const [name, schema] of Object.entries(schemas)) {
  const block = yaml.dump({ [name]: schema }, { lineWidth: 100, noRefs: true, quotingType: '"' })
    .split("\n").filter((line, i, lines) => i < lines.length - 1 || line).map(line => `    ${line}`).join("\n") + "\n\n";
  const lines = text.split("\n");
  const start = lines.findIndex(line => line === `    ${name}:`);
  if (start < 0) text = text.trimEnd() + "\n\n" + block;
  else {
    let end = start + 1;
    while (end < lines.length && !/^    [A-Za-z_][A-Za-z_0-9]*:\s*$/.test(lines[end])) end++;
    lines.splice(start, end - start, ...block.trimEnd().split("\n"), "");
    text = lines.join("\n");
  }
}
fs.writeFileSync(specPath, text.replace(/^[\t ]+$/gm, "").trimEnd() + "\n");
