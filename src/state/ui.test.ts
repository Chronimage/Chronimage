import { act } from 'react';
import { describe, expect, it } from 'vitest';
import { DEFAULT_TWEAKS, SCREENS, useUi } from './ui';

describe('useUi', () => {
  it('starts on the catalog screen', () => {
    const { screen } = useUi.getState();
    expect(screen.id).toBe('catalog');
  });

  it('setScreen switches to the requested screen', () => {
    act(() => {
      useUi.getState().setScreen('catalog');
    });
    expect(useUi.getState().screen).toEqual(SCREENS.catalog);
  });

  it('setTweaks merges partial updates', () => {
    act(() => {
      useUi.getState().setTweaks({ theme: 'light' });
    });
    const { tweaks } = useUi.getState();
    expect(tweaks.theme).toBe('light');
    // Untouched fields retain defaults.
    expect(tweaks.accent).toBe(DEFAULT_TWEAKS.accent);
  });
});
