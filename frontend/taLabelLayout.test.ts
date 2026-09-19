import { describe, expect, it } from 'vitest';
import { layoutTaLabels } from './taLabelLayout';

describe('Tactical Aid label layout', () => {
  it('offsets nearby labels without overlap and preserves every event', () => {
    const anchors = Array.from({ length: 30 }, (_, i) => ({ key: String(i), left: 0.5, top: 0.5 }));
    const groups = layoutTaLabels(anchors, 800, 600);
    expect(groups.some((g) => g.members.length > 1)).toBe(true);
    expect(groups.flatMap((g) => g.members).sort()).toEqual(anchors.map((a) => a.key).sort());
    for (const [i, a] of groups.entries()) {
      for (const b of groups.slice(i + 1)) {
        expect(
          a.left + a.width <= b.left ||
            b.left + b.width <= a.left ||
            a.top + a.height <= b.top ||
            b.top + b.height <= a.top,
        ).toBe(true);
      }
    }
    expect(layoutTaLabels(anchors, 800, 600)).toEqual(groups);
  });

  it('merges stacks when space runs out while preserving non-overlapping bounds', () => {
    const anchors = Array.from({ length: 60 }, (_, i) => ({
      key: String(i),
      left: (i % 10) / 9,
      top: Math.floor(i / 10) / 5,
    }));
    for (const [width, height] of [
      [800, 600],
      [240, 180],
      [180, 100],
    ]) {
      const groups = layoutTaLabels(anchors, width!, height!);
      expect(groups.flatMap((g) => g.members).sort()).toEqual(anchors.map((a) => a.key).sort());
      for (const [i, a] of groups.entries()) {
        expect(a.left + a.width).toBeLessThanOrEqual(width!);
        expect(a.top + a.height).toBeLessThanOrEqual(height!);
        for (const b of groups.slice(i + 1)) {
          expect(
            a.left + a.width <= b.left ||
              b.left + b.width <= a.left ||
              a.top + a.height <= b.top ||
              b.top + b.height <= a.top,
          ).toBe(true);
        }
      }
    }
  });

  it('stacks competing card footprints without requiring three events', () => {
    const groups = layoutTaLabels(
      [
        { key: 'a', left: 0.5, top: 0.5 },
        { key: 'b', left: 0.51, top: 0.51 },
      ],
      800,
      600,
    );
    expect(groups).toHaveLength(1);
    expect(groups[0]!.members).toEqual(['a', 'b']);
  });

  it('compacts a dense same-side cluster without scattering spare individual cards', () => {
    const anchors = Array.from({ length: 12 }, (_, i) => ({
      key: String(i),
      left: 0.5 + (i % 3) * 0.01,
      top: 0.5,
      faction: i % 2 ? 'USSR' : 'USA',
    }));
    const groups = layoutTaLabels(anchors, 800, 600);
    expect(groups).toHaveLength(2);
    expect(groups.map((g) => g.members.length)).toEqual([6, 6]);
    const triple = layoutTaLabels(anchors.filter((a) => a.faction === 'USA').slice(0, 3), 800, 600);
    expect(triple).toHaveLength(1);
    const spread = layoutTaLabels(
      [
        { key: 'a', left: 0.1, top: 0.2 },
        { key: 'b', left: 0.5, top: 0.5 },
        { key: 'c', left: 0.9, top: 0.8 },
      ],
      800,
      600,
    );
    expect(spread.map((g) => g.members.length)).toEqual([1, 1, 1]);
  });

  it('keeps allied and Soviet stacks separate, mixing only in a one-card viewport', () => {
    const anchors = Array.from({ length: 30 }, (_, i) => ({
      key: String(i),
      left: 0.5,
      top: 0.5,
      faction: i % 2 ? 'USSR' : 'USA',
    }));
    const sides = (members: string[]) =>
      new Set(members.map((key) => anchors[Number(key)]!.faction));
    const groups = layoutTaLabels(anchors, 800, 600);
    expect(groups.every((g) => sides(g.members).size === 1)).toBe(true);
    expect(groups.every((g) => g.height <= 138)).toBe(true);
    const tiny = layoutTaLabels(anchors, 180, 54);
    expect(tiny).toHaveLength(1);
    expect(sides(tiny[0]!.members).size).toBe(2);
  });

  it('prefers a clear position over covering an objective', () => {
    const anchors = [{ key: 'a', left: 0.5, top: 0.5 }];
    const first = layoutTaLabels(anchors, 800, 600)[0]!;
    const obstacle = {
      left: (first.left + first.width / 2) / 800,
      top: (first.top + first.height / 2) / 600,
      radius: 16,
    };
    const clear = layoutTaLabels(anchors, 800, 600, [obstacle])[0]!;
    expect(clear.top).not.toBe(first.top);
  });

  it('keeps unobstructed connectors short and separates cards with room', () => {
    const anchors = [
      { key: 'a', left: 0.25, top: 0.25 },
      { key: 'b', left: 0.75, top: 0.75 },
    ];
    const groups = layoutTaLabels(anchors, 800, 600);
    expect(groups).toHaveLength(2);
    groups.forEach((g, i) => {
      const x = anchors[i]!.left * 800,
        y = anchors[i]!.top * 600;
      expect(
        Math.hypot(
          Math.max(g.left - x, 0, x - g.left - g.width),
          Math.max(g.top - y, 0, y - g.top - g.height),
        ),
      ).toBeLessThanOrEqual(10);
    });
  });

  it('leaves a stack gap outside all member markers, including spread-out members', () => {
    const anchors = [
      { key: 'a', left: 0.4, top: 0.5 },
      { key: 'b', left: 0.55, top: 0.52 },
    ];
    const [group] = layoutTaLabels(anchors, 800, 600);
    expect(group!.members).toHaveLength(2);
    const minX = 320,
      maxX = 440,
      minY = 300,
      maxY = 312;
    expect(
      group!.left + group!.width <= minX - 32 ||
        group!.left >= maxX + 32 ||
        group!.top + group!.height <= minY - 32 ||
        group!.top >= maxY + 32,
    ).toBe(true);
  });

  it('places edge stacks inside the map without hiding their marker area when space exists', () => {
    for (const [left, top] of [
      [0.01, 0.01],
      [0.99, 0.99],
      [0.01, 0.99],
    ]) {
      const anchors = [
        { key: 'a', left: left!, top: top! },
        { key: 'b', left: left!, top: top! },
      ];
      const [g] = layoutTaLabels(anchors, 800, 600);
      const x = left! * 800,
        y = top! * 600;
      expect(
        Math.hypot(
          Math.max(g!.left - x, 0, x - g!.left - g!.width),
          Math.max(g!.top - y, 0, y - g!.top - g!.height),
        ),
      ).toBeGreaterThanOrEqual(32);
      expect(g!.left).toBeGreaterThanOrEqual(4);
      expect(g!.top).toBeGreaterThanOrEqual(4);
      expect(g!.left + g!.width).toBeLessThanOrEqual(796);
      expect(g!.top + g!.height).toBeLessThanOrEqual(596);
    }
  });

  it('separates drop and other TA stacks within both faction sides', () => {
    const anchors = Array.from({ length: 12 }, (_, i) => ({
      key: String(i),
      left: 0.5,
      top: 0.5,
      faction: i % 2 ? 'USSR' : 'USA',
      groupingCategory: i % 4 < 2 ? ('unit-drop' as const) : ('other' as const),
    }));
    const groups = layoutTaLabels(anchors, 800, 600);
    expect(groups).toHaveLength(4);
    for (const group of groups) {
      expect(new Set(group.members.map((key) => anchors[Number(key)]!.groupingCategory)).size).toBe(
        1,
      );
      expect(new Set(group.members.map((key) => anchors[Number(key)]!.faction)).size).toBe(1);
    }
    const tiny = layoutTaLabels(anchors, 180, 54);
    expect(tiny).toHaveLength(1);
    expect(tiny[0]!.members).toHaveLength(12);
  });

  it('prefers 24 pixels for a single drop and reduces clearance only when necessary', () => {
    for (const [height, expected] of [
      [160, 24],
      [140, 16],
      [124, 8],
    ]) {
      const [g] = layoutTaLabels(
        [{ key: 'a', left: 0.5, top: 0.5, groupingCategory: 'unit-drop' }],
        180,
        height!,
      );
      const x = 90,
        y = height! / 2;
      const length = Math.hypot(
        Math.max(g!.left - x, 0, x - g!.left - g!.width),
        Math.max(g!.top - y, 0, y - g!.top - g!.height),
      );
      expect(length).toBe(expected);
    }
    const [g] = layoutTaLabels([{ key: 'a', left: 0.5, top: 0.5 }], 800, 600);
    expect(300 - g!.top - g!.height).toBe(8);
  });

  it('keeps edge labels inside the viewport, including after resizing', () => {
    const anchors = [
      { key: 'a', left: 0, top: 0 },
      { key: 'b', left: 1, top: 1 },
    ];
    for (const [width, height] of [
      [800, 600],
      [180, 100],
    ]) {
      for (const group of layoutTaLabels(anchors, width!, height!)) {
        expect(group.left).toBeGreaterThanOrEqual(0);
        expect(group.top).toBeGreaterThanOrEqual(0);
        expect(group.left + group.width).toBeLessThanOrEqual(width!);
        expect(group.top + group.height).toBeLessThanOrEqual(height!);
      }
    }
  });

  it('separates sparse events and safely handles an empty or hidden map', () => {
    expect(layoutTaLabels([], 800, 600)).toEqual([]);
    expect(layoutTaLabels([{ key: 'a', left: 0.5, top: 0.5 }], 0, 0)).toEqual([]);
    const anchors = [
      { key: 'a', left: 0.1, top: 0.1 },
      { key: 'b', left: 0.9, top: 0.9 },
    ];
    expect(layoutTaLabels(anchors, 800, 600).map((g) => g.members.length)).toEqual([1, 1]);
  });
});
