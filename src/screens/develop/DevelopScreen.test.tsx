import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import React from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useDevelopUi } from '../../state/develop';
import { type DevelopOperations, identityOperations } from '../../tauri/invoke';
import { DevelopScreen } from './DevelopScreen';
import { DevelopSidePanel } from './DevelopSidePanel';

function wrapper({ children }: { children: React.ReactNode }) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return React.createElement(QueryClientProvider, { client }, children);
}

function photoFixture(id: number, overrides: Record<string, unknown> = {}) {
  return {
    id,
    sha256: String(id).padStart(64, '0'),
    filename: `IMG_${id}.ARW`,
    width: 7008,
    height: 4672,
    captured_at: '2026-04-01T12:00:00Z',
    imported_at: '2026-04-22T10:00:00Z',
    is_raw: true,
    size_bytes: 42_000_000,
    camera_make: 'Sony',
    camera_model: 'ILCE-7M4',
    aperture: 2.8,
    shutter: '1/500',
    iso: 400,
    focal_mm: 50,
    aesthetic_score: 7.5,
    paired_photo_id: null,
    raw_format: 'ARW',
    orientation: 1,
    sharpness_score: 620,
    ...overrides,
  };
}

function previewReceipt(photoId: number) {
  return { photo_id: photoId, preview_data_url: 'data:image/jpeg;base64,test', elapsed_ms: 4 };
}

async function mockDevelopInvoke(
  photos: ReturnType<typeof photoFixture>[],
  presets: Array<{
    id: number;
    name: string;
    group_name: string;
    description: string | null;
    operations_json: string;
    is_system: boolean;
    created_at: string;
    updated_at: string;
  }> = [],
) {
  const { invoke } = await import('@tauri-apps/api/core');
  const maskRows: Array<Record<string, unknown>> = [];
  let nextMaskId = 44;
  vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
    const callArgs = (args ?? {}) as Record<string, unknown>;
    if (cmd === 'list_photos') return photos;
    if (cmd === 'develop_open') {
      const photoId = Number(callArgs.photoId ?? photos[0]?.id ?? 1);
      return {
        photo_id: photoId,
        operations: identityOperations(),
        preview_data_url: 'data:image/jpeg;base64,test',
      };
    }
    if (cmd === 'develop_apply') return previewReceipt(Number(callArgs.photoId ?? 1));
    if (cmd === 'develop_save') return 123;
    if (cmd === 'develop_reset') return 1;
    if (cmd === 'develop_paste_edits') return { pasted_photo_count: 1, skipped: [] };
    if (cmd === 'presets_list') return presets;
    if (cmd === 'develop_masks_list') {
      const photoId = Number(callArgs.photoId ?? 1);
      return maskRows.filter((mask) => mask.photo_id === photoId);
    }
    if (cmd === 'develop_mask_create') {
      const req = callArgs.req as Record<string, unknown>;
      const mask = {
        id: nextMaskId++,
        photo_id: Number(req.photo_id ?? 1),
        edit_id: null,
        name: req.name ?? 'Mask',
        source: req.source ?? 'brush',
        mode: req.mode ?? 'normal',
        visible: true,
        order_index: maskRows.length,
        payload_storage: req.payload_storage ?? 'inline',
        mask_payload: JSON.stringify(req.mask_payload ?? { kind: req.source ?? 'brush' }),
        operations_json: JSON.stringify(req.operations ?? identityOperations()),
        confidence: null,
        created_at: '2026-04-01T00:00:00Z',
        updated_at: '2026-04-01T00:00:00Z',
      };
      maskRows.push(mask);
      return mask.id;
    }
    if (cmd === 'develop_mask_generate') {
      const req = callArgs.req as Record<string, unknown>;
      const mask = {
        id: nextMaskId++,
        photo_id: Number(req.photo_id ?? 1),
        edit_id: null,
        name: req.name ?? 'Subject mask',
        source: req.source ?? 'subject',
        mode: req.mode ?? 'normal',
        visible: true,
        order_index: maskRows.length,
        payload_storage: 'inline',
        mask_payload: JSON.stringify({
          kind: 'bitmap',
          source: req.source ?? 'subject',
          model: 'local-segmentation-v1',
          format: 'png-luma8',
          width: 2,
          height: 2,
          data_b64: 'mask-png',
        }),
        operations_json: JSON.stringify(req.operations ?? identityOperations()),
        confidence: 0.72,
        created_at: '2026-04-01T00:00:00Z',
        updated_at: '2026-04-01T00:00:00Z',
      };
      maskRows.push(mask);
      return {
        mask,
        preview_data_url: 'data:image/jpeg;base64,masked',
        elapsed_ms: 3,
      };
    }
    if (cmd === 'develop_mask_update') {
      const req = callArgs.req as Record<string, unknown>;
      const maskId = Number(req.mask_id);
      const index = maskRows.findIndex((mask) => mask.id === maskId);
      const current = index >= 0 ? maskRows[index] : maskRows[0];
      const updated = {
        ...current,
        ...(typeof req.name === 'string' ? { name: req.name } : {}),
        ...(typeof req.source === 'string' ? { source: req.source } : {}),
        ...(typeof req.mode === 'string' ? { mode: req.mode } : {}),
        ...(typeof req.visible === 'boolean' ? { visible: req.visible } : {}),
        ...(req.mask_payload ? { mask_payload: JSON.stringify(req.mask_payload) } : {}),
        ...(req.operations ? { operations_json: JSON.stringify(req.operations) } : {}),
        updated_at: '2026-04-01T00:00:01Z',
      };
      if (index >= 0) maskRows[index] = updated;
      return updated;
    }
    if (cmd === 'develop_mask_delete') {
      const maskId = Number(callArgs.maskId);
      const index = maskRows.findIndex((mask) => mask.id === maskId);
      if (index >= 0) maskRows.splice(index, 1);
      return 1;
    }
    if (cmd === 'develop_mask_apply_preview') return previewReceipt(Number(callArgs.photoId ?? 1));
    if (cmd === 'prompt_sidecar_ping') {
      return { configured: true, url: 'http://localhost:17183', reachable: true, model: 'sam2', error: null };
    }
    if (cmd === 'mask_from_prompt') return { mask_b64: 'mask-png', confidence: 0.92, latency_ms: 12 };
    if (cmd === 'prompt_edit')
      return { image_b64: 'rendered-png', latency_ms: 20, model_id: 'flux-dev', seed: 7 };
    if (cmd === 'prompt_edit_list') {
      return [
        {
          id: 9,
          photo_id: Number(callArgs.photoId ?? 1),
          prompt: 'edit',
          strength: 65,
          constraints_json: '[]',
          mask_b64: null,
          rendered_b64: 'rendered-png',
          model_id: 'flux-dev',
          seed: 7,
          latency_ms: 20,
          state: 'pending',
          created_at: '2026-04-01T00:00:00Z',
        },
      ];
    }
    if (cmd === 'get_thumbnail') return [];
    return [];
  });
  return vi.mocked(invoke);
}

function lastCallArg(
  invoke: {
    mock: { calls: unknown[][] };
  },
  command: string,
): Record<string, unknown> {
  const call = invoke.mock.calls.filter(([cmd]) => cmd === command).at(-1);
  if (!call) throw new Error(`missing ${command} call`);
  return (call[1] ?? {}) as Record<string, unknown>;
}

beforeEach(() => {
  useDevelopUi.setState({
    focusedPhotoId: null,
    preview: null,
    operations: null,
    operationSource: null,
    activeMask: null,
    selectedMaskId: null,
    maskOverlayVisible: true,
    maskOverlayOpacity: 62,
  });
});

describe('DevelopScreen', () => {
  it('renders the pick-a-photo empty state when no catalog photos exist', async () => {
    render(<DevelopScreen />, { wrapper });
    expect(await screen.findByText(/pick a photo to develop/i)).toBeInTheDocument();
  });

  it('renders the develop stage + inspector when a photo is available', async () => {
    await mockDevelopInvoke([photoFixture(1)]);

    render(<DevelopScreen />, { wrapper });

    expect(await screen.findByText(/exposure/i)).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: /auto light/i })).toBeInTheDocument();
  });

  it('auto-light sets default values on click', async () => {
    await mockDevelopInvoke([photoFixture(1)]);

    render(<DevelopScreen />, { wrapper });

    const autoLight = await screen.findByRole('button', { name: /auto light/i });
    fireEvent.click(autoLight);
    // Exposure slider's editable value field should now read `+12 EV`
    // after auto-light. The value lives in an `<input>` rather than a
    // text node, so we query by display value (Lightroom-style click-
    // to-edit numeric read-out, see `primitives/Slider.tsx`).
    expect(await screen.findByDisplayValue('+12 EV')).toBeInTheDocument();
  });

  it('slider edits apply to preview and Save persists the same current operations', async () => {
    const invoke = await mockDevelopInvoke([photoFixture(1)]);
    render(<DevelopScreen />, { wrapper });

    fireEvent.change(await screen.findByLabelText('Exposure'), { target: { value: '25' } });

    await waitFor(() => {
      const applyArgs = lastCallArg(invoke, 'develop_apply');
      expect((applyArgs.operations as DevelopOperations).exposure).toBe(1);
    });

    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));

    await waitFor(() => {
      const saveArgs = lastCallArg(invoke, 'develop_save');
      expect((saveArgs.operations as DevelopOperations).exposure).toBe(1);
    });
  });

  it('copy and paste edits writes the copied operations onto the newly focused photo', async () => {
    const invoke = await mockDevelopInvoke([photoFixture(1), photoFixture(2)]);
    render(<DevelopScreen />, { wrapper });

    fireEvent.change(await screen.findByLabelText('Exposure'), { target: { value: '25' } });
    await waitFor(() => {
      expect((lastCallArg(invoke, 'develop_apply').operations as DevelopOperations).exposure).toBe(1);
    });

    fireEvent.click(screen.getByText('Copy edits'));
    fireEvent.click(screen.getByTitle('IMG_2.ARW'));
    await waitFor(() => expect(screen.getAllByText(/IMG_2\.ARW/).length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole('button', { name: /paste/i }));

    await waitFor(() => {
      const pasteArgs = lastCallArg(invoke, 'develop_paste_edits');
      expect(pasteArgs.photoIds).toEqual([2]);
      expect((pasteArgs.operations as DevelopOperations).exposure).toBe(1);
    });
    expect(await screen.findByDisplayValue('+25 EV')).toBeInTheDocument();
  });

  it('reset clears UI values and re-renders the identity operations', async () => {
    const invoke = await mockDevelopInvoke([photoFixture(1)]);
    render(<DevelopScreen />, { wrapper });

    fireEvent.click(await screen.findByRole('button', { name: /auto light/i }));
    expect(await screen.findByDisplayValue('+12 EV')).toBeInTheDocument();
    fireEvent.click(screen.getByTitle('Reset edits'));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('develop_reset', { photoId: 1 });
      const applyArgs = lastCallArg(invoke, 'develop_apply');
      expect((applyArgs.operations as DevelopOperations).exposure).toBe(0);
    });
    expect(await screen.findByDisplayValue('0 EV')).toBeInTheDocument();
  });

  it('preset applies update the screen operations so Save persists the previewed edit once', async () => {
    const presetOps = { ...identityOperations(), exposure: 1, contrast: 20 };
    const invoke = await mockDevelopInvoke(
      [photoFixture(1)],
      [
        {
          id: 7,
          name: 'Enhance sky',
          group_name: 'Scene',
          description: 'Deeper blues',
          operations_json: JSON.stringify(presetOps),
          is_system: true,
          created_at: '2026-04-01T00:00:00Z',
          updated_at: '2026-04-01T00:00:00Z',
        },
      ],
    );

    // DevelopScreen now renders the sidepanel internally as the leftmost
    // ResizablePanel, so the test no longer needs to mount it separately.
    render(
      <React.StrictMode>
        <DevelopScreen />
      </React.StrictMode>,
      { wrapper },
    );

    await screen.findByText(/photo 1/i);
    // Preset categories migrated to shadcn ToggleGroup (radio role).
    fireEvent.click(screen.getByRole('radio', { name: 'Scene' }));
    invoke.mockClear();
    fireEvent.click(screen.getByRole('button', { name: /enhance sky/i }));
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));

    await waitFor(() => {
      const applyCalls = invoke.mock.calls.filter(([cmd]) => cmd === 'develop_apply');
      expect(applyCalls).toHaveLength(1);
      expect(((applyCalls[0]?.[1] as Record<string, unknown>).operations as DevelopOperations).exposure).toBe(
        0.75,
      );
    });

    await waitFor(() => {
      const saveArgs = lastCallArg(invoke, 'develop_save');
      expect((saveArgs.operations as DevelopOperations).exposure).toBe(0.75);
      expect((saveArgs.operations as DevelopOperations).contrast).toBe(15);
    });
  });

  it('generates a bitmap quick mask without the prompt sidecar', async () => {
    const invoke = await mockDevelopInvoke([photoFixture(1)]);
    render(
      <>
        <DevelopSidePanel />
        <DevelopScreen />
      </>,
      { wrapper },
    );

    const subject = await screen.findByRole('button', { name: /subject/i });
    await waitFor(() => expect(subject).not.toBeDisabled());
    fireEvent.click(subject);

    await waitFor(() => {
      const generateArgs = lastCallArg(invoke, 'develop_mask_generate');
      expect(generateArgs.req).toMatchObject({
        photo_id: 1,
        name: 'Subject mask',
        source: 'subject',
        mode: 'normal',
      });
    });
    expect(await screen.findByTestId('selected-mask-overlay')).toBeInTheDocument();
    expect(screen.getAllByText(/subject mask/i).length).toBeGreaterThan(0);
    expect(invoke.mock.calls.some(([cmd]) => cmd === 'develop_mask_create')).toBe(false);
    expect(invoke.mock.calls.some(([cmd]) => cmd === 'mask_from_prompt')).toBe(false);
  });

  it('updates selected mask adjustments and refreshes the preview', async () => {
    const invoke = await mockDevelopInvoke([photoFixture(1)]);
    render(
      <>
        <DevelopSidePanel />
        <DevelopScreen />
      </>,
      { wrapper },
    );

    const subject = await screen.findByRole('button', { name: /subject/i });
    await waitFor(() => expect(subject).not.toBeDisabled());
    fireEvent.click(subject);
    await waitFor(() => {
      expect(lastCallArg(invoke, 'develop_mask_generate').req).toMatchObject({ photo_id: 1 });
    });
    await screen.findByTestId('selected-mask-overlay');
    invoke.mockClear();

    fireEvent.change(screen.getByLabelText('Exposure'), { target: { value: '25' } });

    await waitFor(() => {
      const updateArgs = lastCallArg(invoke, 'develop_mask_update');
      const req = updateArgs.req as { mask_id: number; operations: DevelopOperations };
      expect(req.mask_id).toBe(44);
      expect(req.operations.exposure).toBe(1);
      expect(req.operations.crop_w).toBe(1);
    });

    await waitFor(() => {
      expect(invoke.mock.calls.some(([cmd]) => cmd === 'develop_mask_apply_preview')).toBe(true);
    });
  });

  it('uses explicit crop mode for direct canvas crop dragging', async () => {
    const invoke = await mockDevelopInvoke([photoFixture(1)]);
    const rectSpy = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      width: 700,
      height: 467,
      top: 0,
      left: 0,
      right: 700,
      bottom: 467,
      toJSON: () => ({}),
    });

    try {
      render(<DevelopScreen />, { wrapper });

      fireEvent.click(await screen.findByTitle('Crop directly on the canvas'));
      await waitFor(() => expect(invoke.mock.calls.some(([cmd]) => cmd === 'develop_apply')).toBe(true));
      invoke.mockClear();

      const frame = screen.getByTestId('develop-canvas-frame');
      fireEvent.pointerDown(screen.getByLabelText(/resize crop nw/i), {
        pointerId: 1,
        clientX: 0,
        clientY: 0,
      });
      fireEvent.pointerMove(frame, { pointerId: 1, clientX: 70, clientY: 47 });
      fireEvent.pointerUp(frame, { pointerId: 1 });
      fireEvent.click(screen.getByRole('button', { name: 'Done' }));

      await waitFor(() => {
        const applyArgs = lastCallArg(invoke, 'develop_apply');
        const ops = applyArgs.operations as DevelopOperations;
        expect(ops.crop_x ?? 0).toBeGreaterThan(0.09);
        expect(ops.crop_y ?? 0).toBeGreaterThan(0.09);
        expect(ops.crop_w ?? 1).toBeLessThan(0.92);
      });
    } finally {
      rectSpy.mockRestore();
    }
  });
});

describe('DevelopSidePanel', () => {
  it('switches preset category on click', () => {
    render(<DevelopSidePanel />, { wrapper });
    // Preset categories are now a shadcn ToggleGroup (single-select), so
    // each option is a radio. The selected one carries `aria-checked`,
    // not `aria-pressed`.
    const sceneBtn = screen.getByRole('radio', { name: 'Scene' });
    fireEvent.click(sceneBtn);
    expect(sceneBtn).toHaveAttribute('aria-checked', 'true');
  });

  it('shows the empty-state message in the custom-presets tab when none exist', () => {
    render(<DevelopSidePanel />, { wrapper });
    fireEvent.click(screen.getByRole('radio', { name: /my presets/i }));
    expect(screen.getByText(/no custom presets yet/i)).toBeInTheDocument();
  });
});
