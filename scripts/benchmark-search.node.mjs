// Manual CPU benchmark; input is a local JSON array of serialized ReplaySummary rows.
// No replay files are opened. The 16-player stress case is explicitly synthetic.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { performance } from 'node:perf_hooks';
import {
  filterReplaySearch,
  indexReplaySummaries,
  parseReplaySearch,
} from '../frontend/replaySearch.ts';

const input = process.argv[2];
if (!input) {
  throw new Error('Usage: node scripts/benchmark-search.node.mjs <summaries.json>');
}
const rows = JSON.parse(readFileSync(input, 'utf8'));
const start = performance.now();
const index = indexReplaySummaries(rows);
const preparationMs = performance.now() - start;
function measure(run) {
  for (let i = 0; i < 20; i++) {
    run();
  }
  const samples = [];
  let count;
  for (let i = 0; i < 100; i++) {
    const before = performance.now();
    count = run().length;
    samples.push(performance.now() - before);
  }
  samples.sort((a, b) => a - b);
  return { matches: count, medianMs: samples[50], p95Ms: samples[95] };
}
function legacy(query) {
  return rows.filter((row) => {
    const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
    if (!terms.length) {
      return true;
    }
    const values = [
      row.fileName,
      row.replayName,
      row.serverName,
      row.mapName,
      row.mapDisplayName,
      row.gameMode,
      row.serverModes,
      row.format,
      row.dateTime,
      row.recorder,
      row.playerNames,
      row.factions,
      row.parseError,
    ].map((v) => v?.toLocaleLowerCase() ?? '');
    return terms.every((term) => values.some((value) => value.includes(term)));
  });
}
const queries = ['generalx', 'generalx riviera', 'generalx riviera nato', 'ussr', 'no-such-player'];
const timings = queries.map((query) => {
  const run = () => filterReplaySearch(index, parseReplaySearch(query));
  // Faction queries intentionally narrow player matches compared with legacy search.
  if (!parseReplaySearch(query).faction.length) {
    assert.deepEqual(
      run().map((r) => r.path),
      legacy(query).map((r) => r.path),
    );
  }
  return { query, previous: measure(() => legacy(query)), prepared: measure(run) };
});
const stressed = indexReplaySummaries(
  rows.map((row) => ({
    ...row,
    searchPlayers: Array.from({ length: 16 }, (_, i) => ({
      name: `Test player ${i}`,
      faction: i < 8 ? 'NATO' : 'USSR',
    })),
  })),
);
const stressQueries = [
  'player:missing faction:nato',
  'player:"Test player 15" faction:ussr',
  'player:"Test player 15" map:riviera faction:ussr',
];
console.log(
  JSON.stringify(
    {
      rowCount: rows.length,
      preparationMs,
      timings,
      syntheticSixteenPlayersPerReplay: stressQueries.map((query) => ({
        query,
        ...measure(() => filterReplaySearch(stressed, parseReplaySearch(query))),
      })),
    },
    null,
    2,
  ),
);
