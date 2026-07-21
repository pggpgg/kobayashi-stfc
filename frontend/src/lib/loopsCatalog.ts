import type { HostileListItem } from "./api";

export type LoopGoalId =
  | "one_round"
  | "damage_dealt"
  | "no_hits"
  | "kills_per_hull"
  | "smallest_ship";

export interface LoopGoal {
  id: LoopGoalId;
  label: string;
  shortLabel: string;
  description: string;
}

export const LOOP_GOALS: readonly LoopGoal[] = [
  {
    id: "one_round",
    label: "One-round kills",
    shortLabel: "R1 kill",
    description: "Maximize the chance that the target dies in round one.",
  },
  {
    id: "damage_dealt",
    label: "Most damage dealt",
    shortLabel: "Damage",
    description:
      "Push the target hull as low as possible, even before a kill is reliable.",
  },
  {
    id: "no_hits",
    label: "No hits received",
    shortLabel: "Untouched",
    description: "Favor winning crews that preserve the most hull.",
  },
  {
    id: "kills_per_hull",
    label: "Most kills per hull",
    shortLabel: "Grind",
    description:
      "Use chain-grind simulation and favor repeat kills with low hull loss.",
  },
  {
    id: "smallest_ship",
    label: "Smallest viable ship",
    shortLabel: "Smallest ship",
    description: "Track the lowest ship tier and level that can win reliably.",
  },
] as const;

export type LoopEngagement = "hostile" | "armada" | "solo_armada";
export type LoopShipPolicy = "required" | "recommended" | "open";

export interface LoopDefinition {
  id: string;
  name: string;
  summary: string;
  progression: string;
  engagement: LoopEngagement;
  minOps?: number;
  hostilePatterns: readonly RegExp[];
  excludePatterns?: readonly RegExp[];
  shipPolicy: LoopShipPolicy;
  specialtyShipIds: readonly string[];
  specialtyShipLabel?: string;
  goals: readonly LoopGoalId[];
  sourceUrl?: string;
}

const ALL_GOALS: readonly LoopGoalId[] = [
  "one_round",
  "damage_dealt",
  "no_hits",
  "kills_per_hull",
  "smallest_ship",
];

/**
 * Data-driven loop catalog. Hostile ids are deliberately not embedded here: the
 * ladder is resolved from the installed hostile catalog, so upstream refreshes can
 * add levels without a frontend release.
 */
export const LOOP_CATALOG: readonly LoopDefinition[] = [
  {
    id: "faction-armadas",
    name: "Faction armadas",
    summary: "Federation, Klingon, and Romulan armada progression.",
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
    name: "Eclipse & Exchange",
    summary: "Eclipse hostiles and Exchange armada targets in Rogue space.",
    progression: "Eclipse security codes, Rogue research, and Stella particles",
    engagement: "hostile",
    minOps: 27,
    hostilePatterns: [/exchange (transport|bank|vault)/i, /eclipse/i],
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
    id: "xindi-aquatic",
    name: "Xindi-Aquatic",
    summary: "Delphic Expanse targets built around the NX-01.",
    progression: "NX-01 refinery, ship upgrades, and Ex-Borg extension rewards",
    engagement: "hostile",
    minOps: 40,
    hostilePatterns: [/xindi-aquatic/i],
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
    name: "Apex Raiders",
    summary: "Apex Raider targets built around chain-clearing combat.",
    progression: "GS-31 parts, research, and wave-defense progression",
    engagement: "hostile",
    hostilePatterns: [/apex raider/i],
    shipPolicy: "recommended",
    specialtyShipIds: ["gs_31"],
    specialtyShipLabel: "GS-31",
    goals: ALL_GOALS,
    sourceUrl:
      "https://startrekfleetcommand.com/news/gs-31-ship-parts-refinery-the-best-source-of-ship-parts/",
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
  const label = hostileLabel(hostile);
  if (loop.excludePatterns?.some((pattern) => pattern.test(label)))
    return false;
  return loop.hostilePatterns.some((pattern) => pattern.test(label));
}

export function resolveLoopHostiles(
  hostiles: readonly HostileListItem[],
  loop: LoopDefinition,
): HostileListItem[] {
  const seen = new Set<string>();
  return hostiles
    .filter((hostile) => hostileMatchesLoop(hostile, loop))
    .filter((hostile) => {
      const key = `${hostileGroupingLabel(hostile).toLocaleLowerCase()}|${hostile.level}|${hostile.ship_class}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    })
    .sort(
      (a, b) =>
        b.level - a.level ||
        hostileLabel(a).localeCompare(hostileLabel(b), undefined, {
          sensitivity: "base",
        }) ||
        a.ship_class.localeCompare(b.ship_class),
    );
}

export function loopGoal(goalId: LoopGoalId): LoopGoal {
  return LOOP_GOALS.find((goal) => goal.id === goalId) ?? LOOP_GOALS[0];
}
