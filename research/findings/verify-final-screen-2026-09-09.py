"""Independent Python check of final-screen controls against native Flatpak output."""

import concurrent.futures
import dataclasses
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "parser"))
from wic_replay_parser import WICReplayParserV4  # noqa: E402


def verify(row):
    source = row["input"]
    path = Path(source["path"])
    assert hashlib.sha256(path.read_bytes()).hexdigest() == source["sha256"]
    players = WICReplayParserV4(str(path)).final_screen_players()

    def camel(key):
        first, *rest = key.split("_")
        return first + "".join(p.title() for p in rest)

    actual = (
        [{camel(k): v for k, v in dataclasses.asdict(p).items()} for p in players]
        if players is not None
        else None
    )
    assert actual == row["data"]["candidate"], str(path)
    return {
        "path": str(path),
        "sha256": source["sha256"],
        "rows": len(players) if players is not None else None,
    }


def main():
    rows = [
        json.loads(line)
        for line in (
            ROOT / "local/generated/final-screen-candidate-corpus-2026-09-09.jsonl"
        ).open()
    ]
    suffixes = [
        "demo58.wicdemo",
        "799__demo03.wicdemo",
        "333__demo17.wicdemo",
        "358__demo85.wicdemo",
        "730__demo95.wicdemo",
        "NoW vs Shiny 3.wicdemo",
        "demo263.wicdemo",
        "demo69.wicdemo",
        "demo66.wicdemo",
        "demo34.wicdemo",
        "demo30.wicdemo",
        "demo33.wicdemo",
        "demo186.wicdemo",
        "demo312.wicdemo",
    ]
    selected = [
        r
        for r in rows
        if "data" in r
        and (
            r["data"]["candidate"] is None
            or any(Path(p).name in suffixes for p in r["input"]["aliases"])
        )
    ]
    with concurrent.futures.ProcessPoolExecutor(max_workers=4) as pool:
        checked = list(pool.map(verify, selected))
    (ROOT / "local/generated/final-screen-python-check-2026-09-09.json").write_text(
        json.dumps(checked, indent=2) + "\n"
    )
    print(f"Python/native parity: {len(checked)} controls passed")


if __name__ == "__main__":
    main()
