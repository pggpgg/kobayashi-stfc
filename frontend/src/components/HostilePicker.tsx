import { useState, useEffect, useRef, useId, type CSSProperties } from 'react';
import { type HostileListItem, hostileSortLabel } from '../lib/api';

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
  const [query, setQuery] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  const selected = hostiles.find((h) => h.id === value);
  const selectedLabel = selected ? hostileRowLabel(selected) : (value ? value : '');
  const displayValue = open ? query : selectedLabel;

  const q = query.trim().toLowerCase();
  const filtered = q
    ? hostiles.filter((h) => {
        const row = hostileRowLabel(h).toLowerCase();
        return row.includes(q) || h.id.toLowerCase().includes(q);
      })
    : hostiles;
  const limited = filtered.slice(0, LIST_LIMIT);

  useEffect(() => {
    if (!open) setQuery('');
  }, [open]);

  useEffect(() => {
    if (open && inputRef.current) inputRef.current.focus();
  }, [open]);

  const handleBlur = () => {
    setTimeout(() => setOpen(false), 150);
  };

  const handleSelect = (id: string) => {
    onChange(id);
    setOpen(false);
    setQuery('');
  };

  const loading = !disabled && hostiles.length === 0;

  return (
    <div
      style={{
        position: 'relative',
        minWidth: 200,
        maxWidth: 280,
        ...style,
      }}
    >
      <input
        ref={inputRef}
        type="text"
        value={loading ? '' : displayValue}
        readOnly={loading || disabled}
        onChange={(e) => {
          setQuery(e.target.value);
          setOpen(true);
        }}
        onFocus={() => {
          if (!loading && !disabled) setOpen(true);
        }}
        onBlur={handleBlur}
        placeholder={loading ? 'Loading…' : 'Search scenario…'}
        aria-label="Scenario"
        aria-autocomplete="list"
        aria-expanded={open}
        aria-controls={listId}
        disabled={disabled}
        style={{
          width: '100%',
          boxSizing: 'border-box',
          padding: '0.4rem 0.6rem',
          background: 'var(--bg)',
          border: '1px solid var(--border)',
          borderRadius: 6,
          color: 'var(--text)',
          fontSize: '0.9rem',
          outline: 'none',
        }}
      />
      {open && !loading && !disabled && (
        <div
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
          {limited.length === 0 && (
            <div style={{ padding: '0.4rem 0.6rem', fontSize: '0.85rem', color: 'var(--text-muted)' }}>
              No match
            </div>
          )}
          {limited.map((h) => (
            <button
              key={h.id}
              type="button"
              role="option"
              aria-selected={h.id === value}
              style={{
                display: 'block',
                width: '100%',
                padding: '0.4rem 0.6rem',
                textAlign: 'left',
                background: h.id === value ? 'var(--border)' : 'transparent',
                border: 'none',
                color: 'var(--text)',
                fontSize: '0.85rem',
              }}
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
