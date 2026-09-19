import { describe, expect, it } from 'vitest';

import { hashMapName, mapInitials, mapTileArt } from './mapTile';

describe('mapInitials', () => {
  it('takes word initials from a multi-word map name', () => {
    expect(mapInitials('Cold Winter')).toBe('CW');
  });

  it('takes opening letters from a single-word map name', () => {
    expect(mapInitials('Vineyards')).toBe('VI');
  });

  it('caps a long name at three initials', () => {
    expect(mapInitials('Fields of Endless Green')).toBe('FOE');
  });

  it('falls back rather than returning an empty label', () => {
    expect(mapInitials('   ')).toBe('??');
    expect(mapInitials('---')).toBe('??');
  });
});

describe('mapTileArt', () => {
  it('is stable for the same map name', () => {
    expect(mapTileArt('Vineyards')).toEqual(mapTileArt('Vineyards'));
  });

  it('separates maps that share an opening letter', () => {
    const first = mapTileArt('Vineyards');
    const second = mapTileArt('Valley');
    expect(first.skyTop).not.toBe(second.skyTop);
    expect(first.ridgeFarPath).not.toBe(second.ridgeFarPath);
  });

  it('draws a neutral tile for an unresolved map', () => {
    const art = mapTileArt('');
    expect(art.initials).toBe('UN');
    expect(art.ridgeNearPath).toMatch(/^M0,80 L/);
  });

  it('gives each map its own gradient id', () => {
    expect(mapTileArt('Vineyards').gradientId).not.toBe(mapTileArt('Valley').gradientId);
  });

  it('hashes as an unsigned 32-bit value', () => {
    for (const name of ['Vineyards', 'Cold Winter', 'Alpine', '']) {
      const hash = hashMapName(name);
      expect(hash).toBeGreaterThanOrEqual(0);
      expect(hash).toBeLessThan(2 ** 32);
      expect(Number.isInteger(hash)).toBe(true);
    }
  });
});
