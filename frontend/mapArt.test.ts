import { effectScope, watchEffect } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

type Asset = { imageUrl: string; bounds: null; commandPointNames: Record<string, string> };
type Batch = Record<string, Asset | null>;
const asset = (imageUrl: string): Asset => ({ imageUrl, bounds: null, commandPointNames: {} });
const getMapArtBatch = vi.fn<(mapNames: string[]) => Promise<Batch>>();
vi.mock('./api', () => ({ getMapArtBatch: (names: string[]) => getMapArtBatch(names) }));

const { mapArtAvailable, mapArtFor, requestMapArt, resetMapArt, setMapArtAvailable } =
  await import('./mapArt');

describe('map art cache', () => {
  beforeEach(() => {
    getMapArtBatch.mockReset();
    setMapArtAvailable(false);
    resetMapArt();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('asks for nothing until art is available', () => {
    requestMapArt('russia3');
    expect(getMapArtBatch).not.toHaveBeenCalled();
    expect(mapArtFor('russia3')).toBeNull();
    expect(mapArtAvailable()).toBe(false);
  });

  it('loads already-mounted tiles when startup enables the cached art', async () => {
    getMapArtBatch.mockResolvedValue({ russia3: asset('data:image/png;base64,AAAA') });
    const scope = effectScope();
    scope.run(() => watchEffect(() => requestMapArt('russia3')));

    expect(getMapArtBatch).not.toHaveBeenCalled();
    setMapArtAvailable(true);
    await vi.waitFor(() => expect(mapArtFor('russia3')).toBe('data:image/png;base64,AAAA'));

    expect(getMapArtBatch).toHaveBeenCalledTimes(1);
    scope.stop();
  });

  it('fetches the unique visible maps in one batch', async () => {
    getMapArtBatch.mockImplementation(async (names) =>
      Object.fromEntries(names.map((name) => [name, asset(`data:image/png;base64,${name}`)])),
    );
    setMapArtAvailable(true);

    requestMapArt('usfarmland1');
    requestMapArt('usfarmland3');
    requestMapArt('usfarmland1');
    await vi.waitFor(() => expect(mapArtFor('usfarmland3')).not.toBeNull());

    expect(getMapArtBatch).toHaveBeenCalledTimes(1);
    expect(new Set(getMapArtBatch.mock.calls[0]?.[0])).toEqual(
      new Set(['usfarmland1', 'usfarmland3']),
    );
  });

  it('remembers a real cache miss and does not ask again', async () => {
    getMapArtBatch.mockResolvedValue({ some_custom_map: null });
    setMapArtAvailable(true);

    requestMapArt('some_custom_map');
    await vi.waitFor(() => expect(getMapArtBatch).toHaveBeenCalledTimes(1));
    requestMapArt('some_custom_map');

    expect(getMapArtBatch).toHaveBeenCalledTimes(1);
    expect(mapArtFor('some_custom_map')).toBeNull();
  });

  it('retries a transient backend failure instead of caching it as no art', async () => {
    getMapArtBatch
      .mockRejectedValueOnce(new Error('database is locked'))
      .mockResolvedValueOnce({ russia3: asset('data:image/png;base64,AAAA') });
    setMapArtAvailable(true);

    requestMapArt('russia3');
    await vi.waitFor(() => expect(mapArtFor('russia3')).toBe('data:image/png;base64,AAAA'), {
      timeout: 1_000,
    });

    expect(getMapArtBatch).toHaveBeenCalledTimes(2);
  });

  it('leaves repeated failures retryable after the bounded automatic attempts', async () => {
    getMapArtBatch.mockRejectedValue(new Error('database is locked'));
    setMapArtAvailable(true);
    requestMapArt('russia3');
    await vi.waitFor(() => expect(getMapArtBatch).toHaveBeenCalledTimes(3), { timeout: 1_000 });

    getMapArtBatch.mockResolvedValue({ russia3: asset('data:image/png;base64,AAAA') });
    requestMapArt('russia3');
    await vi.waitFor(() => expect(mapArtFor('russia3')).toBe('data:image/png;base64,AAAA'), {
      timeout: 1_000,
    });

    expect(getMapArtBatch).toHaveBeenCalledTimes(4);
  });

  it('ignores blank map names', () => {
    setMapArtAvailable(true);
    requestMapArt('');
    requestMapArt('   ');
    expect(getMapArtBatch).not.toHaveBeenCalled();
  });

  it('prevents an old in-flight response from repopulating a reset cache', async () => {
    let finish: ((batch: Batch) => void) | undefined;
    getMapArtBatch.mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    setMapArtAvailable(true);
    requestMapArt('russia3');
    await vi.waitFor(() => expect(getMapArtBatch).toHaveBeenCalledTimes(1));

    resetMapArt();
    finish?.({ russia3: asset('data:image/png;base64,STALE') });
    await Promise.resolve();
    await Promise.resolve();

    expect(mapArtFor('russia3')).toBeNull();
  });

  it('drops frontend URLs when the installation is removed', async () => {
    getMapArtBatch.mockResolvedValue({ russia3: asset('data:image/png;base64,AAAA') });
    setMapArtAvailable(true);
    requestMapArt('russia3');
    await vi.waitFor(() => expect(mapArtFor('russia3')).not.toBeNull());

    setMapArtAvailable(false);
    expect(mapArtFor('russia3')).toBeNull();

    setMapArtAvailable(true);
    requestMapArt('russia3');
    await vi.waitFor(() => expect(getMapArtBatch).toHaveBeenCalledTimes(2));
  });
});
