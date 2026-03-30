import type { CSSProperties } from "react";
import {
  SUPPORT_BUFF_OPTIONS,
  type SupportBuffId,
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
  minWidth: 240,
  maxHeight: 200,
  overflowY: "auto",
  background: "var(--surface)",
  border: "1px solid var(--border)",
  borderRadius: 6,
  padding: "0.4rem",
  zIndex: 50,
  boxShadow: "0 4px 12px rgba(0,0,0,0.25)",
};

const rowStyle: CSSProperties = {
  fontSize: "0.85rem",
  display: "flex",
  alignItems: "center",
  gap: "0.5rem",
  cursor: "pointer",
  padding: "0.25rem 0.2rem",
  borderRadius: 4,
};

interface SupportBuffSelectProps {
  selected: SupportBuffId[];
  onChange: (ids: SupportBuffId[]) => void;
}

export default function SupportBuffSelect({
  selected,
  onChange,
}: SupportBuffSelectProps) {
  function toggle(id: SupportBuffId) {
    if (selected.includes(id)) {
      onChange(selected.filter((x) => x !== id));
    } else {
      onChange([...selected, id]);
    }
  }

  const label =
    selected.length === 0
      ? "Support buffs"
      : `Support buffs (${selected.length})`;

  return (
    <details
      style={{ position: "relative" }}
      className="support-buff-select"
    >
      <summary style={summaryStyle}>{label}</summary>
      <div style={panelStyle} role="group" aria-label="Support buffs">
        {SUPPORT_BUFF_OPTIONS.map((opt) => (
          <label
            key={opt.id}
            style={rowStyle}
            title={"description" in opt ? opt.description : opt.label}
          >
            <input
              type="checkbox"
              checked={selected.includes(opt.id)}
              onChange={() => toggle(opt.id)}
            />
            <span>{opt.label}</span>
          </label>
        ))}
      </div>
    </details>
  );
}
