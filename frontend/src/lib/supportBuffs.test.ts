import supportBuffCatalogJson from "../../../data/support_buffs.json";
import { SUPPORT_BUFF_OPTIONS } from "./supportBuffs";

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
    expect(SUPPORT_BUFF_OPTIONS).toHaveLength(4);

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
