import { beforeEach, describe, expect, it } from 'vitest';
import { useSourceDeleteStore } from './sourceDelete';

describe('useSourceDeleteStore', () => {
  beforeEach(() => {
    useSourceDeleteStore.getState().reset();
  });

  it('register inserts a row in collecting state with the given name', () => {
    useSourceDeleteStore.getState().register(7, 'iPhone Backup');
    const row = useSourceDeleteStore.getState().active.get(7);
    expect(row).toBeDefined();
    expect(row?.sourceName).toBe('iPhone Backup');
    expect(row?.phase).toBe('collecting');
    expect(row?.finished).toBe(false);
    expect(row?.total).toBe(0);
    expect(row?.done).toBe(0);
  });

  it('applyProgress advances the phase and counts without flipping finished too early', () => {
    useSourceDeleteStore.getState().register(11, 'Selfies');
    useSourceDeleteStore.getState().applyProgress({
      source_id: 11,
      phase: 'deleting',
      total: 100,
      done: 42,
    });
    const mid = useSourceDeleteStore.getState().active.get(11);
    expect(mid?.phase).toBe('deleting');
    expect(mid?.total).toBe(100);
    expect(mid?.done).toBe(42);
    expect(mid?.finished).toBe(false);
    // Name from register is preserved across applyProgress ticks.
    expect(mid?.sourceName).toBe('Selfies');
  });

  it('applyProgress flips finished=true on the done phase', () => {
    useSourceDeleteStore.getState().register(12, 'Old Camera');
    useSourceDeleteStore.getState().applyProgress({
      source_id: 12,
      phase: 'done',
      total: 50,
      done: 50,
    });
    expect(useSourceDeleteStore.getState().active.get(12)?.finished).toBe(true);
  });

  it('committed phase keeps finished=false (DB done, but recycle still running)', () => {
    useSourceDeleteStore.getState().register(13, 'Trip 2024');
    useSourceDeleteStore.getState().applyProgress({
      source_id: 13,
      phase: 'committed',
      total: 200,
      done: 200,
    });
    expect(useSourceDeleteStore.getState().active.get(13)?.finished).toBe(false);
  });

  it('applyProgress synthesises a row when register was never called', () => {
    useSourceDeleteStore.getState().applyProgress({
      source_id: 99,
      phase: 'thumb_cleanup',
      total: 5,
      done: 3,
    });
    const row = useSourceDeleteStore.getState().active.get(99);
    expect(row?.sourceName).toBe('Source 99');
    expect(row?.phase).toBe('thumb_cleanup');
    expect(row?.done).toBe(3);
  });

  it('dismiss removes the row', () => {
    useSourceDeleteStore.getState().register(1, 'A');
    useSourceDeleteStore.getState().dismiss(1);
    expect(useSourceDeleteStore.getState().active.has(1)).toBe(false);
  });

  it('reset clears every active row', () => {
    useSourceDeleteStore.getState().register(1, 'A');
    useSourceDeleteStore.getState().register(2, 'B');
    useSourceDeleteStore.getState().reset();
    expect(useSourceDeleteStore.getState().active.size).toBe(0);
  });
});
