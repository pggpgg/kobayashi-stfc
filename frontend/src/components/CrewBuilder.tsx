import { useState, useEffect, useRef, useId } from 'react';
import type { OfficerListItem } from '../lib/api';
import type { CrewState, PinsState } from '../lib/types';
import { belowDeckSlotCount } from '../lib/types';
import { fetchOfficers } from '../lib/api';

interface CrewBuilderProps {
  shipLevel: number;
  crew: CrewState;
  pins: PinsState;
  onCrewChange: (crew: CrewState) => void;
  onPinsChange: (pins: PinsState) => void;
}

export default function CrewBuilder({
  shipLevel,
  crew,
  pins,
  onCrewChange,
  onPinsChange,
}: CrewBuilderProps) {
  const [officers, setOfficers] = useState<OfficerListItem[]>([]);
  const [ownedOnly, setOwnedOnly] = useState(false);

  const belowN = belowDeckSlotCount(shipLevel);

  useEffect(() => {
    let cancelled = false;
    fetchOfficers(ownedOnly).then((list) => {
      if (!cancelled) setOfficers(list);
    });
    return () => {
      cancelled = true;
    };
  }, [ownedOnly]);

  /** When placing an officer in one slot, clear them from all other slots so they only appear once. */
  const clearIdFromOtherSlots = (id: string | null): Partial<CrewState> => {
    if (!id) return {};
    return {
      captain: crew.captain === id ? null : crew.captain,
      bridge: [
        crew.bridge[0] === id ? null : crew.bridge[0],
        crew.bridge[1] === id ? null : crew.bridge[1],
      ] as [string | null, string | null],
      belowDeck: crew.belowDeck.map((o) => (o === id ? null : o)),
    };
  };

  const setCaptain = (id: string | null) => {
    const cleared = clearIdFromOtherSlots(id);
    onCrewChange({ ...crew, ...cleared, captain: id });
  };
  const setBridge = (index: number, id: string | null) => {
    const cleared = clearIdFromOtherSlots(id);
    const bridge = [...(cleared.bridge ?? crew.bridge)] as [string | null, string | null];
    bridge[index] = id;
    onCrewChange({ ...crew, ...cleared, bridge });
  };
  const setBelowDeck = (index: number, id: string | null) => {
    const cleared = clearIdFromOtherSlots(id);
    const belowDeck = [...(cleared.belowDeck ?? crew.belowDeck)];
    belowDeck[index] = id;
    onCrewChange({ ...crew, ...cleared, belowDeck });
  };

  const togglePin = (
    kind: 'captain' | 'bridge' | 'belowDeck',
    index?: number
  ) => {
    if (kind === 'captain') {
      onPinsChange({ ...pins, captain: !pins.captain });
    } else if (kind === 'bridge' && index !== undefined) {
      const next = [...pins.bridge] as [boolean, boolean];
      next[index] = !next[index];
      onPinsChange({ ...pins, bridge: next });
    } else if (kind === 'belowDeck' && index !== undefined) {
      const next = [...pins.belowDeck];
      next[index] = !next[index];
      onPinsChange({ ...pins, belowDeck: next });
    }
  };

  const selectedIds = new Set([
    crew.captain,
    ...crew.bridge,
    ...crew.belowDeck,
  ].filter(Boolean) as string[]);

  const slotStyle = (isCaptain?: boolean) => ({
    flex: 1,
    minWidth: 100,
    maxWidth: isCaptain ? 160 : 140,
    display: 'flex',
    flexDirection: 'column' as const,
    alignItems: 'center',
    gap: 4,
  });

  const boxStyle = (isCaptain?: boolean) => ({
    width: '100%',
    padding: '0.5rem',
    background: 'var(--bg)',
    border: `1px solid ${isCaptain ? 'var(--accent)' : 'var(--border)'}`,
    borderRadius: 8,
    boxShadow: isCaptain ? '0 0 0 1px var(--accent)' : undefined,
  });

  return (
    <section
      style={{
        padding: '1rem',
        background: 'var(--surface)',
        border: '1px solid var(--border)',
        borderRadius: 8,
        marginBottom: '1rem',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '0.75rem' }}>
        <h2 style={{ margin: 0, fontSize: '1rem' }}>BRIDGE</h2>
        <label style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: '0.85rem' }}>
          <input
            type="checkbox"
            checked={ownedOnly}
            onChange={(e) => setOwnedOnly(e.target.checked)}
          />
          Owned only
        </label>
      </div>

      {/* Top row: Bridge 1 | Captain (center) | Bridge 2 */}
      <div
        style={{
          display: 'flex',
          justifyContent: 'center',
          alignItems: 'flex-start',
          gap: 8,
          marginBottom: '1rem',
        }}
      >
        <div style={slotStyle(false)}>
          <span style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}>Bridge 1</span>
          <div style={boxStyle(false)}>
            <TypeAheadSlot
              officers={officers}
              value={crew.bridge[0]}
              selectedIds={selectedIds}
              onChange={(id) => setBridge(0, id)}
              placeholder="Select…"
            />
          </div>
          <button
            type="button"
            onClick={() => togglePin('bridge', 0)}
            style={{ fontSize: '0.7rem', padding: '2px 6px', opacity: pins.bridge[0] ? 1 : 0.6 }}
          >
            {pins.bridge[0] ? 'Pinned' : 'Pin'}
          </button>
        </div>

        <div style={slotStyle(true)}>
          <span style={{ fontSize: '0.75rem', color: 'var(--accent)' }}>Captain</span>
          <div style={boxStyle(true)}>
            <TypeAheadSlot
              officers={officers}
              value={crew.captain}
              selectedIds={selectedIds}
              onChange={setCaptain}
              placeholder="Select…"
            />
          </div>
          <button
            type="button"
            onClick={() => togglePin('captain')}
            style={{ fontSize: '0.7rem', padding: '2px 6px', opacity: pins.captain ? 1 : 0.6 }}
          >
            {pins.captain ? 'Pinned' : 'Pin'}
          </button>
        </div>

        <div style={slotStyle(false)}>
          <span style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}>Bridge 2</span>
          <div style={boxStyle(false)}>
            <TypeAheadSlot
              officers={officers}
              value={crew.bridge[1]}
              selectedIds={selectedIds}
              onChange={(id) => setBridge(1, id)}
              placeholder="Select…"
            />
          </div>
          <button
            type="button"
            onClick={() => togglePin('bridge', 1)}
            style={{ fontSize: '0.7rem', padding: '2px 6px', opacity: pins.bridge[1] ? 1 : 0.6 }}
          >
            {pins.bridge[1] ? 'Pinned' : 'Pin'}
          </button>
        </div>
      </div>

      {/* Bottom row: Below Deck slots */}
      <div style={{ marginBottom: '0.5rem', fontSize: '0.75rem', color: 'var(--text-muted)' }}>
        Below deck
      </div>
      <div
        style={{
          display: 'flex',
          flexWrap: 'wrap',
          gap: 8,
        }}
      >
        {crew.belowDeck.slice(0, belowN).map((id, i) => (
          <div key={i} style={{ ...slotStyle(false), minWidth: 120, maxWidth: 140 }}>
            <span style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}>Below {i + 1}</span>
            <div style={boxStyle(false)}>
              <TypeAheadSlot
                officers={officers}
                value={id}
                selectedIds={selectedIds}
                onChange={(oId) => setBelowDeck(i, oId)}
                placeholder="Select…"
              />
            </div>
            <button
              type="button"
              onClick={() => togglePin('belowDeck', i)}
              style={{ fontSize: '0.7rem', padding: '2px 6px', opacity: pins.belowDeck[i] ? 1 : 0.6 }}
            >
              {pins.belowDeck[i] ? 'Pinned' : 'Pin'}
            </button>
          </div>
        ))}
      </div>

      <p style={{ margin: '0.75rem 0 0', fontSize: '0.8rem', color: 'var(--text-muted)' }}>
        Synergy: — (hint strip when data available)
      </p>
    </section>
  );
}

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
  const [query, setQuery] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const selectedName = value ? (officers.find((o) => o.id === value)?.name ?? value) : null;
  const displayValue = open ? query : (selectedName ?? '');

  const filtered = query.trim()
    ? officers.filter((o) => o.name.toLowerCase().includes(query.toLowerCase()))
    : officers;
  const limited = filtered.slice(0, 200);

  useEffect(() => {
    if (!open) setQuery('');
  }, [open]);

  useEffect(() => {
    if (open && inputRef.current) inputRef.current.focus();
  }, [open]);

  const handleBlur = () => {
    setTimeout(() => setOpen(false), 150);
  };

  const handleSelect = (id: string | null) => {
    onChange(id);
    setOpen(false);
    setQuery('');
  };

  return (
    <div style={{ position: 'relative', width: '100%' }}>
      <input
        ref={inputRef}
        type="text"
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
          width: '100%',
          padding: '0.35rem 0.5rem',
          background: 'transparent',
          border: 'none',
          color: 'var(--text)',
          fontSize: '0.9rem',
          outline: 'none',
        }}
      />
      {open && (
        <div
          ref={listRef}
          id={listId}
          role="listbox"
          style={{
            position: 'absolute',
            left: 0,
            right: 0,
            top: '100%',
            marginTop: 2,
            maxHeight: 220,
            overflowY: 'auto',
            background: 'var(--surface)',
            border: '1px solid var(--border)',
            borderRadius: 6,
            zIndex: 100,
            boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
          }}
        >
          <button
            type="button"
            role="option"
            style={{
              display: 'block',
              width: '100%',
              padding: '0.4rem 0.6rem',
              textAlign: 'left',
              background: 'transparent',
              border: 'none',
              color: 'var(--text-muted)',
              fontSize: '0.85rem',
            }}
            onMouseDown={(e) => { e.preventDefault(); handleSelect(null); }}
          >
            — Clear —
          </button>
          {limited.length === 0 && (
            <div style={{ padding: '0.4rem 0.6rem', fontSize: '0.85rem', color: 'var(--text-muted)' }}>
              No match
            </div>
          )}
          {limited.map((o) => (
            <button
              key={o.id}
              type="button"
              role="option"
              style={{
                display: 'block',
                width: '100%',
                padding: '0.4rem 0.6rem',
                textAlign: 'left',
                background: selectedIds.has(o.id) ? 'var(--border)' : 'transparent',
                border: 'none',
                color: 'var(--text)',
                fontSize: '0.85rem',
              }}
              onMouseDown={(e) => { e.preventDefault(); handleSelect(o.id); }}
            >
              {o.name}
              {selectedIds.has(o.id) ? ' ✓' : ''}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
