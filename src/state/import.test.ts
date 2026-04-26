import { beforeEach, describe, expect, it } from 'vitest';
import { useImportStore } from './import';

describe('useImportStore', () => {
  beforeEach(() => {
    useImportStore.getState().reset();
  });

  it('register adds a row with zero totals', () => {
    useImportStore.getState().register({
      importId: 1,
      sourceId: 10,
      sourceName: 'A',
      mode: 'consolidate',
      deleteAfterCopy: false,
    });
    const row = useImportStore.getState().active.get(1);
    expect(row).toBeDefined();
    expect(row?.total).toBe(0);
    expect(row?.finished).toBe(false);
  });

  it('applyProgress updates counts + marks finished only on the backend finished tick', () => {
    useImportStore.getState().register({
      importId: 2,
      sourceId: 20,
      sourceName: 'B',
      mode: 'consolidate',
      deleteAfterCopy: false,
    });
    useImportStore.getState().applyProgress({
      import_id: 2,
      source_id: 20,
      total: 10,
      done: 3,
      current_file: 'IMG_0003.JPG',
      eta_seconds: 42,
      finished: false,
    });
    const mid = useImportStore.getState().active.get(2);
    expect(mid?.done).toBe(3);
    expect(mid?.total).toBe(10);
    expect(mid?.mode).toBe('consolidate');
    expect(mid?.finished).toBe(false);

    useImportStore.getState().applyProgress({
      import_id: 2,
      source_id: 20,
      total: 10,
      done: 10,
      current_file: '',
      eta_seconds: 0,
      finished: true,
    });
    expect(useImportStore.getState().active.get(2)?.finished).toBe(true);
  });

  it('applyProgress flips finished=true even when scan found zero files', () => {
    // Regression: re-importing a folder whose files were just recycled by
    // a source disconnect ends with done=0,total=0. The card used to stick
    // around forever because finished was inferred from `done >= total &&
    // total > 0`. With the explicit `finished` flag from the backend it
    // clears immediately.
    useImportStore.getState().register({
      importId: 4,
      sourceId: 40,
      sourceName: 'EmptySrc',
      mode: 'consolidate',
      deleteAfterCopy: false,
    });
    useImportStore.getState().applyProgress({
      import_id: 4,
      source_id: 40,
      total: 0,
      done: 0,
      current_file: '',
      eta_seconds: 0,
      finished: true,
    });
    expect(useImportStore.getState().active.get(4)?.finished).toBe(true);
  });

  it('applyProgress synthesises a row if register was never called', () => {
    useImportStore.getState().applyProgress({
      import_id: 99,
      source_id: 55,
      total: 4,
      done: 1,
      current_file: 'x',
      eta_seconds: 10,
      finished: false,
    });
    const row = useImportStore.getState().active.get(99);
    expect(row).toBeDefined();
    expect(row?.sourceName).toBe('Source 55');
  });

  it('dismiss removes the row', () => {
    useImportStore.getState().register({
      importId: 3,
      sourceId: 30,
      sourceName: 'C',
      mode: 'consolidate',
      deleteAfterCopy: false,
    });
    useImportStore.getState().dismiss(3);
    expect(useImportStore.getState().active.has(3)).toBe(false);
  });
});
