import {
  type CSSProperties,
  type KeyboardEvent,
  memo,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { type HostileListItem, hostileSortLabel } from "../lib/api";

const LIST_LIMIT = 200;

function hostileNameLevelKey(h: HostileListItem): string {
  return `${hostileSortLabel(h).toLocaleLowerCase()}|${h.level}`;
}

function formatHostileDiscriminator(h: HostileListItem): string {
  const systems = h.system_names?.filter(Boolean) ?? [];
  if (systems.length > 0) {
    const shown =
      systems.length <= 2
        ? systems.join(", ")
        : `${systems.slice(0, 2).join(", ")} +${systems.length - 2}`;
    return h.warp != null && h.warp > 0 ? `${shown}, W${h.warp}` : shown;
  }
  if (h.warp != null && h.warp > 0) {
    return `W${h.warp} · #${h.id}`;
  }
  if (h.has_loot === false) {
    return `no system, no loot · #${h.id}`;
  }
  return `no system · #${h.id}`;
}

function hostileRowLabel(h: HostileListItem, disambiguate = false): string {
  const base = hostileSortLabel(h);
  if (!disambiguate) {
    return `Lvl ${h.level} · ${base}`;
  }
  return `Lvl ${h.level} · ${base} (${formatHostileDiscriminator(h)})`;
}

interface HostilePickerProps {
  hostiles: HostileListItem[];
  value: string;
  onChange: (id: string) => void;
  disabled?: boolean;
  /** Match header `<select>` styling */
  style?: CSSProperties;
}

export default memo(function HostilePicker({
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

  const duplicateNameLevels = useMemo(() => {
    const counts = new Map<string, number>();
    for (const h of hostiles) {
      const k = hostileNameLevelKey(h);
      counts.set(k, (counts.get(k) ?? 0) + 1);
    }
    const dups = new Set<string>();
    for (const [k, count] of counts) {
      if (count > 1) dups.add(k);
    }
    return dups;
  }, [hostiles]);

  const rowLabel = useCallback(
    (h: HostileListItem) =>
      hostileRowLabel(h, duplicateNameLevels.has(hostileNameLevelKey(h))),
    [duplicateNameLevels],
  );

  const selected = hostiles.find((h) => h.id === value);
  const selectedLabel = selected ? rowLabel(selected) : value ? value : "";
  const displayValue = open ? query : selectedLabel;

  const q = query.trim().toLowerCase();
  const filtered = useMemo(() => {
    if (!q) return hostiles;
    return hostiles.filter((h) => {
      const row = rowLabel(h).toLowerCase();
      if (row.includes(q) || h.id.toLowerCase().includes(q)) return true;
      if (h.system_names?.some((sys) => sys.toLowerCase().includes(q))) {
        return true;
      }
      if (h.primary_resource_name?.toLowerCase().includes(q)) {
        return true;
      }
      return false;
    });
  }, [hostiles, q, rowLabel]);
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

  const handleBlur = useCallback(() => {
    setTimeout(() => setOpen(false), 150);
  }, []);

  const handleSelect = useCallback(
    (id: string) => {
      onChange(id);
      setOpen(false);
      setQuery("");
    },
    [onChange],
  );

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
        minWidth: 240,
        maxWidth: 380,
        ...style,
      }}
    >
      <input
        ref={inputRef}
        type="text"
        role="combobox"
        value={loading ? "" : displayValue}
        title={selectedLabel || undefined}
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
          textOverflow: "ellipsis",
        }}
      />
      {open && !loading && !disabled && (
        <div
          id={listId}
          role="listbox"
          style={{
            position: "absolute",
            left: 0,
            minWidth: "100%",
            width: "max-content",
            maxWidth: "min(520px, 90vw)",
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
              {rowLabel(h)}
            </button>
          ))}
        </div>
      )}
    </div>
  );
});
