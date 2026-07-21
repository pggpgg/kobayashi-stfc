import { describe, expect, it } from "vitest";
import type { HostileListItem } from "./api";
import {
  hostileMatchesLoop,
  LOOP_CATALOG,
  resolveLoopHostiles,
} from "./loopsCatalog";

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
});
