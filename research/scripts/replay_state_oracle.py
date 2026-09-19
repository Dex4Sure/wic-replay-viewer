#!/usr/bin/env python3
"""Capture or compare the original client's UnitFrame replay inputs.

``capture`` attaches to a running, hash-verified 32-bit ``wic.exe`` process and
therefore requires an explicit command-line acknowledgement. ``compare`` is
offline and matches those raw runtime float bits against a research state export.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import signal
import struct
import threading
from typing import Any, Iterable

from replay_state_reconstruction import (
    decode_compact_orientation,
    decode_compact_position,
)

EXP_PLAYER_UNIT_FRAME_RVA = 0x005362E0
CAPTURE_SCHEMA_VERSION = 1


def _sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _float_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", value))[0]


def expected_parent_frames(document: dict[str, Any]) -> list[dict[str, Any]]:
    frames = []
    for unit in document["units"]:
        for frame in unit["frames"]:
            if frame["encoding"] == "compact":
                raw = frame["raw"]
                values = [decode_compact_position(value) for value in raw[1:4]]
                values.extend(decode_compact_orientation(value) for value in raw[4:8])
                bits = [_float_bits(value) for value in values]
            else:
                bits = frame["raw"]["parentBits"]
            frames.append(
                {
                    "offset": frame["offset"],
                    "unitId": unit["unitId"],
                    "stateBits": bits,
                }
            )
    frames.sort(key=lambda row: row["offset"])
    return frames


def _read_json_lines(path: pathlib.Path) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    header = None
    events = []
    with path.open() as handle:
        for line in handle:
            row = json.loads(line)
            if row.get("type") == "captureHeader":
                header = row
            elif row.get("type") == "unitFrame":
                events.append(row)
    if header is None:
        raise ValueError("capture has no header")
    return header, events


def compare_capture(
    export_path: pathlib.Path,
    capture_path: pathlib.Path,
    skip_export: int,
    skip_capture: int,
) -> dict[str, Any]:
    document = json.loads(export_path.read_text())
    if document.get("contract") != "researchReplayState":
        raise ValueError("input is not a replay-state research export")
    header, captured = _read_json_lines(capture_path)
    expected = expected_parent_frames(document)[skip_export:]
    captured = captured[skip_capture:]
    compared = min(len(expected), len(captured))
    mismatches = []
    for index, (wanted, observed) in enumerate(zip(expected, captured)):
        if (
            wanted["unitId"] != observed["unitId"]
            or wanted["stateBits"] != observed["stateBits"]
        ):
            if len(mismatches) < 100:
                mismatches.append(
                    {
                        "index": index,
                        "expected": wanted,
                        "captured": observed,
                    }
                )
    return {
        "schemaVersion": CAPTURE_SCHEMA_VERSION,
        "scope": "unitFrameRuntimeOracleComparison",
        "replaySha256": document["source"]["replaySha256"],
        "captureBinarySha256": header["binarySha256"],
        "expectedFrames": len(expected),
        "capturedFrames": len(captured),
        "comparedFrames": compared,
        "mismatchCount": sum(
            wanted["unitId"] != observed["unitId"]
            or wanted["stateBits"] != observed["stateBits"]
            for wanted, observed in zip(expected, captured)
        ),
        "lengthsMatch": len(expected) == len(captured),
        "firstMismatches": mismatches,
        "verdict": (
            "exactParentCheckpointMatch"
            if len(expected) == len(captured) and not mismatches
            else "notProven"
        ),
    }


def _capture_source(module_name: str, max_events: int) -> str:
    return f"""
const module = Process.getModuleByName({json.dumps(module_name)});
const target = module.base.add(ptr('{EXP_PLAYER_UNIT_FRAME_RVA:#x}'));
let sequence = 0;
send({{type: 'ready', moduleName: module.name, modulePath: module.path,
      moduleBase: module.base.toString(), moduleSize: module.size,
      target: target.toString()}});
Interceptor.attach(target, {{
  onEnter(args) {{
    if ({max_events} > 0 && sequence >= {max_events}) return;
    const stateBits = [];
    for (let index = 1; index <= 7; index++) stateBits.push(args[index].toUInt32());
    send({{type: 'unitFrame', sequence: sequence++,
          threadId: this.threadId, unitId: args[0].toUInt32() & 0xffff,
          stateBits: stateBits}});
  }}
}});
"""


def capture(
    pid: int,
    binary: pathlib.Path,
    expected_sha256: str,
    output: pathlib.Path,
    module_name: str,
    max_events: int,
) -> int:
    actual_sha256 = _sha256(binary)
    if actual_sha256.lower() != expected_sha256.lower():
        raise ValueError(
            f"binary hash mismatch: expected {expected_sha256}, found {actual_sha256}"
        )
    import frida  # Imported only after all non-invasive validation succeeds.

    stop = threading.Event()
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x") as handle:
        handle.write(
            json.dumps(
                {
                    "type": "captureHeader",
                    "schemaVersion": CAPTURE_SCHEMA_VERSION,
                    "binaryPath": str(binary),
                    "binarySha256": actual_sha256,
                    "moduleName": module_name,
                    "unitFrameRva": f"{EXP_PLAYER_UNIT_FRAME_RVA:#x}",
                }
            )
            + "\n"
        )
        handle.flush()
        session = frida.attach(pid)
        script = session.create_script(_capture_source(module_name, max_events))

        def on_message(message: dict[str, Any], _data: bytes | None) -> None:
            if message.get("type") == "send":
                row = message["payload"]
                handle.write(json.dumps(row, separators=(",", ":")) + "\n")
                handle.flush()
                if max_events > 0 and row.get("sequence", -1) + 1 >= max_events:
                    stop.set()
            else:
                handle.write(
                    json.dumps({"type": "fridaMessage", "message": message}) + "\n"
                )
                handle.flush()

        script.on("message", on_message)
        script.load()
        previous = signal.signal(signal.SIGINT, lambda *_args: stop.set())
        try:
            stop.wait()
        finally:
            signal.signal(signal.SIGINT, previous)
            script.unload()
            session.detach()
    return 0


def _write_json(path: pathlib.Path, document: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(document, indent=2) + "\n")


def main(argv: Iterable[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    capture_parser = subparsers.add_parser("capture")
    capture_parser.add_argument("--pid", required=True, type=int)
    capture_parser.add_argument("--binary", required=True, type=pathlib.Path)
    capture_parser.add_argument("--expected-sha256", required=True)
    capture_parser.add_argument("--output", required=True, type=pathlib.Path)
    capture_parser.add_argument("--module", default="wic.exe")
    capture_parser.add_argument("--max-events", type=int, default=0)
    capture_parser.add_argument(
        "--confirm-attach",
        action="store_true",
        help="required acknowledgement that this attaches to a running game",
    )

    compare_parser = subparsers.add_parser("compare")
    compare_parser.add_argument("--export", required=True, type=pathlib.Path)
    compare_parser.add_argument("--capture", required=True, type=pathlib.Path)
    compare_parser.add_argument("--json", required=True, type=pathlib.Path)
    compare_parser.add_argument("--skip-export", type=int, default=0)
    compare_parser.add_argument("--skip-capture", type=int, default=0)

    args = parser.parse_args(argv)
    if args.command == "capture":
        if not args.confirm_attach:
            parser.error("capture requires --confirm-attach")
        return capture(
            args.pid,
            args.binary,
            args.expected_sha256,
            args.output,
            args.module,
            args.max_events,
        )
    result = compare_capture(
        args.export, args.capture, args.skip_export, args.skip_capture
    )
    _write_json(args.json, result)
    print(
        f"compared {result['comparedFrames']} frames: "
        f"{result['mismatchCount']} mismatches ({result['verdict']})"
    )
    return 0 if result["verdict"] == "exactParentCheckpointMatch" else 1


if __name__ == "__main__":
    raise SystemExit(main())
