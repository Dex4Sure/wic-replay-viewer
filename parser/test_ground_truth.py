"""
Regression test: compare Rust parser output against saved ground truth.
Run after any parser changes to ensure known-good replays still parse correctly.
"""

import json
import os
import subprocess
import sys
from pathlib import Path

GROUND_TRUTH_FILE = Path(
    os.environ.get(
        "WIC_GROUND_TRUTH_FILE",
        Path(__file__).parent / "ground_truth.json",
    )
)
REPLAY_ROOT = Path(os.environ.get("WIC_REPLAY_ROOT", Path.home()))
RUST_PARSER = Path(
    os.environ.get(
        "WIC_RUST_PARSER",
        Path(__file__).parent.parent / "target" / "release" / "wic_replay_parser",
    )
)
REQUIRE_ALL = os.environ.get("WIC_REQUIRE_ALL_GROUND_TRUTH") == "1"


def find_replay(rel_path: str) -> Path:
    """Resolve a ground-truth path relative to the configured evidence root."""
    relative = Path(rel_path)
    candidates = [REPLAY_ROOT / relative]
    if relative.parts and relative.parts[0] == "replays":
        candidates.append(REPLAY_ROOT.joinpath(*relative.parts[1:]))
    for candidate in candidates:
        if candidate.exists():
            return candidate
    raise FileNotFoundError(f"Replay not found: {rel_path}")


def compare_players(expected, actual, replay_name):
    """Compare player lists, return list of differences."""
    diffs = []
    exp_by_id = {p["id"]: p for p in expected}
    act_by_id = {p["id"]: p for p in actual}

    all_ids = sorted(set(exp_by_id) | set(act_by_id))
    for pid in all_ids:
        if pid not in exp_by_id:
            diffs.append(f"  Player {pid}: NEW (not in ground truth)")
            continue
        if pid not in act_by_id:
            diffs.append(f"  Player {pid}: MISSING (was {exp_by_id[pid]['name']})")
            continue
        ep = exp_by_id[pid]
        ap = act_by_id[pid]
        for field in ["name", "team", "faction", "score", "role"]:
            if ep.get(field) != ap.get(field):
                diffs.append(
                    f"  Player {pid} ({ep['name']}): {field} changed: {ep.get(field)!r} -> {ap.get(field)!r}"
                )
    return diffs


def parse_replay(path: Path) -> dict:
    """Parse one replay with the compiled native Rust CLI."""
    process = subprocess.run(
        [str(RUST_PARSER), "--json", str(path)],
        capture_output=True,
        text=True,
        check=False,
    )
    if process.returncode != 0:
        detail = process.stderr.strip() or "Rust parser returned no error message"
        raise RuntimeError(detail)
    return json.loads(process.stdout)


def run_tests():
    if not RUST_PARSER.is_file():
        print(
            "ERROR: build the Rust parser first with "
            "`cargo build --release --locked --manifest-path rust_parser/Cargo.toml`"
        )
        return 1
    if not GROUND_TRUTH_FILE.is_file():
        print(
            "ERROR: ground truth is missing; set WIC_GROUND_TRUTH_FILE "
            "to an existing JSON expectations file"
        )
        return 1

    with open(GROUND_TRUTH_FILE) as f:
        ground_truth = json.load(f)

    passed = 0
    failed = 0
    errors = 0
    skipped = 0

    for replay_name, expected in sorted(ground_truth.items()):
        try:
            path = find_replay(replay_name)
        except FileNotFoundError as e:
            print(f"SKIP {replay_name}: {e}")
            skipped += 1
            continue

        try:
            data = parse_replay(path)
            actual = data
        except Exception as e:
            print(f"ERROR {replay_name}: {e}")
            errors += 1
            continue

        diffs = []

        # Compare game info
        game_info_fields = {
            "map_name": "mapName",
            "map_display_name": "mapDisplayName",
            "server_name": "serverName",
            "date_time": "dateTime",
            "game_mode": "gameMode",
        }
        for expected_field, actual_field in game_info_fields.items():
            ev = expected["game_info"].get(expected_field)
            av = actual["gameInfo"].get(actual_field)
            if ev != av:
                diffs.append(f"  game_info.{expected_field}: {ev!r} -> {av!r}")

        # Compare top-level fields
        top_level_fields = {
            "winner": "winner",
            "winner_domination_pct": "winnerDominationPct",
            "loser_domination_pct": "loserDominationPct",
            "incomplete": "incomplete",
            "recorder": "recorder",
        }
        for expected_field, actual_field in top_level_fields.items():
            ev = expected.get(expected_field)
            av = actual.get(actual_field)
            if ev != av:
                diffs.append(f"  {expected_field}: {ev!r} -> {av!r}")

        # Compare scores list
        if expected.get("player_scores") != actual.get("playerScores"):
            diffs.append("  player_scores changed")

        # Compare players
        diffs.extend(
            compare_players(expected["players"], actual["players"], replay_name)
        )

        if diffs:
            print(f"FAIL {replay_name}:")
            for d in diffs:
                print(d)
            failed += 1
        else:
            passed += 1

    executed = passed + failed + errors
    print(f"\n{'=' * 50}")
    print(
        f"Results: {passed} passed, {failed} failed, {errors} errors, "
        f"{skipped} skipped out of {executed + skipped} fixtures"
    )
    if executed == 0:
        print(
            "ERROR: no ground-truth fixtures ran; set WIC_REPLAY_ROOT to the "
            "private evidence root"
        )
        return 1
    if REQUIRE_ALL and skipped:
        print("ERROR: release validation requires every ground-truth fixture")
        return 1
    return failed + errors


if __name__ == "__main__":
    sys.exit(run_tests())
