import { memo, useMemo, type ReactNode, useId, useState } from "react";
import type { OfficerListItem } from "../lib/api";
import { joinOfficerList, splitOfficerList } from "../lib/workspaceRequests";

function canonName(raw: string, officers: OfficerListItem[]): string {
  const t = raw.trim();
  if (!t) return "";
  const hit = officers.find((o) => o.name.toLowerCase() === t.toLowerCase());
  return hit?.name ?? t;
}

interface OfficerNameMultiSelectProps {
  label: ReactNode;
  valueComma: string;
  onChangeComma: (comma: string) => void;
  officers: OfficerListItem[];
  placeholder?: string;
}

export default memo(function OfficerNameMultiSelect({
  label,
  valueComma,
  onChangeComma,
  officers,
  placeholder = "Type to search officers…",
}: OfficerNameMultiSelectProps) {
  const listId = useId();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");

  const selected = useMemo(() => splitOfficerList(valueComma), [valueComma]);
  const q = query.trim().toLowerCase();

  const suggestions = useMemo(
    () =>
      officers
        .filter((o) => {
          if (selected.some((s) => s.toLowerCase() === o.name.toLowerCase()))
            return false;
          if (!q) return true;
          return o.name.toLowerCase().includes(q);
        })
        .slice(0, 120),
    [officers, selected, q],
  );

  function addName(name: string) {
    const c = canonName(name, officers);
    if (!c) return;
    if (selected.some((s) => s.toLowerCase() === c.toLowerCase())) return;
    onChangeComma(joinOfficerList([...selected, c]));
    setQuery("");
    setOpen(false);
  }

  function removeName(name: string) {
    const next = selected.filter((s) => s.toLowerCase() !== name.toLowerCase());
    onChangeComma(joinOfficerList(next));
  }

  const displayValue = open ? query : "";

  return (
    <div style={{ fontSize: "0.8rem" }}>
      <div style={{ marginBottom: 4 }}>{label}</div>
      <div
        style={{
          border: "1px solid var(--border)",
          borderRadius: 4,
          background: "var(--bg)",
          padding: "4px 6px",
        }}
      >
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: 4,
            marginBottom: selected.length ? 6 : 0,
          }}
        >
          {selected.map((name) => (
            <span
              key={name}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 4,
                padding: "2px 6px",
                borderRadius: 4,
                background: "var(--surface)",
                border: "1px solid var(--border)",
                fontSize: "0.75rem",
              }}
            >
              {name}
              <button
                type="button"
                aria-label={`Remove ${name}`}
                onClick={() => removeName(name)}
                style={{
                  border: "none",
                  background: "transparent",
                  color: "var(--text-muted)",
                  cursor: "pointer",
                  padding: 0,
                  lineHeight: 1,
                  fontSize: "1rem",
                }}
              >
                ×
              </button>
            </span>
          ))}
        </div>
        <div style={{ position: "relative" }}>
          <input
            type="text"
            role="combobox"
            aria-expanded={open}
            aria-controls={listId}
            aria-autocomplete="list"
            placeholder={placeholder}
            value={displayValue}
            onChange={(e) => {
              setQuery(e.target.value);
              setOpen(true);
            }}
            onFocus={() => setOpen(true)}
            onBlur={() => {
              setTimeout(() => setOpen(false), 150);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setOpen(false);
                setQuery("");
              }
              if (e.key === "Enter" && suggestions.length === 1) {
                e.preventDefault();
                addName(suggestions[0].name);
              }
            }}
            style={{
              width: "100%",
              padding: "0.3rem 0.35rem",
              background: "transparent",
              border: "none",
              color: "var(--text)",
              fontSize: "0.85rem",
              outline: "none",
            }}
          />
          {open && suggestions.length > 0 && (
            <div
              id={listId}
              role="listbox"
              style={{
                position: "absolute",
                left: 0,
                right: 0,
                top: "100%",
                marginTop: 2,
                maxHeight: 200,
                overflowY: "auto",
                background: "var(--surface)",
                border: "1px solid var(--border)",
                borderRadius: 6,
                zIndex: 50,
                boxShadow: "0 4px 12px rgba(0,0,0,0.25)",
              }}
            >
              {suggestions.map((o) => (
                <button
                  key={o.id}
                  type="button"
                  role="option"
                  onMouseDown={(ev) => ev.preventDefault()}
                  onClick={() => addName(o.name)}
                  style={{
                    display: "block",
                    width: "100%",
                    textAlign: "left",
                    padding: "0.35rem 0.5rem",
                    fontSize: "0.8rem",
                    border: "none",
                    background: "transparent",
                    color: "var(--text)",
                    cursor: "pointer",
                  }}
                >
                  {o.name}
                </button>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
});
