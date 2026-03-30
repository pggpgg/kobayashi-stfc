/** Alliance / ship support buffs selectable in the workspace (sent to the API as `support_buffs`). */
export const SUPPORT_BUFF_OPTIONS = [
  {
    id: "titan_a_fortification",
    label: "Titan-A Fortification",
    description:
      "Fortifies your ships and 2–13 alliance ships: +25% Critical Hit Damage; further bonuses scale with your Titan-A research (profile).",
  },
  {
    id: "titan_a_max_fortification",
    label: "Titan-A Max Fortification",
    description:
      "Max fortification: all Fortified effects, +250% base weapon damage, plus bonuses from your Titan-A research (profile).",
  },
  { id: "cerritos_support", label: "Cerritos Support" },
  { id: "defiant_reinforce", label: "Defiant Reinforce" },
] as const;

export type SupportBuffId = (typeof SUPPORT_BUFF_OPTIONS)[number]["id"];
