import { describe, expect, it } from 'vitest';

import { exportDefaultPath, parentDirectory } from './exportLocation';

describe('remembered replay export locations', () => {
  it('keeps the source file name in a remembered Linux or macOS directory', () => {
    expect(exportDefaultPath('/replays/match.wicdemo', '/exports/archive')).toBe(
      '/exports/archive/match.wicdemo',
    );
  });

  it('keeps native separators in a remembered Windows directory', () => {
    expect(exportDefaultPath('C:\\Replays\\match.wicdemo', 'D:\\Exports')).toBe(
      'D:\\Exports\\match.wicdemo',
    );
  });

  it('derives export directories without losing filesystem roots', () => {
    expect(parentDirectory('/match.wicdemo')).toBe('/');
    expect(parentDirectory('/exports/archive/match.wicdemo')).toBe('/exports/archive');
    expect(parentDirectory('C:\\match.wicdemo')).toBe('C:\\');
    expect(parentDirectory('D:\\Exports\\match.wicdemo')).toBe('D:\\Exports');
  });
});
