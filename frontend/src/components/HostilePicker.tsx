import {
  type CSSProperties,
  type KeyboardEvent,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { type HostileListItem, hostileSortLabel } from "../lib/api";

const LIST_LIMIT = 200;

function hostileRowLabel(h: HostileListItem): string {
  return `${hostileSortLabel(h)} (Lvl ${h.level})`;
}

interface HostilePickerProps {
  hostiles: HostileListItem[];
  value: string;
  onChange: (id: string) => void;
  disabled?: boolean;
  /** Match header `<select>` styling */
  style?: CSSProperties;
}

export default function HostilePicker({
  hostiles,
  value,
  onChange,
  disabled,
  style,
}: HostilePickerProps) {
  const listId = useId();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [highlightedIndex, setHighlightedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const selected = hostiles.find((h) => h.id === value);
  const selectedLabel = selected
    ? hostileRowLabel(selected)
    : value
      ? value
      : "";
  const displayValue = open ? query : selectedLabel;

  const q = query.trim().toLowerCase();
  const filtered = useMemo(() => {
    if (!q) return hostiles;
    return hostiles.filter((h) => {
      const row = hostileRowLabel(h).toLowerCase();
      return row.includes(q) || h.id.toLowerCase().includes(q);
    });
  }, [hostiles, q]);
  const limited = useMemo(() => filtered.slice(0, LIST_LIMIT), [filtered]);

  useEffect(() => {
    if (!open) setQuery("");
  }, [open]);

  useEffect(() => {
    if (open && inputRef.current) inputRef.current.focus();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    if (q) {
      setHighlightedIndex(0);
      return;
    }
    const idx = limited.findIndex((h) => h.id === value);
    setHighlightedIndex(idx >= 0 ? idx : limited.length > 0 ? 0 : 0);
  }, [open, q, limited, value]);

  useEffect(() => {
    if (!open || limited.length === 0) return;
    setHighlightedIndex((i) => Math.min(Math.max(0, i), limited.length - 1));
  }, [limited.length, open]);

  useEffect(() => {
    if (!open || highlightedIndex < 0 || highlightedIndex >= limited.length)
      return;
    const h = limited[highlightedIndex];
    if (!h) return;
    document
      .getElementById(`${listId}-opt-${h.id}`)
      ?.scrollIntoView({ block: "nearest" });
  }, [highlightedIndex, open, limited, listId]);

  const handleBlur = () => {
    setTimeout(() => setOpen(false), 150);
  };

  const handleSelect = (id: string) => {
    onChange(id);
    setOpen(false);
    setQuery("");
  };

  const highlightedHostile =
    highlightedIndex >= 0 && highlightedIndex < limited.length
      ? limited[highlightedIndex]
      : undefined;
  const activeOptionId =
    open && highlightedHostile
      ? `${listId}-opt-${highlightedHostile.id}`
      : undefined;

  const loading = !disabled && hostiles.length === 0;

  const onInputKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (disabled || loading) return;
    if (e.key === "Escape") {
      e.preventDefault();
      setOpen(false);
      setQuery("");
      return;
    }
    if (!open && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      e.preventDefault();
      setOpen(true);
      return;
    }
    if (!open) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (limited.length === 0) return;
      setHighlightedIndex((i) => (i + 1) % limited.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (limited.length === 0) return;
      setHighlightedIndex((i) => (i - 1 + limited.length) % limited.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const h = limited[highlightedIndex];
      if (h) handleSelect(h.id);
    }
  };

  return (
    <div
      style={{
        position: "relative",
        minWidth: 200,
        maxWidth: 280,
        ...style,
      }}
    >
      <input
        ref={inputRef}
        type="text"
        role="combobox"
        value={loading ? "" : displayValue}
        readOnly={loading || disabled}
        onChange={(e) => {
          setQuery(e.target.value);
          setOpen(true);
        }}
        onFocus={() => {
          if (!loading && !disabled) setOpen(true);
        }}
        onBlur={handleBlur}
        onKeyDown={onInputKeyDown}
        placeholder={loading ? "Loading…" : "Search scenario…"}
        aria-label="Scenario"
        aria-autocomplete="list"
        aria-expanded={open}
        aria-controls={listId}
        aria-activedescendant={activeOptionId}
        disabled={disabled}
        style={{
          width: "100%",
          boxSizing: "border-box",
          padding: "0.4rem 0.6rem",
          background: "var(--bg)",
          border: "1px solid var(--border)",
          borderRadius: 6,
          color: "var(--text)",
          fontSize: "0.9rem",
        }}
      />
      {open && !loading && !disabled && (
        <div
          id={listId}
          role="listbox"
          style={{
            position: "absolute",
            left: 0,
            right: 0,
            top: "100%",
            marginTop: 2,
            maxHeight: 220,
            overflowY: "auto",
            background: "var(--surface)",
            border: "1px solid var(--border)",
            borderRadius: 6,
            zIndex: 100,
            boxShadow: "0 4px 12px rgba(0,0,0,0.3)",
          }}
        >
          {limited.length === 0 && (
            <div
              style={{
                padding: "0.4rem 0.6rem",
                fontSize: "0.85rem",
                color: "var(--text-muted)",
              }}
            >
              No match
            </div>
          )}
          {limited.map((h, i) => (
            <button
              key={h.id}
              id={`${listId}-opt-${h.id}`}
              type="button"
              role="option"
              aria-selected={h.id === value}
              tabIndex={-1}
              style={{
                display: "block",
                width: "100%",
                padding: "0.4rem 0.6rem",
                textAlign: "left",
                background:
                  i === highlightedIndex
                    ? "var(--accent-dim)"
                    : h.id === value
                      ? "var(--border)"
                      : "transparent",
                border: "none",
                color: "var(--text)",
                fontSize: "0.85rem",
              }}
              onMouseEnter={() => setHighlightedIndex(i)}
              onMouseDown={(e) => {
                e.preventDefault();
                handleSelect(h.id);
              }}
            >
              {hostileRowLabel(h)}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
