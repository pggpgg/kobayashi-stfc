import { useRef, useId, useEffect } from 'react';
import { useModalFocusTrap } from '../lib/useModalFocusTrap';

export interface SavePresetModalProps {
  open: boolean;
  savePresetName: string;
  onSavePresetNameChange: (v: string) => void;
  savingPreset: boolean;
  onSave: () => void;
  onClose: () => void;
}

export default function SavePresetModal({
  open,
  savePresetName,
  onSavePresetNameChange,
  savingPreset,
  onSave,
  onClose,
}: SavePresetModalProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const titleId = useId();
  useModalFocusTrap(open, panelRef);

  useEffect(() => {
    if (!open) return;
    const onDocKeyDown = (e: globalThis.KeyboardEvent) => {
      if (e.key === 'Escape' && !savingPreset) {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener('keydown', onDocKeyDown);
    return () => document.removeEventListener('keydown', onDocKeyDown);
  }, [open, savingPreset, onClose]);

  if (!open) return null;

  return (
    <div
      role="presentation"
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(0,0,0,0.6)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 1000,
      }}
      onClick={() => !savingPreset && onClose()}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        style={{
          background: 'var(--surface)',
          padding: '1.5rem',
          borderRadius: 8,
          border: '1px solid var(--border)',
          maxWidth: 'min(360px, calc(100vw - 2rem))',
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <h2 id={titleId} style={{ margin: '0 0 1rem', fontSize: '1.1rem', fontWeight: 600 }}>
          Save preset
        </h2>
        <label style={{ display: 'block', marginBottom: 8 }}>
          Preset name
          <input
            type="text"
            value={savePresetName}
            onChange={(e) => onSavePresetNameChange(e.target.value)}
            placeholder="Unnamed"
            autoComplete="off"
            style={{
              display: 'block',
              marginTop: 4,
              padding: '0.5rem',
              width: '100%',
              maxWidth: 280,
              boxSizing: 'border-box',
              background: 'var(--bg)',
              border: '1px solid var(--border)',
              borderRadius: 4,
              color: 'var(--text)',
            }}
          />
        </label>
        <div style={{ display: 'flex', gap: 8, marginTop: 12, flexWrap: 'wrap' }}>
          <button
            type="button"
            onClick={onSave}
            disabled={savingPreset}
            style={{
              padding: '0.5rem 1rem',
              background: 'var(--accent)',
              border: 'none',
              borderRadius: 6,
              color: 'var(--bg)',
            }}
          >
            {savingPreset ? 'Saving…' : 'Save'}
          </button>
          <button
            type="button"
            onClick={onClose}
            disabled={savingPreset}
            style={{
              padding: '0.5rem 1rem',
              background: 'var(--border)',
              border: 'none',
              borderRadius: 6,
              color: 'var(--text)',
            }}
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
