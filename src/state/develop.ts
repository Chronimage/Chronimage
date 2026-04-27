/**
 * Shared UI state for the Develop screen. DevelopSidePanel is an
 * app-shell sibling of DevelopScreen, so this store shares the focused
 * photo plus transient edit state without prop-drilling through app.tsx.
 *
 * The store holds:
 * - `focusedPhotoId` - the photo currently open in the editor, written
 *   by DevelopScreen as the user clicks the filmstrip; read by the
 *   sidepanel so preset mutations know which photo to target.
 * - `preview` - latest preview data URL. Either side can set this after
 *   a successful preview apply; DevelopScreen reads and renders it.
 * - `operations` / `operationSource` - latest unsaved edit operations
 *   and the side that produced them, so preset edits and local controls
 *   stay in sync before Save.
 *
 * Nothing persists; everything here is per-session.
 */

import { create } from 'zustand';
import type { DevelopOperations } from '../tauri/invoke';

type OperationSource = 'screen' | 'sidepanel';

interface DevelopUiState {
  focusedPhotoId: number | null;
  preview: string | null;
  operations: DevelopOperations | null;
  operationSource: OperationSource | null;
  setFocusedPhotoId: (id: number | null) => void;
  setPreview: (url: string | null) => void;
  setOperations: (operations: DevelopOperations | null, source: OperationSource) => void;
}

export const useDevelopUi = create<DevelopUiState>((set) => ({
  focusedPhotoId: null,
  preview: null,
  operations: null,
  operationSource: null,
  setFocusedPhotoId: (id) => set({ focusedPhotoId: id }),
  setPreview: (url) => set({ preview: url }),
  setOperations: (operations, operationSource) => set({ operations, operationSource }),
}));
