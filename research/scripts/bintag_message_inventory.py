#!/usr/bin/env python3
"""Enumerate every BinTag message `wic.exe` can serialize into a replay.

`FUN_009240a0(writer, name)` begins a message on an event writer: it emits the
`Event` envelope carrying the gameplay timestamp and the Adler-32 of `name`, then
the caller appends fields. Walking its callers therefore enumerates the writable
message set exhaustively, which is the systematic alternative to discovering
messages by scanning replays.

Call sites follow one shape, confirmed against Ghidra's disassembly::

    PUSH  <channel>
    CALL  0x00b80880        ; acquire the event writer for that channel
    ...
    PUSH  <name string>     ; char *name
    PUSH  <writer>          ; writer
    CALL  0x009240a0

The scan is a byte-exact search for `E8 rel32` whose target is the message-begin
function, so it does not depend on a linear disassembly staying aligned. For each
hit it walks back a bounded window for the nearest `PUSH imm32` whose immediate
resolves to a NUL-terminated ASCII string inside the image, and reports that name
with its BinTag hash.

This reads the binary directly and never opens the Ghidra project, so it is safe
to run alongside an MCP or headless session.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import struct
import zlib

MESSAGE_BEGIN_VA = 0x009240A0
ACQUIRE_WRITER_VA = 0x00B80880
# Field writers appended between a message's begin and end calls.
WRITE_SCALAR_VA = 0x00989E40
WRITE_VECTOR_VA = 0x00923DB0
MESSAGE_END_VA = 0x00923A70
# Bytes after a begin call searched for that message's field writes.
BODY_SEARCH_BYTES = 512
# Bytes before a call site searched for the name push. The confirmed shape needs
# 6; the window tolerates a different writer push without admitting unrelated code.
PUSH_SEARCH_BYTES = 32
# Bytes before a call site searched for the channel acquisition.
CHANNEL_SEARCH_BYTES = 96
NAME_PATTERN = re.compile(rb"[A-Za-z_][A-Za-z0-9_]{2,63}\x00")


class Image:
    """Flat virtual-address view of a PE."""

    def __init__(self, path: pathlib.Path):
        import pefile

        pe = pefile.PE(str(path), fast_load=True)
        self.base = pe.OPTIONAL_HEADER.ImageBase
        self.sections = []
        for section in pe.sections:
            name = section.Name.rstrip(b"\x00").decode("ascii", "replace")
            start = self.base + section.VirtualAddress
            data = section.get_data()
            self.sections.append((name, start, start + len(data), data))
        pe.close()

    def read(self, va: int, size: int) -> bytes | None:
        for _, start, end, data in self.sections:
            if start <= va and va + size <= end:
                offset = va - start
                return data[offset : offset + size]
        return None

    def text(self) -> tuple[int, bytes]:
        for name, start, _, data in self.sections:
            if name == ".text":
                return start, data
        raise RuntimeError("no .text section")

    def c_string(self, va: int, limit: int = 64) -> str | None:
        block = self.read(va, limit)
        if not block:
            return None
        match = NAME_PATTERN.match(block)
        if not match:
            return None
        return match.group()[:-1].decode("ascii")


def find_calls(image: Image, target_va: int) -> list[int]:
    """Every `E8 rel32` whose target is `target_va`, by virtual address."""
    start, data = image.text()
    hits = []
    position = data.find(b"\xe8")
    while position != -1:
        if position + 5 <= len(data):
            (relative,) = struct.unpack_from("<i", data, position + 1)
            call_va = start + position
            if (call_va + 5 + relative) & 0xFFFFFFFF == target_va:
                hits.append(call_va)
        position = data.find(b"\xe8", position + 1)
    return hits


def name_at_call(image: Image, call_va: int) -> tuple[str | None, int | None]:
    """The message name pushed immediately before `call_va`."""
    window_start = call_va - PUSH_SEARCH_BYTES
    window = image.read(window_start, PUSH_SEARCH_BYTES)
    if not window:
        return None, None
    # Nearest push-immediate to the call wins: arguments are pushed right to
    # left, so the name is the last string-valued immediate before the writer.
    for offset in range(len(window) - 5, -1, -1):
        if window[offset] != 0x68:
            continue
        (immediate,) = struct.unpack_from("<I", window, offset + 1)
        name = image.c_string(immediate)
        if name:
            return name, immediate
    return None, None


def channel_at_call(image: Image, call_va: int) -> int | None:
    """The channel pushed to the writer acquisition preceding `call_va`."""
    window_start = call_va - CHANNEL_SEARCH_BYTES
    window = image.read(window_start, CHANNEL_SEARCH_BYTES)
    if not window:
        return None
    for offset in range(len(window) - 5, -1, -1):
        if window[offset] != 0xE8:
            continue
        (relative,) = struct.unpack_from("<i", window, offset + 1)
        site = window_start + offset
        if (site + 5 + relative) & 0xFFFFFFFF != ACQUIRE_WRITER_VA:
            continue
        # PUSH imm8 (6a xx) or PUSH imm32 (68 xx xx xx xx) directly before it.
        if offset >= 2 and window[offset - 2] == 0x6A:
            return window[offset - 1]
        if offset >= 5 and window[offset - 5] == 0x68:
            return struct.unpack_from("<I", window, offset - 4)[0]
        return None
    return None


def call_target(image: Image, va: int) -> int | None:
    """Target of the `E8 rel32` at `va`, if there is one."""
    block = image.read(va, 5)
    if not block or block[0] != 0xE8:
        return None
    (relative,) = struct.unpack_from("<i", block, 1)
    return (va + 5 + relative) & 0xFFFFFFFF


def vector_name_at_call(image: Image, call_va: int) -> str | None:
    """Base name loaded into EAX for the vector writer at `call_va`."""
    window_start = call_va - PUSH_SEARCH_BYTES
    window = image.read(window_start, PUSH_SEARCH_BYTES)
    if not window:
        return None
    for offset in range(len(window) - 5, -1, -1):
        if window[offset] != 0xB8:  # MOV EAX, imm32
            continue
        (immediate,) = struct.unpack_from("<I", window, offset + 1)
        name = image.c_string(immediate)
        if name:
            return name
    return None


def fields_after_call(image: Image, call_va: int) -> list[str]:
    """Field names written between this message's begin and end calls.

    The two writers pass their name differently. The scalar writer is cdecl, so
    its `name` is the last immediate pushed and therefore the nearest one behind
    the call. The vector writer takes its base name in EAX (`MOV EAX, imm32`)
    and formats `%s.x` / `%s.y` / `%s.z` from it, so that register load is what
    has to be recovered instead.
    """
    body = image.read(call_va, BODY_SEARCH_BYTES)
    if not body:
        return []
    names: list[str] = []
    for offset in range(5, len(body) - 5):
        target = call_target(image, call_va + offset)
        if target is None:
            continue
        if target == MESSAGE_END_VA:
            break
        if target not in (WRITE_SCALAR_VA, WRITE_VECTOR_VA):
            continue
        if target == WRITE_SCALAR_VA:
            name, _ = name_at_call(image, call_va + offset)
        else:
            name = vector_name_at_call(image, call_va + offset)
            if name:
                name = f"{name}.x/.y/.z"
        if name:
            names.append(name)
    return names


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary")
    parser.add_argument("--json")
    args = parser.parse_args()

    image = Image(pathlib.Path(args.binary))
    calls = find_calls(image, MESSAGE_BEGIN_VA)

    messages: dict[str, dict] = {}
    unresolved = []
    for call_va in calls:
        name, string_va = name_at_call(image, call_va)
        if not name:
            unresolved.append(f"0x{call_va:08x}")
            continue
        entry = messages.setdefault(
            name,
            {
                "name": name,
                "hash": f"0x{zlib.adler32(name.encode()):08x}",
                "stringVa": f"0x{string_va:08x}",
                "callSites": [],
                "channels": [],
                "fields": [],
            },
        )
        entry["callSites"].append(f"0x{call_va:08x}")
        for field in fields_after_call(image, call_va):
            if field not in entry["fields"]:
                entry["fields"].append(field)
        channel = channel_at_call(image, call_va)
        if channel is not None and channel not in entry["channels"]:
            entry["channels"].append(channel)

    report = {
        "binary": args.binary,
        "messageBeginVa": f"0x{MESSAGE_BEGIN_VA:08x}",
        "callSites": len(calls),
        "distinctMessages": len(messages),
        "unresolvedCallSites": unresolved,
        "messages": sorted(messages.values(), key=lambda m: m["name"]),
    }
    if args.json:
        pathlib.Path(args.json).write_text(json.dumps(report, indent=1))

    print(f"call sites: {len(calls)}  distinct messages: {len(messages)}")
    if unresolved:
        print(f"unresolved call sites: {len(unresolved)} {unresolved}")
    print()
    for entry in report["messages"]:
        fields = ", ".join(entry["fields"]) or "(none)"
        print(f"{entry['name']:<34}{entry['hash']}  {fields}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
