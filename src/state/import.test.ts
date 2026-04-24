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

  it('applyProgress updates counts + marks finished at done==total', () => {
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
    });
    expect(useImportStore.getState().active.get(2)?.finished).toBe(true);
  });

  it('applyProgress synthesises a row if register was never called', () => {
    useImportStore.getState().applyProgress({
      import_id: 99,
      source_id: 55,
      total: 4,
      done: 1,
      current_file: 'x',
      eta_seconds: 10,
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
