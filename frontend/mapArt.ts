/// Runtime map overview art, cached per internal map name.
///
/// The viewer ships no artwork. When the user points at their own game
/// installation the backend decodes `maps/<internal_name>/overviewmap.dds` and
/// caches it; this module fetches the unique images needed by visible rows in
/// small batches. Rows render their procedural tile immediately and upgrade in
/// place when real art arrives.

import { shallowReactive, shallowRef } from 'vue';

import { getMapArtBatch } from './api';
import type { MapArtAsset } from './types';

const MAX_BATCH_SIZE = 64;
const RETRY_DELAYS_MS = [50, 250] as const;

/// `undefined` means "not looked up yet"; `null` means "looked up, nothing
/// there", which is a normal outcome for custom maps and unconfigured installs.
const resolved = shallowReactive(new Map<string, MapArtAsset | null>());
const inFlight = new Set<string>();
const queued = new Set<string>();
const retryAttempts = new Map<string, number>();
const retryTimers = new Map<string, ReturnType<typeof setTimeout>>();
const available = shallowRef(false);
const cacheGeneration = shallowRef(0);
let batchScheduled = false;

/// Availability is reactive because tiles commonly mount before the backend's
/// background source validation completes. Changing it to true re-runs their
/// watch effects and starts requests for the already visible maps.
export function setMapArtAvailable(value: boolean): void {
  if (available.value === value) {
    return;
  }
  available.value = value;
  if (!value) {
    resetMapArt();
  }
}

export function mapArtAvailable(): boolean {
  return available.value;
}

export function mapArtFor(mapName: string): string | null {
  return resolved.get(mapName.trim())?.imageUrl ?? null;
}

export function mapAssetFor(mapName: string): MapArtAsset | null {
  return resolved.get(mapName.trim()) ?? null;
}

function scheduleBatch(): void {
  if (batchScheduled || !queued.size) {
    return;
  }
  batchScheduled = true;
  queueMicrotask(() => {
    batchScheduled = false;
    void flushBatch();
  });
}

function scheduleRetry(key: string, generation: number): void {
  const failedAttempts = (retryAttempts.get(key) ?? 0) + 1;
  retryAttempts.set(key, failedAttempts);
  const delay = RETRY_DELAYS_MS[failedAttempts - 1];
  if (delay === undefined) {
    // Leave the key unresolved so a later remount or source refresh can start
    // a fresh bounded retry series instead of caching a transient error as a
    // permanent "no art" result.
    retryAttempts.delete(key);
    return;
  }
  const timer = setTimeout(() => {
    if (generation !== cacheGeneration.value) {
      return;
    }
    retryTimers.delete(key);
    requestMapArt(key);
  }, delay);
  retryTimers.set(key, timer);
}

async function flushBatch(): Promise<void> {
  if (!available.value || !queued.size) {
    return;
  }
  const generation = cacheGeneration.value;
  const keys = [...queued].slice(0, MAX_BATCH_SIZE);
  for (const key of keys) {
    queued.delete(key);
    inFlight.add(key);
  }

  try {
    const images = await getMapArtBatch(keys);
    if (generation !== cacheGeneration.value) {
      return;
    }
    for (const key of keys) {
      resolved.set(
        key,
        Object.prototype.hasOwnProperty.call(images, key) ? (images[key] ?? null) : null,
      );
      retryAttempts.delete(key);
    }
  } catch {
    if (generation !== cacheGeneration.value || !available.value) {
      return;
    }
    for (const key of keys) {
      scheduleRetry(key, generation);
    }
  } finally {
    if (generation === cacheGeneration.value) {
      for (const key of keys) {
        inFlight.delete(key);
      }
      scheduleBatch();
    }
  }
}

/// Idempotent: repeated calls for the same map while queued, in flight, waiting
/// to retry, or already resolved do nothing. Reading the two refs is deliberate:
/// it makes component watch effects react to startup availability and resets.
export function requestMapArt(mapName: string): void {
  const enabled = available.value;
  void cacheGeneration.value;
  const key = mapName.trim();
  if (
    !enabled ||
    !key ||
    resolved.has(key) ||
    queued.has(key) ||
    inFlight.has(key) ||
    retryTimers.has(key)
  ) {
    return;
  }
  queued.add(key);
  scheduleBatch();
}

/// Drop frontend URLs and invalidate all queued, in-flight, and delayed work.
/// Old promises are generation-guarded and cannot repopulate a new source.
export function resetMapArt(): void {
  cacheGeneration.value += 1;
  resolved.clear();
  queued.clear();
  inFlight.clear();
  retryAttempts.clear();
  for (const timer of retryTimers.values()) {
    clearTimeout(timer);
  }
  retryTimers.clear();
}
