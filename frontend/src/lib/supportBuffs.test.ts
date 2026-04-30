import supportBuffCatalogJson from "../../../data/support_buffs.json";
import {
  normalizeSupportBuffSelection,
  SUPPORT_BUFF_OPTIONS,
} from "./supportBuffs";

interface SupportBuffCatalogEntry {
  id?: string;
  label?: string;
  display_name?: string;
  description?: string;
  source?: string;
  provenance_notes?: string[];
  stat_targets?: Array<{
    stat: string;
    value: number;
    stacking: "additive" | "multiplicative";
    layer?: string;
  }>;
}

interface SupportBuffCatalog {
  buffs: Record<string, SupportBuffCatalogEntry>;
}

const supportBuffCatalog = supportBuffCatalogJson as SupportBuffCatalog;

describe("support buff catalog", () => {
  it("derives selectable options from shared catalog metadata", () => {
    expect(SUPPORT_BUFF_OPTIONS).toHaveLength(5);

    for (const option of SUPPORT_BUFF_OPTIONS) {
      const catalogEntry = supportBuffCatalog.buffs[option.id];
      expect(catalogEntry).toBeDefined();
      expect(catalogEntry.id).toBe(option.id);
      expect(option.label).toBe(catalogEntry.display_name);
      expect(option.description).toBe(catalogEntry.description);
      expect(option.source).toBe(catalogEntry.source);
      expect(option.provenanceNotes).toEqual(catalogEntry.provenance_notes);
      expect(option.provenanceNotes.length).toBeGreaterThan(0);
      expect(option.statTargets).toEqual(catalogEntry.stat_targets ?? []);

      for (const target of option.statTargets) {
        expect(target.stat.length).toBeGreaterThan(0);
        expect(Number.isFinite(target.value)).toBe(true);
        expect(["additive", "multiplicative"]).toContain(target.stacking);
        expect(target.layer).toBe("static_bonuses");
      }
    }
  });
});

describe("normalizeSupportBuffSelection", () => {
  it("drops unsupported ids and duplicate selections", () => {
    const result = normalizeSupportBuffSelection([
      "cerritos_support",
      "unknown_buff",
      "cerritos_support",
    ]);

    expect(result.ids).toEqual(["cerritos_support"]);
    expect(result.issues.map((issue) => issue.type)).toEqual([
      "unsupported",
      "duplicate",
    ]);
  });

  it("resolves incompatible exclusive groups by highest priority", () => {
    const result = normalizeSupportBuffSelection([
      "titan_a_fortification",
      "titan_a_max_fortification",
      "defiant_reinforce",
    ]);

    expect(result.ids).toEqual([
      "defiant_reinforce",
      "titan_a_max_fortification",
    ]);
    expect(result.issues).toEqual([
      expect.objectContaining({
        type: "incompatible",
        id: "titan_a_fortification",
        keptId: "titan_a_max_fortification",
      }),
    ]);
  });
});
