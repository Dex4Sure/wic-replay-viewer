#!/usr/bin/env python3
"""Test the hypothesis that the unlabelled server-mode bucket means Ranked.

The parser leaves `serverClassification.ranked` null and the viewer shows no
label when no positive mode signal is present. This audit measures what that
all-clear bucket actually contains: which servers produce it, whether any server
straddles it and a labelled bucket, how the matchups are shaped, and whether the
round length is the default one a ranked server is forced to use.

It reports only replay facts. A replay is never called Ranked here: the demo
header does not serialize the dedicated server's RankedFlag, so a ranked and an
unranked public game share the same observed state.

    research/scripts/ranked_bucket_audit.py local/replays/main local/replays/settings \\
      --json local/generated/ranked-bucket-audit.json
"""

from __future__ import annotations

import argparse
import collections
import concurrent.futures
import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys

MANIFEST = "parser/rust_parser/Cargo.toml"
DEFAULT_BINARY = "target/release/wic_replay_parser"
MARKUP = re.compile(r"<[^>]*>")
YEAR = re.compile(r"(\d{4})-")
RANKED_NAME = re.compile(r"\branked\b", re.IGNORECASE)
UNRANKED_NAME = re.compile(r"\bunranked\b", re.IGNORECASE)


def plain_server_name(name: str | None) -> str:
    """Strip the in-game colour and italics markup from a server name."""
    return MARKUP.sub("", name or "").strip()


def year_of(date_time: str | None) -> int | None:
    match = YEAR.search(date_time or "")
    return int(match.group(1)) if match else None


def bucket_of(classification: dict) -> str:
    """Mirror the viewer's label precedence, collapsed to one bucket name."""
    if classification.get("fewPlayerMode"):
        return "fewPlayerMode"
    if classification.get("clanMatch"):
        return "clanMatch"
    if classification.get("tournamentMatch"):
        return "tournamentMatch"
    if classification.get("hasBots"):
        return "bots"
    if classification.get("matchMode"):
        return "matchMode"
    return "unlabelled"


def audit_replay(binary: str, path: pathlib.Path) -> dict:
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    proc = subprocess.run(
        [binary, "--json", str(path)],
        capture_output=True,
        text=True,
    )
    try:
        parsed = json.loads(proc.stdout)
    except json.JSONDecodeError:
        detail = proc.stderr.strip() or "parser returned no JSON"
        return {"path": str(path), "sha256": digest, "error": detail}
    if isinstance(parsed, list):
        parsed = parsed[0] if parsed else {}

    game_info = parsed.get("gameInfo") or {}
    timing = parsed.get("timing") or {}
    players = parsed.get("players") or []
    teams: collections.Counter[int] = collections.Counter()
    for player in players:
        team = player.get("team")
        if team:
            teams[team] += 1

    return {
        "path": str(path),
        "sha256": digest,
        "server": plain_server_name(game_info.get("serverName")),
        "year": year_of(game_info.get("dateTime")),
        "mode": game_info.get("gameMode"),
        "map": game_info.get("mapDisplayName"),
        "bucket": bucket_of(parsed.get("serverClassification") or {}),
        "players": len(players),
        "matchup": "vs".join(str(count) for count in sorted(teams.values()))
        if len(teams) == 2
        else None,
        "roundLengthSeconds": round(timing["roundLengthSeconds"])
        if timing.get("roundLengthExact") and timing.get("roundLengthSeconds")
        else None,
    }


def discover(inputs: list[str]) -> list[pathlib.Path]:
    paths: set[pathlib.Path] = set()
    for item in inputs:
        path = pathlib.Path(item)
        if path.is_file() and path.suffix.lower() == ".wicdemo":
            paths.add(path)
        elif path.is_dir():
            for root, _directories, filenames in os.walk(path, followlinks=True):
                paths.update(
                    pathlib.Path(root, filename)
                    for filename in filenames
                    if pathlib.Path(filename).suffix.lower() == ".wicdemo"
                )
    return sorted(paths, key=lambda path: str(path).casefold())


def deduplicate(results: list[dict]) -> list[dict]:
    """Keep one row per replay: the corpus links the same file under many paths."""
    unique: dict[str, dict] = {}
    for result in results:
        unique.setdefault(result["sha256"], result)
    return list(unique.values())


def build_report(results: list[dict], modern_from: int, paths: int) -> dict:
    parsed = [item for item in results if "error" not in item and item["players"]]
    empty = [item for item in results if "error" not in item and not item["players"]]
    unlabelled = [item for item in parsed if item["bucket"] == "unlabelled"]

    servers: dict[str, collections.Counter[str]] = collections.defaultdict(
        collections.Counter
    )
    for item in parsed:
        servers[item["server"]][item["bucket"]] += 1
    straddling = {
        server: dict(counts)
        for server, counts in servers.items()
        if "unlabelled" in counts and len(counts) > 1
    }

    def era_split(items: list[dict]) -> dict:
        modern = [
            item
            for item in items
            if item["year"] is not None and item["year"] >= modern_from
        ]
        legacy = [
            item
            for item in items
            if item["year"] is not None and item["year"] < modern_from
        ]
        return {
            "modern": {
                "replays": len(modern),
                "serverNameSaysRanked": sum(
                    bool(RANKED_NAME.search(item["server"])) for item in modern
                ),
            },
            "legacy": {
                "replays": len(legacy),
                "serverNameSaysRanked": sum(
                    bool(RANKED_NAME.search(item["server"])) for item in legacy
                ),
            },
        }

    return {
        "method": {
            "buckets": "viewer label precedence: FPM, clan, tournament, bots, match mode, else unlabelled",
            "ranked": "never asserted; RankedFlag is not serialized into the demo header",
            "serverName": "reported as observed evidence, not as a classification input",
        },
        "summary": {
            "paths": paths,
            "uniqueReplays": len(parsed) + len(empty),
            "emptyReplays": len(empty),
            "errors": sum("error" in item for item in results),
            "buckets": dict(
                collections.Counter(item["bucket"] for item in parsed).most_common()
            ),
            "unlabelledEraSplit": era_split(unlabelled),
            "unlabelledServerNameSaysUnranked": sum(
                bool(UNRANKED_NAME.search(item["server"])) for item in unlabelled
            ),
            "distinctServers": len(servers),
            "serversInUnlabelledAndLabelledBuckets": straddling,
            "unlabelledMatchups": dict(
                collections.Counter(
                    item["matchup"] for item in unlabelled if item["matchup"]
                ).most_common(10)
            ),
            "roundLengthByBucketAndMode": {
                f"{bucket}/{mode}": dict(counts.most_common())
                for (bucket, mode), counts in sorted(
                    _round_lengths(parsed).items(),
                )
            },
        },
        "replays": results,
    }


def _round_lengths(parsed: list[dict]) -> dict[tuple[str, str], collections.Counter]:
    lengths: dict[tuple[str, str], collections.Counter] = collections.defaultdict(
        collections.Counter
    )
    for item in parsed:
        if item["roundLengthSeconds"] is not None:
            lengths[(item["bucket"], item["mode"] or "Unknown")][
                item["roundLengthSeconds"]
            ] += 1
    return lengths


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inputs", nargs="+", help="Replay files or corpus directories")
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--binary", default=DEFAULT_BINARY)
    parser.add_argument(
        "--modern-from",
        type=int,
        default=2023,
        help="First year of the revived-Massgate era (default: 2023)",
    )
    parser.add_argument("--json", dest="json_path", help="Write the full JSON report")
    args = parser.parse_args()

    if not pathlib.Path(args.binary).is_file():
        print(
            f"missing parser binary: {args.binary}\n"
            f"build it first: cargo build --release --manifest-path {MANIFEST}",
            file=sys.stderr,
        )
        return 2

    paths = discover(args.inputs)
    if not paths:
        print("no .wicdemo files found", file=sys.stderr)
        return 2

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        results = list(pool.map(lambda path: audit_replay(args.binary, path), paths))

    report = build_report(deduplicate(results), args.modern_from, len(paths))
    if args.json_path:
        pathlib.Path(args.json_path).write_text(
            json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
    print(json.dumps(report["summary"], indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
