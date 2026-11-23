import type { ChainGrindRequestBody, HostileListItem } from "./api";

export type LoopGoalId =
  | "one_round"
  | "damage_dealt"
  | "no_hits"
  | "kills_per_hull"
  | "farming_loot"
  | "smallest_ship";

export interface LoopGoal {
  id: LoopGoalId;
  label: string;
  shortLabel: string;
  description: string;
  /**
   * Whether the optimizer's built-in win-rate-first pruning works against this
   * ranking, so an exhaustive search should be requested when it is affordable.
   */
  requestsExhaustiveSearch?: boolean;
}

/**
 * What the player is ranking crews by.
 *
 * Called a *ranking* preference rather than an "optimization goal" on purpose.
 * `kills_per_hull` and `farming_loot` change what the server actually searches for
 * (turning on chain-grind simulation for survival vs expected resource payout); the
 * rest re-rank the crews the optimizer already found.
 * That distinction matters because the optimizer's internal fitness is a fixed blend
 * of win rate and hull remaining, so genetic and tiered search *prune* toward that
 * before any client-side re-rank — a crew excellent at round-one kills but middling
 * on win rate can be discarded before it is ever simulated.
 *
 * `requestsExhaustiveSearch` marks the goals where that pruning actively fights the
 * ranking, so the climb/handoff can ask for an exhaustive search when the scenario is
 * small enough to afford one, making the re-rank meaningful instead of decorative.
 */
export const LOOP_GOALS: readonly LoopGoal[] = [
  {
    id: "one_round",
    label: "Rank by one-round kills",
    shortLabel: "R1 kill",
    description:
      "Ranks the crews found by round-one kill rate. Searches exhaustively when the scenario is small enough; otherwise the default win-rate-first search may never surface the true best round-one crew.",
    requestsExhaustiveSearch: true,
  },
  {
    id: "damage_dealt",
    label: "Rank by damage dealt",
    shortLabel: "Damage",
    description:
      "Ranks by how low the target's hull ends up, useful before a kill is reliable. Searches exhaustively when affordable, for the same reason as round-one kills.",
    requestsExhaustiveSearch: true,
  },
  {
    id: "no_hits",
    label: "Rank by hull preserved",
    shortLabel: "Untouched",
    description:
      "Prefers winning crews that keep the most hull. This matches what the optimizer already searches for, so the ranking is reliable.",
  },
  {
    id: "kills_per_hull",
    label: "Optimize for repeat kills (survival)",
    shortLabel: "Grind",
    description:
      "Runs chain-grind simulation for consecutive kills with hull carrying over, maximizing chain completion rate and ending hull remaining.",
  },
  {
    id: "farming_loot",
    label: "Optimize for farming loot",
    shortLabel: "Farming loot",
    description:
      "Runs chain-grind simulation for consecutive kills and maximizes expected primary resource payout — accounting for hostile drops, applicable officer loot bonuses, and ship cargo capacity — while keeping chain completion rate visible.",
  },
  {
    id: "smallest_ship",
    label: "Rank by smallest viable ship",
    shortLabel: "Smallest ship",
    description:
      "Among results that still win, prefers the lowest ship tier and level. Note it ranks the ship you selected — it does not yet search across your other ships.",
  },
] as const;

export type LoopEngagement = "hostile" | "armada" | "solo_armada";
export type LoopShipPolicy = "required" | "recommended" | "open";

interface LoopSystem {
  id: number;
  name: string;
}

export type LoopDefinition = {
  id: string;
  name: string;
  summary: string;
  progression: string;
  engagement: LoopEngagement;
  minOps?: number;
  shipPolicy: LoopShipPolicy;
  specialtyShipIds: readonly string[];
  specialtyShipLabel?: string;
  goals: readonly LoopGoalId[];
  sourceUrl?: string;
} & (
  | {
      hostilePatterns: readonly RegExp[];
      excludePatterns?: readonly RegExp[];
      hostileSystems?: never;
    }
  | {
      hostilePatterns?: never;
      excludePatterns?: never;
      hostileSystems: ReadonlyMap<string, LoopSystem>;
    }
);

export const XINDI_AQUATIC_LOOP_ID = "xindi-aquatic";
export const XINDI_AQUATIC_ENHANCED_LOOP_ID = "xindi-aquatic-enhanced";
const XINDI_AQUATIC_ENHANCED_CRUISER = /xindi-aquatic enhanced cruiser/i;

export function loopIdForSavedTarget(
  loopId: string,
  targetName: string,
): string {
  return loopId === XINDI_AQUATIC_LOOP_ID &&
    XINDI_AQUATIC_ENHANCED_CRUISER.test(targetName)
    ? XINDI_AQUATIC_ENHANCED_LOOP_ID
    : loopId;
}

const ALL_GOALS: readonly LoopGoalId[] = [
  "one_round",
  "damage_dealt",
  "no_hits",
  "kills_per_hull",
  "farming_loot",
  "smallest_ship",
];

/**
 * Default chain-grind API parameters for chain-backed loop goals.
 * - `kills_per_hull`: repeat-kill survival (`min_hull_damage` — real ending hull %).
 * - `farming_loot`: expected resource payout (`max_expected_loot` — drops, officer bonuses & cargo).
 */
export function chainGrindForLoopGoal(
  goalId: LoopGoalId,
  killsTarget = 5,
): ChainGrindRequestBody | undefined {
  if (goalId === "kills_per_hull") {
    return {
      enabled: true,
      kills_target: killsTarget,
      secondary: "min_hull_damage",
    };
  }
  if (goalId === "farming_loot") {
    return {
      enabled: true,
      kills_target: killsTarget,
      secondary: "max_expected_loot",
    };
  }
  return undefined;
}

/**
 * Patterns that are allowed to match zero hostiles in the installed catalog,
 * because the content is not published upstream yet — keyed by loop id, holding
 * indices into that loop's `hostilePatterns`.
 *
 * Everything *not* listed here must match at least one hostile or
 * `loopsCatalog.test.ts` fails. That distinction matters: a pattern matching
 * nothing is normally a typo (see `apex-raiders`, which shipped as "Apex Raider"
 * and silently rendered an empty ladder), but Klingon and Romulan War Armadas
 * genuinely do not exist in the community catalog — only six armada families do.
 * Without this split the liveness test would have to choose between failing on
 * legitimate future-proofing and missing real typos.
 */
export const FUTURE_PATTERN_EXEMPTIONS: Record<string, readonly number[]> = {
  // 1 = /klingon war armada/i, 2 = /romulan war armada/i
  "faction-armadas": [1, 2],
};

/**
 * Ladders resolve against the installed hostile catalog. Name patterns include
 * new levels added to a family on refresh. System-specific ladders use exact
 * hostile IDs because the hostile list API omits spawn locations.
 */
export const LOOP_CATALOG: readonly LoopDefinition[] = [
  {
    id: "faction-armadas",
    name: "Faction armadas",
    // Only Federation War Armadas are published upstream today; the Klingon and
    // Romulan patterns below are kept for when they appear but currently resolve
    // nothing, so the summary must not promise them.
    summary: "War armada progression. Federation targets only in current data.",
    progression: "Faction credits, reputation, materials, and ship progression",
    engagement: "armada",
    hostilePatterns: [
      /federation war armada/i,
      /klingon war armada/i,
      /romulan war armada/i,
    ],
    shipPolicy: "open",
    specialtyShipIds: [],
    specialtyShipLabel: "Bring the strongest suitable armada ship",
    goals: ALL_GOALS,
  },
  {
    id: "actian",
    name: "Actian",
    summary: "Hunt the Actian Brood and climb from Chrysalis to Apex targets.",
    progression: "Actian Venom, Syndicate XP, and SNW officer sourcing",
    engagement: "hostile",
    minOps: 33,
    hostilePatterns: [/actian (chrysalis|instigator|apex)/i],
    shipPolicy: "recommended",
    specialtyShipIds: ["mantis"],
    specialtyShipLabel: "Mantis",
    goals: ALL_GOALS,
    sourceUrl: "https://startrekfleetcommand.com/news/update-45-patch-notes/",
  },
  {
    id: "jem-hadar",
    name: "Jem’Hadar & Dominion",
    summary: "Dominion hostiles and solo-armada progression in Dominion space.",
    progression:
      "Bajoran faction store, favors, Edicts, and Defiant progression",
    engagement: "hostile",
    minOps: 34,
    hostilePatterns: [/jem['’]?hadar/i],
    shipPolicy: "recommended",
    specialtyShipIds: ["uss_defiant"],
    specialtyShipLabel:
      "Defiant for its loop bonuses; strongest ship for hostiles",
    goals: ALL_GOALS,
    sourceUrl:
      "https://startrekfleetcommand.com/news/update-47-deep-space-nine-pt-2/",
  },
  {
    id: "swarm",
    name: "Swarm",
    summary: "Daily and weekly Swarm hostile progression.",
    progression: "Frequency modulators and Franklin / Franklin-A upgrades",
    engagement: "hostile",
    minOps: 17,
    hostilePatterns: [/swarm (cluster|aggress?or|horde)/i],
    shipPolicy: "recommended",
    specialtyShipIds: ["uss_franklin", "uss_franklin_a"],
    specialtyShipLabel: "Franklin or Franklin-A",
    goals: ALL_GOALS,
    sourceUrl: "https://startrekfleetcommand.com/news/patch-29-release-notes/",
  },
  {
    id: "freebooter",
    name: "Freebooter",
    summary: "Ex-Borg space hostiles across the combat triangle.",
    progression: "Ex-Borg reputation, store rewards, and efficiency research",
    engagement: "hostile",
    minOps: 38,
    hostilePatterns: [/freebooter/i],
    shipPolicy: "open",
    specialtyShipIds: [],
    specialtyShipLabel: "Use your strongest appropriate ship",
    goals: ALL_GOALS,
    sourceUrl: "https://startrekfleetcommand.com/news/update-52-patch-notes/",
  },
  {
    id: "borg-probes",
    name: "Borg probes",
    summary: "Tactical Probe hunting in Borg systems.",
    progression: "Inert nanoprobes, Borg officers, and Vi’Dar progression",
    engagement: "hostile",
    minOps: 25,
    hostilePatterns: [/borg (drone|assailant|tactical probe)/i],
    shipPolicy: "recommended",
    specialtyShipIds: ["vi_dar", "vi_dar_talios"],
    specialtyShipLabel: "Vi’Dar or Vi’Dar Talios",
    goals: ALL_GOALS,
    sourceUrl: "https://startrekfleetcommand.com/news/patch-16-release-notes/",
  },
  {
    id: "eclipse",
    name: "Exchange convoys",
    // `/eclipse/i` matched nothing in the installed catalog — Eclipse hostiles are
    // not published under that name upstream — so the loop is named for what it
    // actually resolves rather than implying Eclipse targets are listed.
    summary: "Exchange transport, bank, and vault targets in Rogue space.",
    progression: "Eclipse security codes, Rogue research, and Stella particles",
    engagement: "hostile",
    minOps: 27,
    hostilePatterns: [/exchange (transport|bank|vault)/i],
    shipPolicy: "recommended",
    specialtyShipIds: ["stella"],
    specialtyShipLabel: "Stella",
    goals: ALL_GOALS,
    sourceUrl: "https://startrekfleetcommand.com/news/patch-d20-release-notes/",
  },
  {
    id: "voyager",
    name: "Voyager",
    summary:
      "Hirogen hunting and revealed Species 8472 targets in the Delta Quadrant.",
    progression:
      "Hirogen relics, biotoxins, anomaly samples, and Voyager parts",
    engagement: "hostile",
    minOps: 34,
    hostilePatterns: [/hirogen/i, /species 8472/i],
    shipPolicy: "recommended",
    specialtyShipIds: ["uss_voyager"],
    specialtyShipLabel: "Voyager",
    goals: ALL_GOALS,
    sourceUrl:
      "https://startrekfleetcommand.com/news/update-55-voyager-part-2/",
  },
  {
    id: "texas-class",
    name: "Texas-class",
    summary: "Texas-class hostiles and Automated Shipyard progression.",
    progression: "Queen’s Favors, Monaveen items, and Shipyard directives",
    engagement: "hostile",
    minOps: 40,
    hostilePatterns: [/texas-class/i, /automated shipyard/i],
    shipPolicy: "recommended",
    specialtyShipIds: ["monaveen"],
    specialtyShipLabel: "Monaveen",
    goals: ALL_GOALS,
    sourceUrl:
      "https://startrekfleetcommand.com/news/update-58-lower-decks-ii-part-1/",
  },
  {
    id: "gorn-hunters",
    name: "Gorn Hunters",
    summary: "Isolytic combat against Gorn Hunter hostiles.",
    progression: "Isomatter refinery, artifacts, and Eviscerator upgrades",
    engagement: "hostile",
    minOps: 40,
    hostilePatterns: [/gorn hunter/i],
    excludePatterns: [/lost gorn/i],
    shipPolicy: "recommended",
    specialtyShipIds: ["gorn_eviscerator"],
    specialtyShipLabel: "Gorn Eviscerator (or another strong isolytic setup)",
    goals: ALL_GOALS,
    sourceUrl:
      "https://startrekfleetcommand.com/news/a-new-threat-in-star-trek-fleet-command-the-gorn-hunter-hostiles/",
  },
  {
    id: XINDI_AQUATIC_LOOP_ID,
    name: "Xindi-Aquatic",
    summary: "Regular cruisers and scouts for the NX-01.",
    progression: "NX-01 refinery, ship upgrades, and Ex-Borg extension rewards",
    engagement: "hostile",
    minOps: 40,
    hostilePatterns: [/xindi-aquatic/i],
    excludePatterns: [XINDI_AQUATIC_ENHANCED_CRUISER],
    shipPolicy: "required",
    specialtyShipIds: ["enterprise_nx_01"],
    specialtyShipLabel: "Enterprise NX-01",
    goals: ALL_GOALS,
    sourceUrl: "https://startrekfleetcommand.com/news/enterprise-nx-01/",
  },
  {
    id: XINDI_AQUATIC_ENHANCED_LOOP_ID,
    name: "Xindi-Aquatic Enhanced Cruisers",
    summary: "Enhanced battleships for the NX-01.",
    progression: "NX-01 refinery, ship upgrades, and Ex-Borg extension rewards",
    engagement: "hostile",
    minOps: 40,
    hostilePatterns: [XINDI_AQUATIC_ENHANCED_CRUISER],
    shipPolicy: "required",
    specialtyShipIds: ["enterprise_nx_01"],
    specialtyShipLabel: "Enterprise NX-01",
    goals: ALL_GOALS,
    sourceUrl: "https://startrekfleetcommand.com/news/enterprise-nx-01/",
  },
  {
    id: "silent-enemies",
    name: "Silent Enemies",
    summary: "High-critical-damage hostiles in Silent Nebula systems.",
    progression: "Temporal Artifact progression and grade materials",
    engagement: "hostile",
    minOps: 38,
    hostilePatterns: [/silent enemy/i],
    shipPolicy: "open",
    specialtyShipIds: [],
    specialtyShipLabel: "Use your strongest survivable ship",
    goals: ALL_GOALS,
    sourceUrl: "https://startrekfleetcommand.com/news/update-64-patch-notes/",
  },
  {
    id: "formation-armadas",
    name: "Formation armadas",
    summary: "Formation and Rare Formation Armada targets.",
    progression: "Artifact and alliance armada rewards",
    engagement: "armada",
    hostilePatterns: [/formation armada/i],
    shipPolicy: "open",
    specialtyShipIds: [],
    specialtyShipLabel: "Coordinate the strongest suitable armada ship",
    goals: ALL_GOALS,
  },
  {
    id: "borg-solo-armadas",
    name: "Borg solo armadas",
    summary: "Borg spheres, polygons, types, and solo outpost targets.",
    progression: "Borg Cube and solo-armada progression",
    engagement: "solo_armada",
    hostilePatterns: [
      /borg (sphere|polygon|type 03|solo outpost|recon sphere)/i,
      /conqueror borg solo armada/i,
    ],
    shipPolicy: "recommended",
    specialtyShipIds: ["borg_cube"],
    specialtyShipLabel: "Borg Cube is encouraged where its loop bonuses apply",
    goals: ALL_GOALS,
  },
  {
    id: "aggregation",
    name: "Aggregation",
    summary: "Hyperthermic Decay hostiles and building-gated progression.",
    progression: "Recon Locus, Aggregation research, and Prototype Tech",
    engagement: "hostile",
    minOps: 45,
    hostilePatterns: [/aggregation/i],
    shipPolicy: "open",
    specialtyShipIds: ["uss_dauntless"],
    specialtyShipLabel: "Recon Locus progression is the key requirement",
    goals: ALL_GOALS,
    sourceUrl:
      "https://startrekfleetcommand.com/news/patch-notes-villains-pt-1/",
  },
  {
    id: "apex-raiders",
    name: "Frontier Raiders",
    // Named "Apex Raider" here originally, which matches zero hostiles: upstream
    // calls these "Frontier Raider" / "Frontier Raider HAZ". The loop rendered an
    // empty ladder for every player until this was corrected.
    summary: "Frontier Raider targets built around chain-clearing combat.",
    progression: "GS-31 parts, research, and wave-defense progression",
    engagement: "hostile",
    hostilePatterns: [/frontier raider/i],
    shipPolicy: "recommended",
    specialtyShipIds: ["gs_31"],
    specialtyShipLabel: "GS-31",
    goals: ALL_GOALS,
    sourceUrl:
      "https://startrekfleetcommand.com/news/gs-31-ship-parts-refinery-the-best-source-of-ship-parts/",
  },
  {
    id: "mu-rep-grind",
    name: "MU Rep Grind",
    summary:
      "Hunt Imperial Starfleet Planet Harvesters and Romulan Republic Miners in the Mirror Universe.",
    progression: "Federation, Klingon, and Romulan reputation",
    engagement: "hostile",
    minOps: 45,
    // Hostile-to-system IDs from the STFC.space system summary.
    hostileSystems: new Map([
      ["2016644127", { id: 1578320633, name: "Mirror Tau Ceti" }],
      ["2013385428", { id: 22288656, name: "Mirror Doloran" }],
      ["2864818937", { id: 1301278521, name: "Mirror Dessica" }],
      ["4152973142", { id: 1776591157, name: "Mirror Altanea" }],
      ["607623533", { id: 538197195, name: "Mirror P'Jem [Extract]" }],
      ["3496577958", { id: 1901010066, name: "Mirror Rada Ban" }],
      ["1267206309", { id: 559466032, name: "Mirror Romii [Extract]" }],
      ["2578891593", { id: 853741050, name: "Mirror Cheron" }],
      ["3162073836", { id: 929779479, name: "Mirror Pareleon" }],
      ["1381341814", { id: 2135389091, name: "Mirror Vulcan [Extract]" }],
      ["2376900965", { id: 790704090, name: "Mirror Terix" }],
      ["383046647", { id: 1784890032, name: "Mirror Romulus" }],
    ]),
    shipPolicy: "required",
    specialtyShipIds: ["uss_dauntless"],
    specialtyShipLabel: "USS Dauntless",
    goals: ALL_GOALS,
    sourceUrl:
      "https://startrekfleetcommand.com/news/the-u-s-s-dauntless-revolutionizing-the-hostile-hunt/",
  },
  {
    id: "academy-drones",
    name: "Academy drones",
    summary: "Training Drone hostiles tied to Academy Space progression.",
    progression: "Remote Campus, Academy refinery, and Duo Wave Defense",
    engagement: "hostile",
    minOps: 61,
    hostilePatterns: [/academy training drone/i],
    shipPolicy: "open",
    specialtyShipIds: [],
    specialtyShipLabel:
      "Remote Campus progression matters more than a single ship",
    goals: ALL_GOALS,
    sourceUrl:
      "https://startrekfleetcommand.com/news/starfleet-academy-remote-campus-critical-mitigation/",
  },
] as const;

export function hostileLabel(hostile: HostileListItem): string {
  return hostile.display_name ?? hostile.hostile_name;
}

/** Upstream uses trailing arrow glyphs for spawn variants of the same target. */
function hostileGroupingLabel(hostile: HostileListItem): string {
  return hostileLabel(hostile)
    .replace(/[\s↿⇁⇀⇂]+$/u, "")
    .trim();
}

export function hostileMatchesLoop(
  hostile: HostileListItem,
  loop: LoopDefinition,
): boolean {
  if (loop.hostileSystems) return loop.hostileSystems.has(hostile.id);
  const label = hostileLabel(hostile);
  if (loop.excludePatterns?.some((pattern) => pattern.test(label)))
    return false;
  return loop.hostilePatterns.some((pattern) => pattern.test(label));
}

function compareHostileIdStable(a: string, b: string): number {
  const aNum = /^\d+$/.test(a) ? BigInt(a) : null;
  const bNum = /^\d+$/.test(b) ? BigInt(b) : null;
  if (aNum != null && bNum != null) {
    return aNum < bNum ? -1 : aNum > bNum ? 1 : 0;
  }
  return a.localeCompare(b);
}

/**
 * Order-independent preference among catalog variants sharing `(name, level, ship_class)`:
 * prefer real spawned farming targets (with spawn systems, positive drops, positive warp,
 * and canonical non-arrow names) over no-system/no-loot upstream ghost entries (issue #414).
 * Returns `< 0` when `a` is strictly preferred over `b`.
 */
function compareLoopHostileCandidate(
  a: HostileListItem,
  b: HostileListItem,
): number {
  const aSystems = (a.system_names?.length ?? 0) > 0 ? 1 : 0;
  const bSystems = (b.system_names?.length ?? 0) > 0 ? 1 : 0;
  if (aSystems !== bSystems) return bSystems - aSystems;

  const lootScore = (h: HostileListItem) =>
    h.has_loot === true ? 2 : h.has_loot === false ? 0 : 1;
  const aLoot = lootScore(a);
  const bLoot = lootScore(b);
  if (aLoot !== bLoot) return bLoot - aLoot;

  const aWarp = (a.warp ?? 0) > 0 ? 1 : 0;
  const bWarp = (b.warp ?? 0) > 0 ? 1 : 0;
  if (aWarp !== bWarp) return bWarp - aWarp;

  const aClean = hostileLabel(a) === hostileGroupingLabel(a) ? 1 : 0;
  const bClean = hostileLabel(b) === hostileGroupingLabel(b) ? 1 : 0;
  if (aClean !== bClean) return bClean - aClean;

  return compareHostileIdStable(a.id, b.id);
}

export function resolveLoopHostiles(
  hostiles: readonly HostileListItem[],
  loop: LoopDefinition,
): HostileListItem[] {
  const bestByKey = new Map<string, HostileListItem>();
  for (const hostile of hostiles) {
    if (!hostileMatchesLoop(hostile, loop)) continue;
    const key = loop.hostileSystems
      ? hostile.id
      : `${hostileGroupingLabel(hostile).toLocaleLowerCase()}|${hostile.level}|${hostile.ship_class}`;
    const existing = bestByKey.get(key);
    if (!existing || compareLoopHostileCandidate(hostile, existing) < 0) {
      bestByKey.set(key, hostile);
    }
  }
  return [...bestByKey.values()].sort(
    (a, b) =>
      b.level - a.level ||
      hostileLabel(a).localeCompare(hostileLabel(b), undefined, {
        sensitivity: "base",
      }) ||
      a.ship_class.localeCompare(b.ship_class) ||
      compareHostileIdStable(a.id, b.id),
  );
}

/**
 * Ladder order for *climbing*, lowest level first.
 *
 * [`resolveLoopHostiles`] sorts descending because the ladder is displayed with the
 * hardest target at the top. Progression runs the other way, so the climb runner
 * and frontier logic need the reverse — kept as a separate function rather than
 * changing the display sort.
 */
export function resolveLoopHostilesAscending(
  hostiles: readonly HostileListItem[],
  loop: LoopDefinition,
): HostileListItem[] {
  return resolveLoopHostiles(hostiles, loop).reverse();
}

export function loopGoal(goalId: LoopGoalId): LoopGoal {
  return LOOP_GOALS.find((goal) => goal.id === goalId) ?? LOOP_GOALS[0];
}
