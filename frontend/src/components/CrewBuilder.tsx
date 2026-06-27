import {
  memo,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import type {
  EligibilityVerdict,
  OfficerEligibilityResponse,
  OfficerListItem,
} from "../lib/api";
import type { CrewState, PinsState } from "../lib/types";
import { EligibilityBadge } from "./EligibilityBadge";

interface CrewBuilderProps {
  /** Guided mode hides optimizer-oriented pinning and progressively reveals optional below decks. */
  guided?: boolean;
  /** Resolved below-decks officer slot count for the current ship level + unlock schedule. */
  belowDecksSlots: number;
  crew: CrewState;
  pins: PinsState;
  onCrewChange: (crew: CrewState) => void;
  onPinsChange: (pins: PinsState) => void;
  officerOptions?: OfficerListItem[];
  /** Selected combat scenario (""=Auto → defaults badges to generic hostiles). */
  enemyType?: string;
  /** Officer eligibility matrix for per-slot badges; null until loaded. */
  eligibility?: OfficerEligibilityResponse | null;
}

const slotStyleBase = {
  flex: 1,
  minWidth: 100,
  display: "flex",
  flexDirection: "column" as const,
  alignItems: "center",
  gap: 4,
};

const boxStyleBase = {
  width: "100%",
  padding: "0.5rem",
  background: "var(--bg)",
  borderRadius: 8,
};

export default memo(function CrewBuilder({
  guided = false,
  belowDecksSlots,
  crew,
  pins,
  onCrewChange,
  onPinsChange,
  officerOptions = [],
  enemyType = "",
  eligibility = null,
}: CrewBuilderProps) {
  const belowN = belowDecksSlots;
  const [showGuidedBelowDecks, setShowGuidedBelowDecks] = useState(false);

  // Live eligibility badges: best verdict across a seat's ability ids for the current scenario.
  // "" (Auto) falls back to generic hostiles so badges still render before a scenario is chosen.
  const scenarioKey = enemyType || "red_moving_space";
  const seatVerdict = useCallback(
    (
      officerId: string | null | undefined,
      seat: "captain" | "officer" | "below_decks",
    ): { verdict: EligibilityVerdict; reason?: string } | null => {
      if (!eligibility || !officerId) return null;
      const seats = eligibility.officer_abilities[officerId];
      if (!seats) return null;
      const ids =
        seat === "officer"
          ? (seats.officer ?? [])
          : seats[seat]
            ? [seats[seat] as string]
            : [];
      const rank = { works: 2, conditional: 1, does_not_work: 0 } as const;
      let best: { verdict: EligibilityVerdict; reason?: string } | null = null;
      for (const id of ids) {
        const sv = eligibility.matrix[id]?.scenarios?.[scenarioKey];
        if (!sv) continue;
        if (!best || rank[sv.verdict] > rank[best.verdict]) best = sv;
      }
      return best;
    },
    [eligibility, scenarioKey],
  );
  const renderBadge = useCallback(
    (
      officerId: string | null | undefined,
      seat: "captain" | "officer" | "below_decks",
    ) => {
      const v = seatVerdict(officerId, seat);
      return (
        <EligibilityBadge verdict={v?.verdict ?? null} reason={v?.reason} />
      );
    },
    [seatVerdict],
  );

  const selectedIds = useMemo(
    () =>
      new Set(
        [crew.captain, ...crew.bridge, ...crew.belowDeck].filter(
          Boolean,
        ) as string[],
      ),
    [crew.captain, crew.bridge, crew.belowDeck],
  );

  const clearIdFromOtherSlots = useCallback(
    (id: string | null): Partial<CrewState> => {
      if (!id) return {};
      return {
        captain: crew.captain === id ? null : crew.captain,
        bridge: [
          crew.bridge[0] === id ? null : crew.bridge[0],
          crew.bridge[1] === id ? null : crew.bridge[1],
        ] as [string | null, string | null],
        belowDeck: crew.belowDeck.map((o) => (o === id ? null : o)),
      };
    },
    [crew.captain, crew.bridge, crew.belowDeck],
  );

  const setCaptain = useCallback(
    (id: string | null) => {
      const cleared = clearIdFromOtherSlots(id);
      onCrewChange({ ...crew, ...cleared, captain: id });
    },
    [crew, onCrewChange, clearIdFromOtherSlots],
  );

  const setBridge = useCallback(
    (index: number, id: string | null) => {
      const cleared = clearIdFromOtherSlots(id);
      const bridge = [...(cleared.bridge ?? crew.bridge)] as [
        string | null,
        string | null,
      ];
      bridge[index] = id;
      onCrewChange({ ...crew, ...cleared, bridge });
    },
    [crew, onCrewChange, clearIdFromOtherSlots],
  );

  const setBelowDeck = useCallback(
    (index: number, id: string | null) => {
      const cleared = clearIdFromOtherSlots(id);
      const belowDeck = [...(cleared.belowDeck ?? crew.belowDeck)];
      belowDeck[index] = id;
      onCrewChange({ ...crew, ...cleared, belowDeck });
    },
    [crew, onCrewChange, clearIdFromOtherSlots],
  );

  const togglePin = useCallback(
    (kind: "captain" | "bridge" | "belowDeck", index?: number) => {
      if (kind === "captain") {
        onPinsChange({ ...pins, captain: !pins.captain });
      } else if (kind === "bridge" && index !== undefined) {
        const next = [...pins.bridge] as [boolean, boolean];
        next[index] = !next[index];
        onPinsChange({ ...pins, bridge: next });
      } else if (kind === "belowDeck" && index !== undefined) {
        const next = [...pins.belowDeck];
        next[index] = !next[index];
        onPinsChange({ ...pins, belowDeck: next });
      }
    },
    [pins, onPinsChange],
  );

  return (
    <section
      id="guided-crew"
      style={{
        padding: "1rem",
        background: "var(--surface)",
        border: "1px solid var(--border)",
        borderRadius: 8,
        marginBottom: "1rem",
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: "0.75rem",
        }}
      >
        <h2 style={{ margin: 0, fontSize: "1rem" }}>
          {guided ? "Choose your bridge crew" : "BRIDGE"}
        </h2>
      </div>

      {/* Top row: Bridge 1 | Captain (center) | Bridge 2 */}
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          justifyContent: "center",
          alignItems: "flex-start",
          gap: 8,
          marginBottom: "1rem",
        }}
      >
        <div style={{ ...slotStyleBase, maxWidth: 140 }}>
          <span style={{ fontSize: "0.75rem", color: "var(--text-muted)" }}>
            Bridge 1
          </span>
          <div
            style={{
              ...boxStyleBase,
              border: "1px solid var(--border)",
            }}
          >
            <TypeAheadSlot
              officers={officerOptions}
              value={crew.bridge[0]}
              selectedIds={selectedIds}
              onChange={(id) => setBridge(0, id)}
              placeholder="Select…"
            />
          </div>
          {renderBadge(crew.bridge[0], "officer")}
          {!guided && (
            <button
              type="button"
              onClick={() => togglePin("bridge", 0)}
              style={{
                fontSize: "0.7rem",
                padding: "2px 6px",
                opacity: pins.bridge[0] ? 1 : 0.6,
              }}
            >
              {pins.bridge[0] ? "Pinned" : "Pin"}
            </button>
          )}
        </div>

        <div style={{ ...slotStyleBase, maxWidth: 160 }}>
          <span style={{ fontSize: "0.75rem", color: "var(--accent)" }}>
            Captain
          </span>
          <div
            style={{
              ...boxStyleBase,
              border: "1px solid var(--accent)",
              boxShadow: "0 0 0 1px var(--accent)",
            }}
          >
            <TypeAheadSlot
              officers={officerOptions}
              value={crew.captain}
              selectedIds={selectedIds}
              onChange={setCaptain}
              placeholder="Select…"
            />
          </div>
          {renderBadge(crew.captain, "captain")}
          {!guided && (
            <button
              type="button"
              onClick={() => togglePin("captain")}
              style={{
                fontSize: "0.7rem",
                padding: "2px 6px",
                opacity: pins.captain ? 1 : 0.6,
              }}
            >
              {pins.captain ? "Pinned" : "Pin"}
            </button>
          )}
        </div>

        <div style={{ ...slotStyleBase, maxWidth: 140 }}>
          <span style={{ fontSize: "0.75rem", color: "var(--text-muted)" }}>
            Bridge 2
          </span>
          <div
            style={{
              ...boxStyleBase,
              border: "1px solid var(--border)",
            }}
          >
            <TypeAheadSlot
              officers={officerOptions}
              value={crew.bridge[1]}
              selectedIds={selectedIds}
              onChange={(id) => setBridge(1, id)}
              placeholder="Select…"
            />
          </div>
          {renderBadge(crew.bridge[1], "officer")}
          {!guided && (
            <button
              type="button"
              onClick={() => togglePin("bridge", 1)}
              style={{
                fontSize: "0.7rem",
                padding: "2px 6px",
                opacity: pins.bridge[1] ? 1 : 0.6,
              }}
            >
              {pins.bridge[1] ? "Pinned" : "Pin"}
            </button>
          )}
        </div>
      </div>

      {guided && belowN > 0 && (
        <button
          type="button"
          aria-expanded={showGuidedBelowDecks}
          onClick={() => setShowGuidedBelowDecks((visible) => !visible)}
          style={{ marginBottom: "0.65rem" }}
        >
          {showGuidedBelowDecks
            ? "Hide optional below decks"
            : `Add optional below-deck officers (${belowN} slots)`}
        </button>
      )}
      <div hidden={guided && !showGuidedBelowDecks}>
        {/* Bottom row: Below Deck slots */}
        <div
          style={{
            marginBottom: "0.5rem",
            fontSize: "0.75rem",
            color: "var(--text-muted)",
          }}
        >
          Below deck
        </div>
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: 8,
          }}
        >
          {crew.belowDeck.slice(0, belowN).map((id, i) => (
            <div
              key={i}
              style={{ ...slotStyleBase, minWidth: 120, maxWidth: 140 }}
            >
              <span style={{ fontSize: "0.75rem", color: "var(--text-muted)" }}>
                Below {i + 1}
              </span>
              <div
                style={{
                  ...boxStyleBase,
                  border: "1px solid var(--border)",
                }}
              >
                <TypeAheadSlot
                  officers={officerOptions}
                  value={id}
                  selectedIds={selectedIds}
                  onChange={(oId) => setBelowDeck(i, oId)}
                  placeholder="Select…"
                />
              </div>
              {renderBadge(id, "below_decks")}
              {!guided && (
                <button
                  type="button"
                  onClick={() => togglePin("belowDeck", i)}
                  style={{
                    fontSize: "0.7rem",
                    padding: "2px 6px",
                    opacity: pins.belowDeck[i] ? 1 : 0.6,
                  }}
                >
                  {pins.belowDeck[i] ? "Pinned" : "Pin"}
                </button>
              )}
            </div>
          ))}
        </div>
      </div>

      <p
        style={{
          margin: "0.75rem 0 0",
          fontSize: "0.8rem",
          color: "var(--text-muted)",
        }}
      >
        Synergy: — (hint strip when data available)
      </p>
    </section>
  );
});

function TypeAheadSlot({
  officers,
  value,
  selectedIds,
  onChange,
  placeholder,
}: {
  officers: OfficerListItem[];
  value: string | null;
  selectedIds: Set<string>;
  onChange: (id: string | null) => void;
  placeholder: string;
}) {
  const listId = useId();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const selectedName = useMemo(
    () =>
      value ? (officers.find((o) => o.id === value)?.name ?? value) : null,
    [officers, value],
  );
  const displayValue = open ? query : (selectedName ?? "");

  const filtered = useMemo(
    () =>
      query.trim()
        ? officers.filter((o) =>
            o.name.toLowerCase().includes(query.toLowerCase()),
          )
        : officers,
    [officers, query],
  );
  const limited = useMemo(() => filtered.slice(0, 200), [filtered]);

  useEffect(() => {
    if (!open) setQuery("");
  }, [open]);

  useEffect(() => {
    if (open && inputRef.current) inputRef.current.focus();
  }, [open]);

  const handleBlur = useCallback(() => {
    setTimeout(() => setOpen(false), 150);
  }, []);

  const handleSelect = useCallback(
    (id: string | null) => {
      onChange(id);
      setOpen(false);
      setQuery("");
    },
    [onChange],
  );

  return (
    <div style={{ position: "relative", width: "100%" }}>
      <input
        ref={inputRef}
        type="text"
        role="combobox"
        value={displayValue}
        onChange={(e) => {
          setQuery(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onBlur={handleBlur}
        placeholder={placeholder}
        aria-autocomplete="list"
        aria-expanded={open}
        aria-controls={listId}
        style={{
          width: "100%",
          padding: "0.35rem 0.5rem",
          background: "transparent",
          border: "none",
          color: "var(--text)",
          fontSize: "0.9rem",
        }}
      />
      {open && (
        <div
          ref={listRef}
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
          <button
            type="button"
            role="option"
            style={{
              display: "block",
              width: "100%",
              padding: "0.4rem 0.6rem",
              textAlign: "left",
              background: "transparent",
              border: "none",
              color: "var(--text-muted)",
              fontSize: "0.85rem",
            }}
            onMouseDown={(e) => {
              e.preventDefault();
              handleSelect(null);
            }}
          >
            — Clear —
          </button>
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
          {limited.map((o) => (
            <button
              key={o.id}
              type="button"
              role="option"
              style={{
                display: "block",
                width: "100%",
                padding: "0.4rem 0.6rem",
                textAlign: "left",
                background: selectedIds.has(o.id)
                  ? "var(--border)"
                  : "transparent",
                border: "none",
                color: "var(--text)",
                fontSize: "0.85rem",
              }}
              onMouseDown={(e) => {
                e.preventDefault();
                handleSelect(o.id);
              }}
            >
              {o.name}
              {selectedIds.has(o.id) ? " ✓" : ""}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
