/**
 * TagDropdown — Phase 2 §10 manual-tagging menu for a multi-select.
 *
 * Opens from the Catalog toolbar's Tag button. Contains:
 * - Free-text add box with existing-tag autocomplete
 * - List of tags currently applied (any photo in selection) with remove
 * - Recent tags the user has applied before (quick re-apply)
 *
 * Closes on outside-click + Escape + success toast-less (react-query
 * invalidation refreshes the list in-place).
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useAddUserTag, useRemoveUserTag, useUserTags } from '../state/queries';
import { Chip } from './Chip';
import { Icon } from './Icon';

export interface TagDropdownProps {
  photoIds: number[];
  onClose: () => void;
}

export function TagDropdown({ photoIds, onClose }: TagDropdownProps) {
  const [input, setInput] = useState('');
  const ref = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const { data: tags = [] } = useUserTags();
  const addTag = useAddUserTag();
  const removeTag = useRemoveUserTag();

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    const onDocClick = (e: MouseEvent) => {
      if (!ref.current) return;
      if (!ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('mousedown', onDocClick);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDocClick);
      document.removeEventListener('keydown', onKey);
    };
  }, [onClose]);

  const suggestions = useMemo(() => {
    const q = input.trim().toLowerCase();
    if (!q) return tags.slice(0, 12);
    return tags.filter((t) => t.label.toLowerCase().includes(q)).slice(0, 12);
  }, [input, tags]);

  const onAdd = useCallback(
    (label: string) => {
      const trimmed = label.trim();
      if (!trimmed || photoIds.length === 0) return;
      addTag.mutate(
        { photoIds, label: trimmed },
        {
          onSuccess: () => setInput(''),
        },
      );
    },
    [addTag, photoIds],
  );

  const onRemove = useCallback(
    (label: string) => {
      if (!label || photoIds.length === 0) return;
      removeTag.mutate({ photoIds, label });
    },
    [removeTag, photoIds],
  );

  return (
    <div ref={ref} className="tag-dropdown" role="menu">
      <div className="tag-dropdown-head">
        <span className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)' }}>
          Tag {photoIds.length} photo{photoIds.length === 1 ? '' : 's'}
        </span>
        <button type="button" className="btn" onClick={onClose} aria-label="Close tag menu">
          <Icon name="close" size={12} />
        </button>
      </div>
      <input
        ref={inputRef}
        type="text"
        placeholder="Add tag…"
        value={input}
        onChange={(e) => setInput(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            onAdd(input);
          }
        }}
        className="tag-input"
        aria-label="Tag label"
      />
      <div className="tag-suggestions">
        {suggestions.map((t) => (
          <button
            key={t.label}
            type="button"
            className="tag-suggestion"
            onClick={() => onAdd(t.label)}
            role="menuitem"
          >
            <Icon name="plus" size={11} />
            <span>{t.label}</span>
            <span className="mono" style={{ color: 'var(--fg-mute)', marginLeft: 'auto' }}>
              {t.photo_count}
            </span>
          </button>
        ))}
        {suggestions.length === 0 && input.trim() && (
          <button type="button" className="tag-suggestion" onClick={() => onAdd(input)} role="menuitem">
            <Icon name="plus" size={11} />
            <span>Create “{input.trim()}”</span>
          </button>
        )}
        {suggestions.length === 0 && !input.trim() && (
          <div className="tag-empty">No tags yet — type to create one.</div>
        )}
      </div>
      {tags.length > 0 && (
        <div className="tag-applied">
          <div className="mono" style={{ fontSize: 10, color: 'var(--fg-mute)', marginBottom: 4 }}>
            LIBRARY TAGS
          </div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
            {tags.slice(0, 16).map((t) => (
              <Chip key={t.label} onClose={() => onRemove(t.label)}>
                {t.label}
              </Chip>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
