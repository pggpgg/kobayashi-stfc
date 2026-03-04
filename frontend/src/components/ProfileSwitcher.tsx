import { useState, useRef, useEffect } from 'react';
import { useProfile } from '../contexts/ProfileContext';
import { createProfile, deleteProfile } from '../lib/api';

/** Derive initials from profile name (e.g. "Main" -> "M", "Alt Account" -> "AA"). */
function initials(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return '?';
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[words.length - 1][0]).toUpperCase();
}

export default function ProfileSwitcher() {
  const { activeProfileId, setActiveProfileId, profiles, refreshProfiles } = useProfile();
  const [open, setOpen] = useState(false);
  const [showAdd, setShowAdd] = useState(false);
  const [newName, setNewName] = useState('');
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<{ id: string; name: string } | null>(null);
  const [deleting, setDeleting] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const activeProfile = profiles.find((p) => p.id === activeProfileId);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setOpen(false);
        setShowAdd(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const handleAddProfile = async () => {
    if (!newName.trim()) return;
    setCreateError(null);
    setCreating(true);
    try {
      const created = await createProfile({ name: newName.trim() });
      await refreshProfiles();
      setActiveProfileId(created.id);
      setNewName('');
      setShowAdd(false);
    } catch (e) {
      setCreateError(e instanceof Error ? e.message : 'Failed to create profile');
    } finally {
      setCreating(false);
    }
  };

  const handleDeleteProfile = async () => {
    if (!deleteConfirm) return;
    setDeleting(true);
    try {
      await deleteProfile(deleteConfirm.id);
      await refreshProfiles();
      if (activeProfileId === deleteConfirm.id) {
        const remaining = profiles.filter((p) => p.id !== deleteConfirm.id);
        setActiveProfileId(remaining[0]?.id ?? '');
      }
      setDeleteConfirm(null);
      setOpen(false);
    } catch (e) {
      setCreateError(e instanceof Error ? e.message : 'Failed to delete profile');
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div ref={dropdownRef} style={{ position: 'relative' }}>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        title={activeProfile?.name ?? 'Switch profile'}
        style={{
          width: 36,
          height: 36,
          borderRadius: '50%',
          border: '1px solid var(--border)',
          background: 'var(--surface)',
          color: 'var(--text)',
          fontSize: '0.85rem',
          fontWeight: 600,
          cursor: 'pointer',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
        }}
      >
        {activeProfile ? initials(activeProfile.name) : '?'}
      </button>

      {open && (
        <div
          style={{
            position: 'absolute',
            top: '100%',
            right: 0,
            marginTop: 4,
            minWidth: 180,
            background: 'var(--surface)',
            border: '1px solid var(--border)',
            borderRadius: 8,
            boxShadow: '0 4px 12px rgba(0,0,0,0.15)',
            zIndex: 1000,
            padding: 4,
          }}
        >
          {profiles.map((p) => (
            <div
              key={p.id}
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                borderRadius: 4,
                background: p.id === activeProfileId ? 'var(--accent)' : 'transparent',
              }}
            >
              <button
                type="button"
                onClick={() => {
                  setActiveProfileId(p.id);
                  setOpen(false);
                }}
                style={{
                  flex: 1,
                  padding: '0.5rem 0.75rem',
                  textAlign: 'left',
                  background: 'transparent',
                  color: p.id === activeProfileId ? 'var(--bg)' : 'var(--text)',
                  border: 'none',
                  cursor: 'pointer',
                  fontSize: '0.9rem',
                }}
              >
                <span style={{ fontWeight: 600, marginRight: 6 }}>{initials(p.name)}</span>
                {p.name}
              </button>
              {profiles.length > 1 && (
                <button
                  type="button"
                  onClick={() => setDeleteConfirm({ id: p.id, name: p.name })}
                  title={`Delete ${p.name}`}
                  style={{
                    padding: '0.25rem 0.5rem',
                    background: 'transparent',
                    color: p.id === activeProfileId ? 'var(--bg)' : 'var(--text-muted)',
                    border: 'none',
                    cursor: 'pointer',
                    fontSize: '0.8rem',
                    opacity: 0.7,
                  }}
                >
                  ×
                </button>
              )}
            </div>
          ))}

          {showAdd ? (
            <div style={{ padding: 8, borderTop: '1px solid var(--border)', marginTop: 4 }}>
              <input
                type="text"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="Profile name"
                autoFocus
                onKeyDown={(e) => {
                  if (e.key === 'Enter') handleAddProfile();
                  if (e.key === 'Escape') setShowAdd(false);
                }}
                style={{
                  width: '100%',
                  padding: '0.4rem',
                  marginBottom: 6,
                  background: 'var(--bg)',
                  border: '1px solid var(--border)',
                  borderRadius: 4,
                  color: 'var(--text)',
                }}
              />
              {createError && (
                <div style={{ fontSize: '0.8rem', color: 'var(--error)', marginBottom: 4 }}>
                  {createError}
                </div>
              )}
              <div style={{ display: 'flex', gap: 4 }}>
                <button
                  type="button"
                  onClick={handleAddProfile}
                  disabled={creating || !newName.trim()}
                  style={{
                    padding: '0.35rem 0.6rem',
                    background: 'var(--accent)',
                    border: 'none',
                    borderRadius: 4,
                    color: 'var(--bg)',
                    fontSize: '0.85rem',
                    cursor: creating ? 'not-allowed' : 'pointer',
                  }}
                >
                  {creating ? 'Creating…' : 'Add'}
                </button>
                <button
                  type="button"
                  onClick={() => setShowAdd(false)}
                  style={{
                    padding: '0.35rem 0.6rem',
                    background: 'var(--border)',
                    border: 'none',
                    borderRadius: 4,
                    color: 'var(--text)',
                    fontSize: '0.85rem',
                    cursor: 'pointer',
                  }}
                >
                  Cancel
                </button>
              </div>
            </div>
          ) : (
            <button
              type="button"
              onClick={() => setShowAdd(true)}
              style={{
                display: 'block',
                width: '100%',
                padding: '0.5rem 0.75rem',
                textAlign: 'left',
                background: 'transparent',
                color: 'var(--text-muted)',
                border: 'none',
                borderRadius: 4,
                cursor: 'pointer',
                fontSize: '0.85rem',
                marginTop: 2,
              }}
            >
              + Add profile
            </button>
          )}
        </div>
      )}

      {deleteConfirm && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            background: 'rgba(0,0,0,0.5)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 2000,
          }}
          onClick={() => !deleting && setDeleteConfirm(null)}
        >
          <div
            style={{
              background: 'var(--surface)',
              padding: '1.5rem',
              borderRadius: 8,
              border: '1px solid var(--border)',
              minWidth: 280,
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <p style={{ margin: '0 0 1rem', fontWeight: 600 }}>
              Delete profile &quot;{deleteConfirm.name}&quot;?
            </p>
            <p style={{ margin: '0 0 1rem', fontSize: '0.9rem', color: 'var(--text-muted)' }}>
              This will permanently delete all roster, research, buildings, ships, and presets for
              this profile. This cannot be undone.
            </p>
            <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
              <button
                type="button"
                onClick={() => setDeleteConfirm(null)}
                disabled={deleting}
                style={{
                  padding: '0.5rem 1rem',
                  background: 'var(--border)',
                  border: 'none',
                  borderRadius: 6,
                  color: 'var(--text)',
                  cursor: deleting ? 'not-allowed' : 'pointer',
                }}
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleDeleteProfile}
                disabled={deleting}
                style={{
                  padding: '0.5rem 1rem',
                  background: 'var(--error)',
                  border: 'none',
                  borderRadius: 6,
                  color: 'white',
                  cursor: deleting ? 'not-allowed' : 'pointer',
                }}
              >
                {deleting ? 'Deleting…' : 'Delete'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
