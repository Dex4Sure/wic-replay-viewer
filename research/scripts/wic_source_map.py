#!/usr/bin/env python3
"""Attribute wic_ds.exe .text addresses to their source compilation units.

The shipped binaries retain assert strings of the form ``.\\EXG_Unit.cpp``.
Every ``push imm32`` of such a string marks an assert site, and every rel32
call destination marks a function entry. Combining the two yields a
deterministic function-range -> source-file map that needs no Ghidra
analysis pass and no code indexing.

Usage:
    python research/scripts/wic_source_map.py --binary local/binaries/game/wic_ds.exe \
        --json local/generated/wic_ds-source-map.json
"""

from __future__ import annotations

import argparse
import bisect
import json
import pathlib
import re
import struct

import pefile

SOURCE_RE = re.compile(rb"[\x20-\x7e]{2,200}\.(?:cpp|h)\x00")


class SourceMap:
    """Function ranges and their source compilation units."""

    def __init__(self, path: str) -> None:
        pe = pefile.PE(path, fast_load=True)
        self.base = pe.OPTIONAL_HEADER.ImageBase
        self.sections = [
            (
                s.Name.rstrip(b"\x00").decode("latin1"),
                self.base + s.VirtualAddress,
                s.get_data(),
            )
            for s in pe.sections
        ]
        _, self.tva, self.text = next(s for s in self.sections if s[0] == ".text")
        self.tend = self.tva + len(self.text)
        self.strings = self._source_strings()
        self.asserts = self._assert_sites()
        self.starts = sorted(self._call_targets())
        self.ranges = [
            (self.starts[i], self.starts[i + 1]) for i in range(len(self.starts) - 1)
        ] + [(self.starts[-1], self.tend)]
        self.by_function = self._attribute()

    def _source_strings(self) -> dict[int, str]:
        out: dict[int, str] = {}
        for _, va, data in self.sections:
            for m in SOURCE_RE.finditer(data):
                prev = data.rfind(b"\x00", 0, m.start())
                start = prev + 1 if prev != -1 else m.start()
                text = data[start : m.end() - 1].decode("latin1")
                if len(text) < 200:
                    out[va + start] = text
        return out

    def _assert_sites(self) -> dict[int, str]:
        out: dict[int, str] = {}
        for i in range(len(self.text) - 5):
            if self.text[i] != 0x68:
                continue
            imm = struct.unpack_from("<I", self.text, i + 1)[0]
            name = self.strings.get(imm)
            if name:
                out[self.tva + i] = name
        return out

    def _call_targets(self) -> set[int]:
        out: set[int] = set()
        for i in range(len(self.text) - 5):
            if self.text[i] != 0xE8:
                continue
            rel = struct.unpack_from("<i", self.text, i + 1)[0]
            dst = self.tva + i + 5 + rel
            if self.tva <= dst < self.tend:
                out.add(dst)
        return out

    def enclosing(self, va: int) -> tuple[int, int] | None:
        i = bisect.bisect_right(self.starts, va) - 1
        return self.ranges[i] if i >= 0 else None

    def _attribute(self) -> dict[int, set[str]]:
        out: dict[int, set[str]] = {}
        for va, name in self.asserts.items():
            rng = self.enclosing(va)
            if rng:
                out.setdefault(rng[0], set()).add(name)
        return out

    def files_for(self, va: int) -> list[str]:
        rng = self.enclosing(va)
        return sorted(self.by_function.get(rng[0], [])) if rng else []


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True)
    ap.add_argument("--json", required=True)
    ap.add_argument("--query", nargs="*", default=[], help="addresses to resolve")
    args = ap.parse_args()

    sm = SourceMap(args.binary)
    report = {
        "binary": args.binary,
        "sourceStrings": len(sm.strings),
        "assertSites": len(sm.asserts),
        "functionStarts": len(sm.starts),
        "attributedFunctions": len(sm.by_function),
        "functions": {
            hex(start): sorted(files) for start, files in sorted(sm.by_function.items())
        },
    }
    pathlib.Path(args.json).write_text(json.dumps(report, indent=2))
    print(
        f"{report['sourceStrings']} source strings, "
        f"{report['assertSites']} assert sites, "
        f"{report['functionStarts']} function starts, "
        f"{report['attributedFunctions']} attributed"
    )
    for q in args.query:
        va = int(q, 16)
        rng = sm.enclosing(va)
        print(f"  {q} -> {hex(rng[0])}..{hex(rng[1])} {sm.files_for(va)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
