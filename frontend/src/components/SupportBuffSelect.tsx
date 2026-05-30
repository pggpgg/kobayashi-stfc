import { type CSSProperties, memo, useCallback, useMemo } from "react";
import {
  normalizeSupportBuffSelection,
  SUPPORT_BUFF_OPTIONS,
  type SupportBuffId,
  type SupportBuffOption,
  type SupportBuffSide,
  supportBuffOptionsForSide,
} from "../lib/supportBuffs";

const summaryStyle: CSSProperties = {
  padding: "0.4rem 0.6rem",
  background: "var(--bg)",
  border: "1px solid var(--border)",
  borderRadius: 6,
  color: "var(--text)",
  cursor: "pointer",
  fontSize: "0.9rem",
  userSelect: "none",
};

const panelStyle: CSSProperties = {
  position: "absolute",
  top: "100%",
  left: 0,
  marginTop: 4,
  width: 440,
  maxWidth: "min(440px, calc(100vw - 2rem))",
  maxHeight: 380,
  overflowY: "auto",
  background: "var(--surface)",
  border: "1px solid var(--border)",
  borderRadius: 6,
  padding: "0.65rem",
  zIndex: 50,
  boxShadow: "0 4px 12px rgba(0,0,0,0.25)",
};

const rowStyle: CSSProperties = {
  fontSize: "0.85rem",
  display: "flex",
  alignItems: "flex-start",
  gap: "0.6rem",
  cursor: "pointer",
  padding: "0.55rem",
  borderRadius: 6,
  border: "1px solid var(--border)",
  background: "rgba(255,255,255,0.02)",
};

interface SupportBuffSelectProps {
  selected: readonly string[];
  onChange: (ids: SupportBuffId[]) => void;
  /** When set, only show catalog entries routed to this PvP side. */
  side?: SupportBuffSide;
  summaryLabel?: string;
  panelTitle?: string;
  helpText?: string;
}

interface SupportBuffGroup {
  key: string;
  label: string;
  sourceTag: string;
  options: SupportBuffOption[];
  note?: string;
}

const statLabels: Record<string, string> = {
  crit_damage: "Crit damage",
  weapon_damage: "Weapon damage",
};

function sourceGroupFor(
  option: SupportBuffOption,
): Omit<SupportBuffGroup, "options"> {
  if (option.source.startsWith("Titan-A Fortify")) {
    return {
      key: "titan-a-fortify",
      label: "Titan-A Fortify",
      sourceTag: "Alliance support",
      note: "Only one Fortify level can apply.",
    };
  }
  if (option.source.startsWith("Cerritos")) {
    return {
      key: "cerritos",
      label: "Cerritos",
      sourceTag: "Alliance support",
    };
  }
  if (option.source.startsWith("Defiant")) {
    return {
      key: "defiant",
      label: "Defiant",
      sourceTag: "Reinforce",
    };
  }
  if (option.source.startsWith("Mantis")) {
    return {
      key: "mantis",
      label: "Mantis",
      sourceTag: "Syndicate (TBD)",
    };
  }
  return {
    key: option.source,
    label: option.source,
    sourceTag: "Support",
  };
}

function groupSupportBuffOptions(
  options: readonly SupportBuffOption[],
): SupportBuffGroup[] {
  const groups = new Map<string, SupportBuffGroup>();

  for (const option of options) {
    const group = sourceGroupFor(option);
    const existing = groups.get(group.key);
    if (existing) {
      existing.options.push(option);
    } else {
      groups.set(group.key, { ...group, options: [option] });
    }
  }

  return Array.from(groups.values());
}

function statCategoryLabels(option: SupportBuffOption): string[] {
  const statCategories = option.statTargets.map(
    (target) => statLabels[target.stat] ?? target.stat,
  );

  if (statCategories.length === 0) {
    return ["Research gate"];
  }

  if (option.description.toLowerCase().includes("research")) {
    return [...statCategories, "Research gate"];
  }

  return statCategories;
}

function formatStatTarget(option: SupportBuffOption): string | null {
  if (option.statTargets.length === 0) {
    return null;
  }

  return option.statTargets
    .map((target) => {
      const label = statLabels[target.stat] ?? target.stat;
      const pct = Math.round((target.value - 1) * 100);
      return pct > 0 ? `+${pct}% ${label.toLowerCase()}` : label.toLowerCase();
    })
    .join(", ");
}

const defaultHelpText =
  "Choose active alliance support. Fortify, Cerritos, and Defiant unlock their matching catalog research when selected.";

export default memo(function SupportBuffSelect({
  selected,
  onChange,
  side,
  summaryLabel = "Support buffs",
  panelTitle = "Support Buffs",
  helpText = defaultHelpText,
}: SupportBuffSelectProps) {
  const options = side ? supportBuffOptionsForSide(side) : SUPPORT_BUFF_OPTIONS;
  const groups = useMemo(() => groupSupportBuffOptions(options), [options]);
  const validation = normalizeSupportBuffSelection(selected);
  const normalizedSelected = validation.ids;

  const toggle = useCallback(
    (id: SupportBuffId) => {
      if (normalizedSelected.includes(id)) {
        onChange(normalizedSelected.filter((x) => x !== id));
      } else {
        onChange(
          normalizeSupportBuffSelection([...normalizedSelected, id]).ids,
        );
      }
    },
    [normalizedSelected, onChange],
  );

  const label =
    normalizedSelected.length === 0
      ? summaryLabel
      : `${summaryLabel} (${normalizedSelected.length})`;

  return (
    <details style={{ position: "relative" }} className="support-buff-select">
      <summary style={summaryStyle}>{label}</summary>
      <fieldset style={panelStyle}>
        <legend style={{ position: "absolute", left: -10_000, top: "auto" }}>
          {panelTitle}
        </legend>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "0.75rem",
            marginBottom: "0.35rem",
          }}
        >
          <strong style={{ fontSize: "0.85rem" }}>{panelTitle}</strong>
          <span
            style={{
              border: "1px solid rgba(232,149,46,0.5)",
              borderRadius: 999,
              color: "var(--accent)",
              fontSize: "0.65rem",
              lineHeight: 1,
              padding: "0.2rem 0.45rem",
              textTransform: "uppercase",
              whiteSpace: "nowrap",
            }}
          >
            Request-scoped
          </span>
        </div>
        <div
          style={{
            fontSize: "0.74rem",
            color: "var(--text-muted)",
            margin: "0 0 0.65rem",
            lineHeight: 1.35,
          }}
        >
          {helpText}
        </div>
        {validation.issues.length > 0 ? (
          <div
            role="status"
            style={{
              border: "1px solid var(--warning)",
              borderRadius: 6,
              color: "var(--text)",
              background: "rgba(201,162,39,0.14)",
              fontSize: "0.72rem",
              lineHeight: 1.35,
              marginBottom: "0.65rem",
              padding: "0.45rem 0.55rem",
            }}
          >
            {validation.issues[0].message}
          </div>
        ) : null}
        <div style={{ display: "grid", gap: "0.75rem" }}>
          {groups.map((group, groupIndex) => (
            <section
              key={group.key}
              style={{
                borderTop:
                  groupIndex === 0 ? "none" : "1px solid var(--border)",
                paddingTop: groupIndex === 0 ? 0 : "0.75rem",
              }}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: "0.5rem",
                  marginBottom: "0.45rem",
                }}
              >
                <strong style={{ fontSize: "0.8rem" }}>{group.label}</strong>
                <span
                  style={{
                    color: "var(--text-muted)",
                    fontSize: "0.66rem",
                    border: "1px solid var(--border)",
                    borderRadius: 999,
                    padding: "0.15rem 0.4rem",
                    whiteSpace: "nowrap",
                  }}
                >
                  {group.sourceTag}
                </span>
              </div>
              <div style={{ display: "grid", gap: "0.4rem" }}>
                {group.options.map((opt) => {
                  const selectedOption = normalizedSelected.includes(opt.id);
                  const statTargetSummary = formatStatTarget(opt);
                  return (
                    <label
                      key={opt.id}
                      style={{
                        ...rowStyle,
                        background: selectedOption
                          ? "rgba(232,149,46,0.08)"
                          : rowStyle.background,
                      }}
                      title={opt.description}
                    >
                      <input
                        type="checkbox"
                        checked={selectedOption}
                        onChange={() => toggle(opt.id)}
                      />
                      <span
                        style={{ display: "grid", gap: "0.35rem", flex: 1 }}
                      >
                        <span
                          style={{
                            display: "flex",
                            alignItems: "center",
                            justifyContent: "space-between",
                            gap: "0.5rem",
                            flexWrap: "wrap",
                          }}
                        >
                          <strong style={{ fontWeight: 600 }}>
                            {opt.label}
                          </strong>
                          <span
                            style={{
                              display: "flex",
                              gap: "0.25rem",
                              flexWrap: "wrap",
                              justifyContent: "flex-end",
                            }}
                          >
                            {statCategoryLabels(opt).map((category) => (
                              <span
                                key={category}
                                style={{
                                  border: "1px solid rgba(232,149,46,0.35)",
                                  borderRadius: 999,
                                  color: "var(--accent)",
                                  fontSize: "0.62rem",
                                  lineHeight: 1,
                                  padding: "0.18rem 0.35rem",
                                  textTransform: "uppercase",
                                  whiteSpace: "nowrap",
                                }}
                              >
                                {category}
                              </span>
                            ))}
                          </span>
                        </span>
                        <span
                          style={{
                            color: "var(--text-muted)",
                            fontSize: "0.72rem",
                            lineHeight: 1.35,
                          }}
                        >
                          {statTargetSummary
                            ? `${statTargetSummary}; ${opt.description}`
                            : opt.description}
                        </span>
                      </span>
                    </label>
                  );
                })}
              </div>
              {group.note ? (
                <div
                  style={{
                    color: "var(--text-muted)",
                    fontSize: "0.7rem",
                    marginTop: "0.35rem",
                  }}
                >
                  {group.note}
                </div>
              ) : null}
            </section>
          ))}
        </div>
      </fieldset>
    </details>
  );
});
