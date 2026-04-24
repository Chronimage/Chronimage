/**
 * Shared UI state for the Develop screen — lets the DevelopSidePanel
 * (app-shell sibling of DevelopScreen) push preview updates when the
 * user applies a preset without prop-drilling through app.tsx.
 *
 * The store holds:
 * - `focusedPhotoId` — the photo currently open in the editor, written
 *   by DevelopScreen as the user clicks the filmstrip; read by the
 *   sidepanel so preset mutations know which photo to target.
 * - `preview` — latest preview data URL. Sidepanel sets this after
 *   `develop_preset_apply`; DevelopScreen reads + renders it.
 *
 * Nothing persists — everything here is per-session.
 */

import { create } from 'zustand';

interface DevelopUiState {
  focusedPhotoId: number | null;
  preview: string | null;
  setFocusedPhotoId: (id: number | null) => void;
  setPreview: (url: string | null) => void;
}

export const useDevelopUi = create<DevelopUiState>((set) => ({
  focusedPhotoId: null,
  preview: null,
  setFocusedPhotoId: (id) => set({ focusedPhotoId: id }),
  setPreview: (url) => set({ preview: url }),
}));
