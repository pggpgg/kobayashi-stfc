import type { EligibilityVerdict } from "../lib/api";

interface EligibilityBadgeProps {
  /** Verdict for the officer's seat ability vs the current scenario; null = none/unknown. */
  verdict: EligibilityVerdict | null;
  reason?: string;
}

/**
 * Per-officer eligibility indicator shown under a crew slot. `works`/null renders nothing; a
 * `conditional` (✴️) verdict shows a muted info chip; a `does_not_work` (➖) verdict shows a
 * prominent warning chip. The cheat-sheet reason is surfaced via tooltip/aria-label.
 */
export function EligibilityBadge({ verdict, reason }: EligibilityBadgeProps) {
  if (!verdict || verdict === "works") return null;
  const blocked = verdict === "does_not_work";
  const label = blocked ? "✕ may not work" : "✦ conditional";
  const base = blocked
    ? "Does not work against this target"
    : "Works only if conditions are met";
  const title = reason ? `${base}: ${reason}` : base;
  return (
    <span
      role="note"
      title={title}
      aria-label={title}
      style={{
        marginTop: "0.2rem",
        fontSize: "0.62rem",
        fontWeight: 600,
        lineHeight: 1.2,
        padding: "0.05rem 0.3rem",
        borderRadius: 3,
        whiteSpace: "nowrap",
        color: blocked
          ? "var(--warning, #e8952e)"
          : "var(--text-muted, #8aa6b3)",
        background: blocked
          ? "rgba(232,149,46,0.12)"
          : "rgba(138,166,179,0.10)",
        border: `1px solid ${blocked ? "var(--warning, #e8952e)" : "var(--border, #2a4a5c)"}`,
      }}
    >
      {label}
    </span>
  );
}
