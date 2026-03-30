/** Alliance / ship support buffs selectable in the workspace (engine wiring TBD). */
export const SUPPORT_BUFF_OPTIONS = [
  { id: "titan_a_fortification", label: "Titan-A Fortification" },
  { id: "titan_a_max_fortification", label: "Titan-A Max Fortification" },
  { id: "cerritos_support", label: "Cerritos Support" },
  { id: "defiant_reinforce", label: "Defiant Reinforce" },
] as const;

export type SupportBuffId = (typeof SUPPORT_BUFF_OPTIONS)[number]["id"];
