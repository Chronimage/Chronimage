/**
 * AddSourceModal — full-screen dialog that wraps `AddSourcePopover`.
 *
 * The sidebar's old behaviour expanded the AddSourcePopover inline
 * inside the narrow 280-px sidepanel column, which cramped the three
 * action buttons into a single stack and pushed the sources list down
 * the screen. This dialog presents the same workflow in a dedicated
 * centred modal — larger hit targets, side-by-side action buttons,
 * clear title + close button — matching the `CopyConfirmModal` style.
 *
 * Composition: the inner content is the existing `AddSourcePopover`
 * with `layout="block"` so it renders the three actions as a horizontal
 * row. The copy-confirmation flow inside the popover is unchanged and
 * opens on top of this modal as a separate overlay.
 */

import { useEffect } from 'react';
import { AddSourcePopover } from './AddSourcePopover';

export interface AddSourceModalProps {
  open: boolean;
  onClose: () => void;
}

export function AddSourceModal({ open, onClose }: AddSourceModalProps) {
  // Close on Escape. Installed only while the modal is mounted so we
  // don't steal the key elsewhere.
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Add a photo source"
      className="add-source-modal-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      onKeyDown={(e) => {
        if ((e.key === 'Enter' || e.key === ' ') && e.target === e.currentTarget) {
          e.preventDefault();
          onClose();
        }
      }}
    >
      <div className="add-source-modal">
        <div className="add-source-modal-head">
          <div>
            <div className="mono add-source-modal-eyebrow">ADD A SOURCE</div>
            <h2 className="page-title add-source-modal-title">
              Bring in photos<em>.</em>
            </h2>
          </div>
          <button
            type="button"
            className="add-source-modal-close"
            onClick={onClose}
            aria-label="Close"
            title="Close (Esc)"
          >
            <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
              <path
                d="M6 6l12 12M18 6L6 18"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                fill="none"
              />
            </svg>
          </button>
        </div>
        <div className="add-source-modal-body">
          <AddSourcePopover layout="block" />
        </div>
      </div>
    </div>
  );
}
