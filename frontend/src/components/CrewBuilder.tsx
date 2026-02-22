import { useState, useEffect } from 'react';
import type { OfficerListItem } from '../lib/api';
import type { CrewState, PinsState } from '../lib/types';
import { belowDeckSlotCount } from '../lib/types';
import { fetchOfficers } from '../lib/api';

const SLOT_LABELS = ['Captain', 'Bridge 1', 'Bridge 2'] as const;

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
  const [pickerSlot, setPickerSlot] = useState<
    { kind: 'captain' | 'bridge'; index: number } | { kind: 'belowDeck'; index: number } | null
  >(null);

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

  const setCaptain = (id: string | null) => {
    onCrewChange({ ...crew, captain: id });
    setPickerSlot(null);
  };
  const setBridge = (index: number, id: string | null) => {
    const next = [...crew.bridge] as [string | null, string | null];
    next[index] = id;
    onCrewChange({ ...crew, bridge: next });
    setPickerSlot(null);
  };
  const setBelowDeck = (index: number, id: string | null) => {
    const next = [...crew.belowDeck];
    next[index] = id;
    onCrewChange({ ...crew, belowDeck: next });
    setPickerSlot(null);
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

  const getOfficerName = (id: string | null) => {
    if (!id) return null;
    return officers.find((o) => o.id === id)?.name ?? id;
  };

  const openPicker = (slot: typeof pickerSlot) => setPickerSlot(slot);
  const closePicker = () => setPickerSlot(null);

  const selectedIds = new Set([
    crew.captain,
    ...crew.bridge,
    ...crew.belowDeck,
  ].filter(Boolean) as string[]);

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
      <h2 style={{ margin: '0 0 0.75rem', fontSize: '1rem' }}>CrewBuilder</h2>

      <div style={{ marginBottom: '0.5rem', display: 'flex', alignItems: 'center', gap: 8 }}>
        <label style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: '0.85rem' }}>
          <input
            type="checkbox"
            checked={ownedOnly}
            onChange={(e) => setOwnedOnly(e.target.checked)}
          />
          Owned only
        </label>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem' }}>
        {/* Captain */}
        <SlotRow
          label="Captain"
          value={getOfficerName(crew.captain)}
          pinned={pins.captain}
          onPin={() => togglePin('captain')}
          onSelect={() => openPicker({ kind: 'captain', index: 0 })}
        />
        {/* Bridge */}
        {([0, 1] as const).map((i) => (
          <SlotRow
            key={i}
            label={SLOT_LABELS[i + 1]}
            value={getOfficerName(crew.bridge[i])}
            pinned={pins.bridge[i]}
            onPin={() => togglePin('bridge', i)}
            onSelect={() => openPicker({ kind: 'bridge', index: i })}
          />
        ))}
        {/* Below Deck */}
        {crew.belowDeck.slice(0, belowN).map((id, i) => (
          <SlotRow
            key={i}
            label={`Below Deck ${i + 1}`}
            value={getOfficerName(id)}
            pinned={pins.belowDeck[i]}
            onPin={() => togglePin('belowDeck', i)}
            onSelect={() => openPicker({ kind: 'belowDeck', index: i })}
          />
        ))}
      </div>

      <p style={{ margin: '0.75rem 0 0', fontSize: '0.8rem', color: 'var(--text-muted)' }}>
        Synergy: — (hint strip when data available)
      </p>

      {pickerSlot && (
        <OfficerPicker
          officers={officers}
          selectedIds={selectedIds}
          pickerSlot={pickerSlot}
          onSelect={(id) => {
            if (pickerSlot.kind === 'captain') setCaptain(id);
            else if (pickerSlot.kind === 'bridge') setBridge(pickerSlot.index, id);
            else setBelowDeck(pickerSlot.index, id);
          }}
          onClose={closePicker}
        />
      )}
    </section>
  );
}

function SlotRow({
  label,
  value,
  pinned,
  onPin,
  onSelect,
}: {
  label: string;
  value: string | null;
  pinned: boolean;
  onPin: () => void;
  onSelect: () => void;
}) {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        padding: '0.35rem 0',
        borderBottom: '1px solid var(--border)',
      }}
    >
      <span style={{ width: 100, fontSize: '0.85rem' }}>{label}</span>
      <button
        type="button"
        onClick={onSelect}
        style={{
          flex: 1,
          textAlign: 'left',
          padding: '0.4rem 0.6rem',
          background: 'var(--bg)',
          border: '1px solid var(--border)',
          borderRadius: 4,
          color: value ? 'var(--text)' : 'var(--text-muted)',
        }}
      >
        {value ?? 'Select…'}
      </button>
      <button
        type="button"
        onClick={onPin}
        title={pinned ? 'Unpin' : 'Pin'}
        aria-label={pinned ? 'Unpin' : 'Pin'}
        style={{
          padding: '0.4rem',
          background: pinned ? 'var(--accent-dim)' : 'transparent',
          border: '1px solid var(--border)',
          borderRadius: 4,
          color: 'var(--text)',
          fontSize: '0.75rem',
        }}
      >
        {pinned ? 'Pinned' : 'Pin'}
      </button>
    </div>
  );
}

function OfficerPicker({
  officers,
  selectedIds,
  pickerSlot,
  onSelect,
  onClose,
}: {
  officers: OfficerListItem[];
  selectedIds: Set<string>;
  pickerSlot: { kind: string; index: number };
  onSelect: (id: string | null) => void;
  onClose: () => void;
}) {
  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(0,0,0,0.6)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 1000,
      }}
      onClick={onClose}
    >
      <div
        style={{
          background: 'var(--surface)',
          border: '1px solid var(--border)',
          borderRadius: 8,
          padding: '1rem',
          maxWidth: 400,
          maxHeight: '70vh',
          overflow: 'auto',
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
          <strong>Choose officer</strong>
          <button type="button" onClick={onClose} style={{ padding: 4 }}>✕</button>
        </div>
        <button
          type="button"
          style={{ display: 'block', width: '100%', padding: '0.4rem', marginBottom: 8, textAlign: 'left' }}
          onClick={() => onSelect(null)}
        >
          — Clear —
        </button>
        <ul style={{ listStyle: 'none', margin: 0, padding: 0 }}>
          {officers.map((o) => (
            <li key={o.id}>
              <button
                type="button"
                style={{
                  display: 'block',
                  width: '100%',
                  padding: '0.4rem',
                  textAlign: 'left',
                  background: selectedIds.has(o.id) ? 'var(--border)' : 'transparent',
                  border: 'none',
                  color: 'var(--text)',
                }}
                onClick={() => onSelect(o.id)}
              >
                {o.name}
                {selectedIds.has(o.id) ? ' ✓' : ''}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
