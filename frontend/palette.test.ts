import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const styles = readFileSync(new URL('./styles.css', import.meta.url), 'utf8');
const replayMap = readFileSync(new URL('./components/ReplayMap.vue', import.meta.url), 'utf8');

function paletteToken(name: string): string {
  const match = styles.match(new RegExp(`--color-${name}:\\s*(#[0-9a-fA-F]{6});`));
  expect(match, `missing --color-${name}`).not.toBeNull();
  return match![1];
}

function relativeLuminance(hex: string): number {
  const channels = hex
    .slice(1)
    .match(/.{2}/g)!
    .map((channel) => Number.parseInt(channel, 16) / 255)
    .map((channel) => (channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4));
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function contrast(foreground: string, background: string): number {
  const lighter = Math.max(relativeLuminance(foreground), relativeLuminance(background));
  const darker = Math.min(relativeLuminance(foreground), relativeLuminance(background));
  return (lighter + 0.05) / (darker + 0.05);
}

describe('semantic palette', () => {
  it('keeps brand, interaction, status, and replay-data roles distinct', () => {
    expect(paletteToken('action-primary')).not.toBe(paletteToken('accent'));
    expect(paletteToken('accent')).not.toBe(paletteToken('focus'));
    expect(paletteToken('danger')).not.toBe(paletteToken('faction-soviet-text'));
    expect(paletteToken('border-interactive')).not.toBe(paletteToken('faction-allied-strong'));
  });

  it.each(['primary', 'secondary', 'muted', 'subtle'])(
    'keeps %s text readable on an elevated surface',
    (role) => {
      expect(
        contrast(paletteToken(`text-${role}`), paletteToken('surface-elevated')),
      ).toBeGreaterThanOrEqual(4.5);
    },
  );

  it.each(['accent', 'danger', 'warning', 'success', 'faction-allied-text', 'faction-soviet-text'])(
    'keeps the %s semantic foreground readable on an elevated surface',
    (role) => {
      expect(contrast(paletteToken(role), paletteToken('surface-elevated'))).toBeGreaterThanOrEqual(
        4.5,
      );
    },
  );

  it.each(['action-primary', 'action-primary-hover', 'action-primary-active'])(
    'keeps primary-button text readable in the %s state',
    (role) => {
      expect(contrast(paletteToken('text-primary'), paletteToken(role))).toBeGreaterThanOrEqual(
        4.5,
      );
    },
  );

  it('keeps non-text focus and interactive borders perceptible', () => {
    expect(contrast(paletteToken('focus'), paletteToken('surface-canvas'))).toBeGreaterThanOrEqual(
      3,
    );
    expect(
      contrast(paletteToken('border-interactive'), paletteToken('surface-deep')),
    ).toBeGreaterThanOrEqual(3);
  });

  it.each([
    '.button-primary:not(:disabled)',
    '.button-secondary:not(:disabled)',
    '.locations-toggle',
    '.location-remove:not(:disabled)',
    '.locations-add:not(:disabled)',
    '.search-field button',
    '.library-selection-actions button',
    '.replay-card:not(.replay-card-active):not(.replay-card-selected)',
    '.library-empty-inline button',
    '.replay-management-actions button:not(:disabled)',
    '.detail-tabs button:not(.detail-tab-active)',
    '.aid-map-marker',
    '.aid-event-row:not(.aid-event-row-selected)',
    '.replay-play-button',
    '.replay-speed-trigger',
    '.replay-speed-menu button:not(.replay-speed-button-active)',
    '.replay-event-row',
    '.support-summary button:not(.support-summary-active)',
  ])('keeps hover and pressed styling for %s', (selector) => {
    expect(styles).toContain(`${selector}:hover`);
    expect(styles).toContain(`${selector}:active`);
  });

  it('keeps keyboard focus and disabled controls globally explicit', () => {
    expect(styles).toContain('button:focus-visible');
    expect(styles).toContain('input:focus-visible');
    expect(styles).toContain('select:focus-visible');
    expect(styles).toContain('button:disabled');
    expect(styles).toContain('select:disabled');
  });

  it('uses blue card surfaces with focused red selection markers', () => {
    expect(styles).toMatch(
      /\.replay-card-selected,\s*\.replay-card-active\s*\{[^}]*border-color:\s*var\(--color-border-interactive\)/s,
    );
    expect(styles).toMatch(
      /\.replay-card-selected \.replay-selection-mark\s*\{[^}]*background:\s*var\(--color-action-primary\)/s,
    );
    expect(styles).toMatch(
      /\.replay-card-active \.replay-card-accent\s*\{[^}]*background:\s*var\(--color-massgate-red\)/s,
    );
  });

  it('uses a compact custom speed menu instead of WebKitGTK native-select focus', () => {
    expect(replayMap).not.toContain('<select');
    expect(replayMap).toContain('aria-haspopup="menu"');
    expect(replayMap).toContain('class="replay-speed-menu"');
    expect(replayMap).toContain(':aria-checked="speed === option"');
    expect(styles).toMatch(
      /\.replay-speed-menu \.replay-speed-button-active\s*\{[^}]*var\(--color-accent\)/s,
    );
  });
});
