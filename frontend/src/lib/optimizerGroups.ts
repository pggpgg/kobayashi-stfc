export interface OfficerGroup {
  officers: string[];
  min_count: number;
}

/** Preserve saved settings on decode errors; the editor requires explicit clearing. */
export function readOfficerGroups(raw: string): {
  groups: OfficerGroup[];
  error: string | null;
} {
  if (!raw.trim()) return { groups: [], error: null };
  try {
    const value: unknown = JSON.parse(raw);
    if (!Array.isArray(value)) throw new Error("not an array");
    const groups = value.map((item: unknown) => {
      if (!item || typeof item !== "object") throw new Error("not a group");
      const group = item as Record<string, unknown>;
      if (
        !Array.isArray(group.officers) ||
        !group.officers.every((name: unknown) => typeof name === "string") ||
        typeof group.min_count !== "number" ||
        !Number.isFinite(group.min_count)
      ) {
        throw new Error("invalid group fields");
      }
      return {
        officers: group.officers
          .map((name: string) => name.trim())
          .filter(Boolean),
        min_count: group.min_count,
      };
    });
    return { groups, error: null };
  } catch {
    return {
      groups: [],
      error:
        "Saved officer groups could not be read. Clear invalid groups to configure them again.",
    };
  }
}

export function officerGroupsError(
  groups: readonly OfficerGroup[],
): string | null {
  for (const [index, group] of groups.entries()) {
    const count = new Set(
      group.officers.map((name) => name.trim().toLowerCase()).filter(Boolean),
    ).size;
    if (!count)
      return `Officer group ${index + 1}: choose at least one officer.`;
    if (!Number.isInteger(group.min_count) || group.min_count < 1) {
      return `Officer group ${index + 1}: minimum required must be a whole number of at least 1.`;
    }
    if (group.min_count > count) {
      return `Officer group ${index + 1}: minimum required cannot exceed the number of selected officers.`;
    }
  }
  return null;
}

export function validatedOfficerGroups(
  raw: string | undefined,
): OfficerGroup[] | undefined {
  const decoded = readOfficerGroups(raw ?? "");
  const error = decoded.error ?? officerGroupsError(decoded.groups);
  if (error) throw new Error(error);
  return decoded.groups.length ? decoded.groups : undefined;
}
