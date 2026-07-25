import type { RungStatusInfo } from "../lib/loopRungStatus";

interface RungStatusBadgeProps {
  info: RungStatusInfo;
  /** Marks the rung the player should attack next. */
  isNext?: boolean;
}

const PRESENTATION: Record<
  RungStatusInfo["status"],
  { label: string; color: string; background: string; border: string }
> = {
  cleared: {
    label: "✓ cleared",
    color: "var(--success, #4fb477)",
    background: "rgba(79,180,119,0.12)",
    border: "var(--success, #4fb477)",
  },
  contested: {
    label: "◐ contested",
    color: "var(--warning, #e8952e)",
    background: "rgba(232,149,46,0.12)",
    border: "var(--warning, #e8952e)",
  },
  untried: {
    label: "· untried",
    color: "var(--text-muted, #8aa6b3)",
    background: "rgba(138,166,179,0.10)",
    border: "var(--border, #2a4a5c)",
  },
  locked: {
    label: "🔒 locked",
    color: "var(--text-muted, #8aa6b3)",
    background: "transparent",
    border: "var(--border, #2a4a5c)",
  },
};

function describe(info: RungStatusInfo): string {
  const pct = (value: number) => `${(value * 100).toFixed(1)}%`;
  switch (info.status) {
    case "cleared":
      return info.metric
        ? `Cleared — ${pct(info.metric.value)} (95% CI ${pct(info.metric.ciLow)}–${pct(info.metric.ciHigh)}) for the selected goal`
        : "Cleared";
    case "contested":
      return info.metric
        ? `Not clear yet — ${pct(info.metric.value)} (95% CI ${pct(info.metric.ciLow)}–${pct(info.metric.ciHigh)}); the lower bound is below this goal's bar`
        : "Not clear yet";
    case "untried":
      return "No result recorded for this rung yet";
    case "locked":
      return "The rung below this one is not cleared yet";
  }
}

/**
 * Per-rung progress chip for the loops ladder.
 *
 * Deliberately reports the goal's measured interval in the tooltip rather than only
 * a colour: "cleared" is a claim about the interval's lower bound, and a player
 * comparing two rungs needs to see how much of that is confidence versus margin.
 */
export function RungStatusBadge({ info, isNext }: RungStatusBadgeProps) {
  const presentation = PRESENTATION[info.status];
  const title = describe(info);
  return (
    <span
      style={{ display: "inline-flex", gap: "0.25rem", alignItems: "center" }}
    >
      <span
        role="note"
        title={title}
        aria-label={title}
        style={{
          fontSize: "0.62rem",
          fontWeight: 600,
          lineHeight: 1.2,
          padding: "0.05rem 0.3rem",
          borderRadius: 3,
          whiteSpace: "nowrap",
          color: presentation.color,
          background: presentation.background,
          border: `1px solid ${presentation.border}`,
        }}
      >
        {presentation.label}
      </span>
      {isNext ? (
        <span
          role="note"
          title="This is the next rung to attack"
          aria-label="Next rung to attack"
          style={{
            fontSize: "0.62rem",
            fontWeight: 700,
            lineHeight: 1.2,
            padding: "0.05rem 0.3rem",
            borderRadius: 3,
            whiteSpace: "nowrap",
            color: "var(--accent, #e8a33d)",
            background: "rgba(232,163,61,0.12)",
            border: "1px solid var(--accent, #e8a33d)",
          }}
        >
          ▶ next
        </span>
      ) : null}
    </span>
  );
}
