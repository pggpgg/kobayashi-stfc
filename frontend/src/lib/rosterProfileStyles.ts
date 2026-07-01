import type { CSSProperties } from "react";

/** Shared by RosterProfile.tsx and its extracted subcomponents (behavior-preserving). */
export const styles = {
  cellPad: { padding: "var(--space-4) var(--space-7)" },
  muted: { color: "var(--text-muted)" },
  blockGap: { marginBottom: "var(--space-9)" },
  noMargin: { margin: 0 },
  fieldLabel: { fontWeight: 600, marginBottom: "var(--space-1)" },
  bulletList: { margin: 0, paddingLeft: "var(--space-10)" },
  errorNote: { marginTop: "var(--space-7)", color: "var(--error)" },
  rowCenter: { display: "flex", alignItems: "center", gap: "var(--space-7)" },
} satisfies Record<string, CSSProperties>;
