import { fireEvent, render } from '@testing-library/react';
import { describe, expect, test, vi } from 'vitest';
import { identityCurve, identityCurves } from '../../tauri/invoke';
import { CurvesPanel } from './CurvesPanel';

/**
 * Vitest lives under jsdom which doesn't lay out SVG viewBox coordinates,
 * so `getBoundingClientRect()` returns zeros unless we stub. These tests
 * stub the rect so `clientToNormalised` can resolve drag coordinates
 * deterministically.
 */
function stubSvgRect(width = 200, height = 200) {
  const proto = SVGSVGElement.prototype as unknown as {
    getBoundingClientRect: () => DOMRect;
  };
  proto.getBoundingClientRect = () => ({
    left: 0,
    top: 0,
    right: width,
    bottom: height,
    width,
    height,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  });
}

describe('CurvesPanel', () => {
  test('renders the 5 draggable control points for the active channel', () => {
    stubSvgRect();
    const onChange = vi.fn();
    const setChannel = vi.fn();
    const { container } = render(
      <CurvesPanel value={identityCurves()} onChange={onChange} channel="rgb" setChannel={setChannel} />,
    );
    const handles = container.querySelectorAll('circle[role="slider"]');
    expect(handles.length).toBe(5);
  });

  test('dragging a midpoint emits an updated curve via onChange', () => {
    stubSvgRect(200, 200);
    const onChange = vi.fn();
    const setChannel = vi.fn();
    const { container } = render(
      <CurvesPanel value={identityCurves()} onChange={onChange} channel="rgb" setChannel={setChannel} />,
    );

    const svg = container.querySelector('svg');
    const handles = container.querySelectorAll('circle[role="slider"]');
    if (!svg) throw new Error('svg missing');
    // Midpoint handle (index 2) sits at (x=0.5, y=0.5) → (100, 100) in the
    // 200-px stubbed rect. Lift the mid by 50 px (y → 0.75).
    fireEvent.pointerDown(handles[2] as Element, { pointerId: 1, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(svg, { pointerId: 1, clientX: 100, clientY: 50 });
    fireEvent.pointerUp(svg, { pointerId: 1, clientX: 100, clientY: 50 });

    expect(onChange).toHaveBeenCalled();
    const lastCall = onChange.mock.calls.at(-1)?.[0];
    expect(lastCall).toBeTruthy();
    expect(lastCall.rgb[2][0]).toBeCloseTo(0.5, 2);
    // y=0.75 after the +50px lift (SVG y is inverted).
    expect(lastCall.rgb[2][1]).toBeGreaterThan(0.6);
    expect(lastCall.rgb[2][1]).toBeLessThan(0.9);
  });

  test('channel switcher calls setChannel on click', () => {
    stubSvgRect();
    const setChannel = vi.fn();
    const { getByLabelText } = render(
      <CurvesPanel value={identityCurves()} onChange={() => {}} channel="rgb" setChannel={setChannel} />,
    );
    fireEvent.click(getByLabelText('R channel'));
    expect(setChannel).toHaveBeenCalledWith('r');
  });

  test('click on empty curve area inserts a new control point', () => {
    stubSvgRect(200, 200);
    const onChange = vi.fn();
    const { container } = render(
      <CurvesPanel value={identityCurves()} onChange={onChange} channel="rgb" setChannel={() => {}} />,
    );
    const svg = container.querySelector('svg');
    if (!svg) throw new Error('svg missing');

    // Click somewhere away from existing control points. Identity stops
    // sit at x = 0, 0.25, 0.5, 0.75, 1.0 — click at (0.1, 0.1) in normalised
    // space = (20, 180) pixels (SVG y inverted).
    fireEvent.pointerDown(svg, { pointerId: 7, clientX: 20, clientY: 180 });
    fireEvent.pointerUp(svg, { pointerId: 7, clientX: 20, clientY: 180 });

    expect(onChange).toHaveBeenCalled();
    const next = onChange.mock.calls.at(-1)?.[0];
    expect(next.rgb.length).toBe(6);
    // Inserted between idx 0 (x=0) and idx 1 (x=0.25).
    expect(next.rgb[1][0]).toBeCloseTo(0.1, 2);
  });

  test('right-click on a midpoint removes it', () => {
    stubSvgRect(200, 200);
    const onChange = vi.fn();
    const { container } = render(
      <CurvesPanel value={identityCurves()} onChange={onChange} channel="rgb" setChannel={() => {}} />,
    );
    const handles = container.querySelectorAll('circle[role="slider"]');
    // The mid handle at idx=2 (x=0.5, y=0.5). Right-click.
    fireEvent.contextMenu(handles[2] as Element);
    expect(onChange).toHaveBeenCalled();
    const next = onChange.mock.calls.at(-1)?.[0];
    expect(next.rgb.length).toBe(4);
    // The midpoint is gone — remaining x values are 0, 0.25, 0.75, 1.
    expect(next.rgb.map((p: [number, number]) => p[0])).toEqual([0, 0.25, 0.75, 1]);
  });

  test('right-click on an endpoint is a no-op', () => {
    stubSvgRect(200, 200);
    const onChange = vi.fn();
    const { container } = render(
      <CurvesPanel value={identityCurves()} onChange={onChange} channel="rgb" setChannel={() => {}} />,
    );
    const handles = container.querySelectorAll('circle[role="slider"]');
    fireEvent.contextMenu(handles[0] as Element); // first endpoint
    fireEvent.contextMenu(handles[handles.length - 1] as Element); // last endpoint
    expect(onChange).not.toHaveBeenCalled();
  });

  test('reset button restores the identity curve on the active channel', () => {
    stubSvgRect();
    const onChange = vi.fn();
    const nonIdentity = {
      ...identityCurves(),
      rgb: [
        [0, 0],
        [0.25, 0.5],
        [0.5, 0.8],
        [0.75, 0.9],
        [1, 1],
      ] as ReturnType<typeof identityCurve>,
    };
    const { getByLabelText } = render(
      <CurvesPanel value={nonIdentity} onChange={onChange} channel="rgb" setChannel={() => {}} />,
    );
    fireEvent.click(getByLabelText('Reset curve'));
    expect(onChange).toHaveBeenCalled();
    const next = onChange.mock.calls.at(-1)?.[0];
    expect(next.rgb).toEqual(identityCurve());
  });
});
