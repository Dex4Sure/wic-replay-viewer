<script setup lang="ts">
import { computed, watchEffect } from 'vue';

import { factionTone } from '../format';
import { mapArtFor, requestMapArt } from '../mapArt';
import { TILE_HEIGHT, TILE_WIDTH, mapTileArt } from '../mapTile';

const props = defineProps<{
  /// The internal map name, which is the asset lookup key. This is the
  /// `<internal_name>` directory component inside the game archives, so it must
  /// not be routed through the display name.
  mapName: string;
  /// The human-readable name, used only for the procedural fallback's letters.
  displayName?: string;
  /// The side the recorder played on, drawn as an inner edge. Selection already
  /// owns red on the card itself, so the faction tint lives here instead of on
  /// the accent stripe.
  faction: string | null;
}>();

const art = computed(() => mapTileArt(props.displayName || props.mapName));
/// Real artwork when the user's installation supplied it, otherwise null and
/// the procedural tile below stands on its own.
const photo = computed(() => mapArtFor(props.mapName));
// Virtualized rows reuse this component as they scroll, so the request follows
// whichever map the row currently shows.
watchEffect(() => requestMapArt(props.mapName));

// Deliberately not `factionToneClass`: those chip classes carry their own
// background and border, which would paint over the tile artwork.
const tone = computed(() => {
  const side = factionTone(props.faction);
  return side && `map-tile-${side}`;
});
</script>

<template>
  <span class="map-tile" :class="tone" aria-hidden="true">
    <img v-if="photo" class="map-tile-photo" :src="photo" alt="" decoding="async" />
    <template v-else>
      <svg :viewBox="`0 0 ${TILE_WIDTH} ${TILE_HEIGHT}`" preserveAspectRatio="none">
        <defs>
          <linearGradient :id="art.gradientId" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" :stop-color="art.skyTop" />
            <stop offset="100%" :stop-color="art.skyBottom" />
          </linearGradient>
        </defs>
        <rect :width="TILE_WIDTH" :height="TILE_HEIGHT" :fill="`url(#${art.gradientId})`" />
        <path :d="art.ridgeFarPath" :fill="art.ridgeFar" />
        <path :d="art.ridgeNearPath" :fill="art.ridgeNear" />
      </svg>
      <span class="map-tile-initials">{{ art.initials }}</span>
    </template>
    <span class="map-tile-fade" />
  </span>
</template>
