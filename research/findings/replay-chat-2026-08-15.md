# Replay chat timeline evidence — 2026-08-15

> Historical schema note: this report validated the schema-v4 chat release.
> Schema v5 subsequently added chat-level sender names, and schema v6 added the
> canonical `timeline.participants` directory used to resolve player IDs across
> every timeline event.

## Scope and source identities

This finding documents chat that the recording player's client received. Source
replays and binaries were read only.

| Input | SHA-256 | Purpose |
|---|---|---|
| `binaries/game/wic.exe` | `41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc` | Message and field names |
| `replays/main/4600.wicdemo` | `7c9e8344477afe232eda016a9948d1121489e77bee8ec94f08a871687e2e1f42` | All/team and post-match chat |
| `replays/main/old/demo44.wicdemo` | `b378fd667d51599c682425bde43a47f733cecbf8ee92d36b9ea9d632d9707cef` | Bot response classification |
| `replays/main/old/demo55.wicdemo` | `1401c469887f5b54b2b7a5359cf081a88bc5cc199bee66dd9472f77e27f80b49` | Bot response classification |

## Record shapes

The shipped binary identifies two replay messages:

```text
PlayerReceiveChat         0x3c1806b1
PlayerReceiveChatPrivate  0x7620098c
```

`PlayerReceiveChat` contains three length-prefixed BinTag fields:

```text
aPlayer    flag 1  u32 sender slot
aMessage   flag 5  null-terminated UTF-16LE
aTeamChat  flag 3  bool: 0=all, 1=team
```

`PlayerReceiveChatPrivate` contains `aPlayer` and `aMessage` only, but its name does
not establish human private chat. The only two corpus records are allied AI command
acknowledgements:

```text
demo44 slot 12 Computer: Obedient Infan  "I will attack and hold area."
demo55 slot 11 Computer: Aggressive Air   "Ok - hang in there."
```

Schema version 4 intentionally excludes this bot-response message type. The normal
`PlayerReceiveChat` name is receive-side, so its correct coverage claim is **visible
to the recorder**, not complete server chat.

Each field repeats its total byte size at offsets `+4` and `+9`. Schema version 4
requires those sizes to agree, bounds the field to its containing event envelope,
caps UTF-16 payloads at 2,050 bytes, requires a terminal null, and rejects invalid
UTF-16 rather than consuming adjacent records.

## Timeline boundary

Chat before the first countdown sample is returned separately with
`secondsBeforeMatch`; chat after the last structurally valid `TeamWins` envelope is
returned with `secondsAfterMatch`. Both offsets use the envelopes' native float
timestamps. This preserves lobby and post-game conversation without extending the
gameplay scrub range. In-match chat uses the same countdown-interpolated
`timeSeconds` as every other timeline event.

For `4600.wicdemo`, schema version 4 extracts 1 pre-match, 10 in-match, and 5
post-match messages. Examples include:

```text
pre=41.9   slot 0   all   "newbie"
t=440.319  slot 15  team  "10 ta pls?"
t=526.583  slot 4   team  "nice"
t=903.624  slot 4   team  "8 points for nuke"
post=4.275 slot 15  all   "lol 4600"
```

## Full-corpus validation

The release Rust parser's single-process corpus mode scanned all three linked roots,
deduplicated them by canonical path, and produced:

| Measure | Result |
|---|---:|
| Accepted replays | 2,865 |
| Established rejects | 15 |
| Schema version 4 | 2,865 |
| Pre-match chat | 35,801 |
| In-match chat | 23,597 |
| Post-match chat | 5,488 |
| All-chat | 37,384 |
| Team-chat | 27,502 |
| Excluded bot responses | 2 |
| Invalid sender slots | 0 |
| In-match messages past duration | 0 |
| Recorder TA summary mismatches | 0 |

Mode coverage remained explicit: 2,784 Domination, 43 Assault, 12 Tug of War, and
26 unknown/custom-mode replays parsed successfully. Chat was present in all modes.

## Consumer semantics

- In schema v6, resolve every timeline player ID through `timeline.participants`.
  The schema-v5 chat-level `playerName` remains as a compatibility mirror; it can
  cover chat-only slots absent from `ReplayData.players`.
- Treat team-chat coverage as recorder-point-of-view data.
- Do not expose `PlayerReceiveChatPrivate` as personal chat; current evidence
  identifies it as bot-response traffic.
- Treat message text as untrusted user content. JSON serialization preserves it;
  HTML escaping, filtering, and moderation belong to the presentation layer.
- Keep post-match chat outside the gameplay scrub range.
