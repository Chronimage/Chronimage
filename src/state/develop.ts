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

export interface ActiveDevelopMask {
  prompt: string;
  maskB64: string;
  confidence: number;
  latencyMs: number;
  createdAt: string;
}

/** Drawable mask kinds — when set, DevelopScreen captures the next
 *  click+drag on the canvas to define the gradient/radial bounds and
 *  posts a new mask via `createManualMask`. */
export type DrawMaskKind = 'linear_gradient' | 'radial_gradient' | 'brush';

interface DevelopUiState {
  focusedPhotoId: number | null;
  preview: string | null;
  operations: DevelopOperations | null;
  operationSource: OperationSource | null;
  activeMask: ActiveDevelopMask | null;
  selectedMaskId: number | null;
  maskOverlayVisible: boolean;
  maskOverlayOpacity: number;
  drawMaskKind: DrawMaskKind | null;
  /**
   * Open/closed state for each `<CollapsibleSection>` in the editor inspector,
   * keyed by section id (e.g. `'light'`, `'curves'`). Survives photo
   * switches inside one session so the user's panel layout sticks.
   */
  panelOpen: Record<string, boolean>;
  setFocusedPhotoId: (id: number | null) => void;
  setPreview: (url: string | null) => void;
  setOperations: (operations: DevelopOperations | null, source: OperationSource) => void;
  setActiveMask: (mask: ActiveDevelopMask | null) => void;
  setSelectedMaskId: (id: number | null) => void;
  setMaskOverlayVisible: (visible: boolean) => void;
  setMaskOverlayOpacity: (opacity: number) => void;
  setDrawMaskKind: (kind: DrawMaskKind | null) => void;
  setPanelOpen: (id: string, open: boolean) => void;
}

const DEFAULT_PANEL_OPEN: Record<string, boolean> = {
  // Right inspector
  light: true,
  curves: true,
  color: true,
  'color-mixer': false,
  'color-grading': false,
  effects: false,
  detail: false,
  optics: false,
  geometry: false,
  'lens-blur': false,
  // Left rail (DevelopSidePanel)
  presets: true,
  masks: true,
};

export const useDevelopUi = create<DevelopUiState>((set) => ({
  focusedPhotoId: null,
  preview: null,
  operations: null,
  operationSource: null,
  activeMask: null,
  selectedMaskId: null,
  maskOverlayVisible: true,
  maskOverlayOpacity: 62,
  drawMaskKind: null,
  panelOpen: DEFAULT_PANEL_OPEN,
  setFocusedPhotoId: (id) => set({ focusedPhotoId: id }),
  setPreview: (url) => set({ preview: url }),
  setOperations: (operations, operationSource) => set({ operations, operationSource }),
  setActiveMask: (activeMask) => set({ activeMask }),
  setSelectedMaskId: (selectedMaskId) => set({ selectedMaskId }),
  setMaskOverlayVisible: (maskOverlayVisible) => set({ maskOverlayVisible }),
  setMaskOverlayOpacity: (maskOverlayOpacity) => set({ maskOverlayOpacity }),
  setDrawMaskKind: (drawMaskKind) => set({ drawMaskKind }),
  setPanelOpen: (id, open) => set((state) => ({ panelOpen: { ...state.panelOpen, [id]: open } })),
}));
