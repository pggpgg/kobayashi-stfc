import { describe, expect, it } from "vitest";
import hostilesIndexJson from "../../../data/hostiles/index.json";
import type { HostileListItem } from "./api";
import {
  chainGrindForLoopGoal,
  defaultLoopVariantId,
  FUTURE_PATTERN_EXEMPTIONS,
  hostileLabel,
  hostileMatchesLoop,
  LOOP_CATALOG,
  resolveLoopHostiles,
  resolveLoopHostilesAscending,
  resolveLoopHostilesForVariant,
} from "./loopsCatalog";

/**
 * The real installed hostile catalog, not fixtures.
 *
 * A loop is only as good as its patterns still matching live upstream names, and
 * synthetic fixtures cannot catch drift by construction — every dead pattern this
 * suite found (`apex-raiders`, `eclipse`) passed the fixture-based tests.
 *
 * Note `index.json` carries `hostile_name` but not `display_name`: the server
 * resolves display names from `loca_id`. Matching here therefore runs against the
 * raw upstream name, which is the stricter of the two — a pattern that matches the
 * raw name always matches at runtime, since `hostileLabel` falls back to it.
 */
const installedHostiles: HostileListItem[] = (
  hostilesIndexJson as { hostiles: HostileListItem[] }
).hostiles;

function hostile(
  id: string,
  name: string,
  level: number,
  shipClass = "explorer",
): HostileListItem {
  return {
    id,
    hostile_name: name,
    level,
    ship_class: shipClass,
  };
}

function findLoop(id: string) {
  const loop = LOOP_CATALOG.find((candidate) => candidate.id === id);
  if (!loop) throw new Error(`Missing loop ${id}`);
  return loop;
}

describe("loops catalog", () => {
  it("matches representative researched gameplay loops", () => {
    const cases = [
      ["actian", "Actian Apex"],
      ["jem-hadar", "Jem'Hadar Fighter"],
      ["swarm", "SWARM CLUSTER"],
      ["freebooter", "Freebooter Battleship"],
      ["xindi-aquatic", "Xindi-Aquatic Cruiser"],
      ["xindi-aquatic-enhanced", "Xindi-Aquatic Enhanced Cruiser"],
      ["silent-enemies", "SILENT ENEMY"],
      ["augment-exiles", "Hijacked D7 Class"],
      ["augment-exiles", "DY-200-class"],
    ] as const;

    for (const [loopId, name] of cases) {
      const loop = findLoop(loopId);
      expect(hostileMatchesLoop(hostile("1", name, 50), loop)).toBe(true);
    }
  });

  it("keeps overlapping Xindi-Aquatic levels on separate climb ladders", () => {
    const hostiles = [
      hostile("enhanced-41", "Xindi-Aquatic Enhanced Cruiser", 41),
      hostile("cruiser-42", "Xindi-Aquatic Cruiser", 42),
      hostile("scout-34", "Xindi-Aquatic Scout", 34),
      hostile("cruiser-40", "Xindi-Aquatic Cruiser", 40),
    ];

    expect(
      resolveLoopHostilesAscending(hostiles, findLoop("xindi-aquatic")).map(
        ({ id }) => id,
      ),
    ).toEqual(["scout-34", "cruiser-40", "cruiser-42"]);
    expect(
      resolveLoopHostilesAscending(
        hostiles,
        findLoop("xindi-aquatic-enhanced"),
      ).map(({ id }) => id),
    ).toEqual(["enhanced-41"]);
  });

  it("sorts the ladder high-to-low and deduplicates equivalent upstream rows", () => {
    const loop = findLoop("freebooter");
    const result = resolveLoopHostiles(
      [
        hostile("low", "Freebooter Explorer", 43),
        hostile("high-a", "Freebooter Explorer", 57),
        hostile("high-duplicate", "Freebooter Explorer", 57),
        hostile("high-spawn-variant", "Freebooter Explorer ⇁", 57),
        hostile("other", "Federation Patrol", 60),
      ],
      loop,
    );

    expect(result.map((row) => row.id)).toEqual(["high-a", "low"]);
  });

  it("#414 selects the Yoton farming variant over the no-loot ghost regardless of catalog order", () => {
    const loop = findLoop("xindi-aquatic");
    const ghost: HostileListItem = {
      id: "3294438884",
      hostile_name: "Xindi-Aquatic Cruiser",
      display_name: "Xindi-Aquatic Cruiser",
      level: 62,
      ship_class: "interceptor",
      system_names: [],
      warp: 0,
      has_loot: false,
      primary_resource_name: null,
    };
    const yoton: HostileListItem = {
      id: "3919830203",
      hostile_name: "Xindi-Aquatic Cruiser",
      display_name: "Xindi-Aquatic Cruiser",
      level: 62,
      ship_class: "interceptor",
      system_names: ["Yoton"],
      warp: 1000,
      has_loot: true,
      primary_resource_name: "Tech Module",
    };

    const ghostFirst = resolveLoopHostiles([ghost, yoton], loop);
    const yotonFirst = resolveLoopHostiles([yoton, ghost], loop);

    expect(ghostFirst.map((row) => row.id)).toEqual(["3919830203"]);
    expect(yotonFirst.map((row) => row.id)).toEqual(["3919830203"]);
  });

  it("#415 maps repeat-kill survival and farming loot goals to distinct chain objectives", () => {
    expect(chainGrindForLoopGoal("kills_per_hull")).toEqual({
      enabled: true,
      kills_target: 5,
      secondary: "min_hull_damage",
    });
    expect(chainGrindForLoopGoal("farming_loot")).toEqual({
      enabled: true,
      kills_target: 5,
      secondary: "max_expected_loot",
    });
    expect(chainGrindForLoopGoal("one_round")).toBeUndefined();
  });

  it("does not treat Lost Gorn targets as the Gorn Hunter loop", () => {
    const loop = findLoop("gorn-hunters");
    expect(
      hostileMatchesLoop(hostile("lost", "Lost Gorn Warship", 50), loop),
    ).toBe(false);
  });

  it("orders the climb ladder lowest level first", () => {
    const loop = findLoop("actian");
    const descending = resolveLoopHostiles(
      [
        hostile("low", "Actian Chrysalis", 20),
        hostile("high", "Actian Chrysalis", 40),
      ],
      loop,
    );
    const ascending = resolveLoopHostilesAscending(
      [
        hostile("low", "Actian Chrysalis", 20),
        hostile("high", "Actian Chrysalis", 40),
      ],
      loop,
    );
    expect(ascending.map((row) => row.id)).toEqual(
      [...descending].reverse().map((row) => row.id),
    );
    expect(ascending[0]?.level).toBeLessThan(
      ascending[ascending.length - 1]?.level ?? 0,
    );
  });
});

describe("loops catalog liveness against the installed hostile catalog", () => {
  it("loads the real hostile index", () => {
    expect(installedHostiles.length).toBeGreaterThan(1000);
  });

  it.each(
    LOOP_CATALOG.map((loop) => [loop.id, loop] as const),
  )("%s resolves at least one live hostile", (_id, loop) => {
    expect(resolveLoopHostiles(installedHostiles, loop).length).toBeGreaterThan(
      0,
    );
  });

  /**
   * Per-*pattern* liveness, which is the assertion that actually catches drift: a
   * loop with several patterns stays green on the per-loop check above even when
   * individual patterns are dead, which is exactly how `eclipse` (`/eclipse/i`) and
   * `faction-armadas` (Klingon/Romulan) hid.
   */
  it.each(
    LOOP_CATALOG.flatMap((loop) =>
      (loop.hostilePatterns ?? [])
        .map((pattern, index) => ({ loop, pattern, index }))
        .filter(
          ({ index }) =>
            !(FUTURE_PATTERN_EXEMPTIONS[loop.id] ?? []).includes(index),
        )
        .map(
          ({ loop: l, pattern, index }) =>
            [`${l.id}[${index}] ${String(pattern)}`, l, pattern] as const,
        ),
    ),
  )("%s matches at least one live hostile", (_label, loop, pattern) => {
    const matched = installedHostiles.some(
      (candidate) =>
        pattern.test(hostileLabel(candidate)) &&
        !loop.excludePatterns?.some((exclude) =>
          exclude.test(hostileLabel(candidate)),
        ),
    );
    expect(matched).toBe(true);
  });

  it("resolves only the twelve requested Mirror Universe targets", () => {
    const loop = findLoop("mu-rep-grind");
    const ladder = resolveLoopHostiles(installedHostiles, loop);

    expect(ladder.map((row) => row.level)).toEqual([
      70, 68, 65, 61, 60, 58, 55, 51, 50, 48, 45, 40,
    ]);
    expect(ladder.map((row) => row.id)).toEqual([
      "383046647",
      "2376900965",
      "1381341814",
      "3162073836",
      "2578891593",
      "1267206309",
      "3496577958",
      "607623533",
      "4152973142",
      "2864818937",
      "2013385428",
      "2016644127",
    ]);
  });

  it("resolves the Frontier Raider ladder that shipped dead as 'Apex Raider'", () => {
    const loop = findLoop("apex-raiders");
    expect(resolveLoopHostiles(installedHostiles, loop).length).toBeGreaterThan(
      0,
    );
  });

  it("resolves the Augment Exiles (U.S.S. Reliant) ladder across all five Exile hostile families", () => {
    const loop = findLoop("augment-exiles");
    const ladder = resolveLoopHostiles(installedHostiles, loop);

    expect(loop.specialtyShipIds).toEqual(["uss_reliant"]);
    expect(defaultLoopVariantId(loop)).toBe("hijacked-d7");
    expect(ladder.length).toBe(96);
    expect(Math.min(...ladder.map((row) => row.level))).toBe(26);
    expect(Math.max(...ladder.map((row) => row.level))).toBe(70);
    for (const family of [
      "Hijacked D3 Class",
      "Hijacked D7 Class",
      "Hijacked Legionary",
      "Hijacked U.S.S. Mayflower",
      "DY-200-class",
    ]) {
      expect(ladder.some((row) => hostileLabel(row) === family)).toBe(true);
    }

    // Default variant is Hijacked D7 Class (23 auto-grind pursuit targets, L26..70).
    const defaultD7 = resolveLoopHostilesForVariant(installedHostiles, loop);
    expect(defaultD7.length).toBe(23);
    expect(
      defaultD7.every((row) => hostileLabel(row) === "Hijacked D7 Class"),
    ).toBe(true);

    const regular = resolveLoopHostilesForVariant(
      installedHostiles,
      loop,
      "hijacked-regular",
    );
    expect(regular.length).toBe(69);

    const dy200 = resolveLoopHostilesForVariant(
      installedHostiles,
      loop,
      "dy-200",
    );
    expect(dy200.length).toBe(4);

    const all = resolveLoopHostilesForVariant(installedHostiles, loop, "all");
    expect(all.length).toBe(96);
  });
});
