import { describe, expect, it } from "vitest";
import hostilesIndexJson from "../../../data/hostiles/index.json";
import type { HostileListItem } from "./api";
import {
  FUTURE_PATTERN_EXEMPTIONS,
  hostileLabel,
  hostileMatchesLoop,
  LOOP_CATALOG,
  resolveLoopHostiles,
  resolveLoopHostilesAscending,
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
      ["xindi-aquatic", "Xindi-Aquatic Enhanced Cruiser"],
      ["silent-enemies", "SILENT ENEMY"],
    ] as const;

    for (const [loopId, name] of cases) {
      const loop = findLoop(loopId);
      expect(hostileMatchesLoop(hostile("1", name, 50), loop)).toBe(true);
    }
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
      loop.hostilePatterns
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

  it("resolves the Frontier Raider ladder that shipped dead as 'Apex Raider'", () => {
    const loop = findLoop("apex-raiders");
    expect(resolveLoopHostiles(installedHostiles, loop).length).toBeGreaterThan(
      0,
    );
  });
});
