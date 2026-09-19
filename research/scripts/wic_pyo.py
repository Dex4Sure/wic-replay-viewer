#!/usr/bin/env python3
"""Bounded reader and compact disassembler for WiC's Python 2.5 PYO files."""

from __future__ import annotations

import argparse
import dataclasses
import pathlib
import re
import struct
from collections.abc import Iterator

from wic_sdf import SdfArchive

PYTHON_25_MAGIC = b"\x6d\xf2\x0d\x0a"
MAX_DEPTH = 128
MAX_CONTAINER_ITEMS = 1_000_000
MAX_STRING_BYTES = 64 * 1024 * 1024
HAVE_ARGUMENT = 90


class PyoError(ValueError):
    """Raised when a PYO or marshal structure violates its bounds."""


@dataclasses.dataclass(frozen=True)
class CodeObject:
    argcount: int
    nlocals: int
    stacksize: int
    flags: int
    code: bytes
    consts: tuple[object, ...]
    names: tuple[object, ...]
    varnames: tuple[object, ...]
    freevars: tuple[object, ...]
    cellvars: tuple[object, ...]
    filename: object
    name: object
    firstlineno: int
    lnotab: bytes


@dataclasses.dataclass(frozen=True)
class Instruction:
    offset: int
    opcode: int
    opname: str
    argument: int | None
    resolved: object | None


_NULL = object()


class _Reader:
    def __init__(self, data: bytes) -> None:
        self.data = data
        self.offset = 0
        self.interned: list[bytes] = []

    def read(self, size: int) -> bytes:
        if size < 0 or self.offset + size > len(self.data):
            raise PyoError("marshal object extends past end of file")
        result = self.data[self.offset : self.offset + size]
        self.offset += size
        return result

    def byte(self) -> int:
        return self.read(1)[0]

    def i32(self) -> int:
        return struct.unpack("<i", self.read(4))[0]

    def u64(self) -> int:
        return struct.unpack("<Q", self.read(8))[0]

    def sized_bytes(self) -> bytes:
        size = self.i32()
        if not 0 <= size <= MAX_STRING_BYTES:
            raise PyoError(f"invalid marshal string length {size}")
        return self.read(size)

    def count(self) -> int:
        count = self.i32()
        if not 0 <= count <= MAX_CONTAINER_ITEMS:
            raise PyoError(f"invalid marshal item count {count}")
        return count

    def obj(self, depth: int = 0):
        if depth > MAX_DEPTH:
            raise PyoError("marshal nesting exceeds limit")
        type_code = chr(self.byte())
        if type_code == "0":
            return _NULL
        if type_code == "N":
            return None
        if type_code == "F":
            return False
        if type_code == "T":
            return True
        if type_code == "S":
            return StopIteration
        if type_code == ".":
            return Ellipsis
        if type_code == "i":
            return self.i32()
        if type_code == "I":
            value = self.u64()
            return value - (1 << 64) if value & (1 << 63) else value
        if type_code == "g":
            return struct.unpack("<d", self.read(8))[0]
        if type_code == "f":
            return float(self.read(self.byte()).decode("ascii"))
        if type_code == "y":
            return complex(
                struct.unpack("<d", self.read(8))[0],
                struct.unpack("<d", self.read(8))[0],
            )
        if type_code == "x":
            real = float(self.read(self.byte()).decode("ascii"))
            imag = float(self.read(self.byte()).decode("ascii"))
            return complex(real, imag)
        if type_code == "l":
            digit_count = self.i32()
            sign = -1 if digit_count < 0 else 1
            digit_count = abs(digit_count)
            if digit_count > MAX_CONTAINER_ITEMS:
                raise PyoError(f"invalid marshal long digit count {digit_count}")
            value = 0
            for index in range(digit_count):
                digit = struct.unpack("<H", self.read(2))[0]
                if digit >= 1 << 15:
                    raise PyoError("invalid marshal long digit")
                value |= digit << (15 * index)
            return sign * value
        if type_code in ("s", "u"):
            raw = self.sized_bytes()
            return raw if type_code == "s" else raw.decode("utf-8")
        if type_code == "t":
            raw = self.sized_bytes()
            self.interned.append(raw)
            return raw
        if type_code == "R":
            index = self.i32()
            if not 0 <= index < len(self.interned):
                raise PyoError(f"invalid marshal string reference {index}")
            return self.interned[index]
        if type_code in ("(", "[", "<", ">"):
            items = [self.obj(depth + 1) for _ in range(self.count())]
            if type_code == "(":
                return tuple(items)
            if type_code == "[":
                return items
            return frozenset(items) if type_code == ">" else set(items)
        if type_code == "{":
            result = {}
            while True:
                key = self.obj(depth + 1)
                if key is _NULL:
                    return result
                value = self.obj(depth + 1)
                if value is _NULL:
                    raise PyoError("marshal dictionary has a value-less key")
                result[key] = value
        if type_code == "c":
            argcount = self.i32()
            nlocals = self.i32()
            stacksize = self.i32()
            flags = self.i32()
            code = self.obj(depth + 1)
            consts = self.obj(depth + 1)
            names = self.obj(depth + 1)
            varnames = self.obj(depth + 1)
            freevars = self.obj(depth + 1)
            cellvars = self.obj(depth + 1)
            filename = self.obj(depth + 1)
            name = self.obj(depth + 1)
            firstlineno = self.i32()
            lnotab = self.obj(depth + 1)
            if not isinstance(code, bytes) or not isinstance(lnotab, bytes):
                raise PyoError("code and line table must be byte strings")
            sequences = (consts, names, varnames, freevars, cellvars)
            if not all(isinstance(value, tuple) for value in sequences):
                raise PyoError("code-object tables must be tuples")
            return CodeObject(
                argcount,
                nlocals,
                stacksize,
                flags,
                code,
                consts,
                names,
                varnames,
                freevars,
                cellvars,
                filename,
                name,
                firstlineno,
                lnotab,
            )
        raise PyoError(f"unsupported marshal type 0x{ord(type_code):02x}")


def load_pyo(data: bytes) -> CodeObject:
    """Decode one complete Python 2.5 PYO payload."""
    if len(data) < 9 or data[:4] != PYTHON_25_MAGIC:
        raise PyoError("not a WiC Python 2.5 PYO")
    reader = _Reader(data[8:])
    root = reader.obj()
    if not isinstance(root, CodeObject):
        raise PyoError("PYO root is not a code object")
    if reader.offset != len(reader.data):
        raise PyoError(f"{len(reader.data) - reader.offset} trailing PYO bytes")
    return root


def walk_code(root: CodeObject) -> Iterator[CodeObject]:
    yield root
    for value in root.consts:
        if isinstance(value, CodeObject):
            yield from walk_code(value)


OPNAMES = {
    0: "STOP_CODE",
    1: "POP_TOP",
    2: "ROT_TWO",
    3: "ROT_THREE",
    4: "DUP_TOP",
    5: "ROT_FOUR",
    9: "NOP",
    10: "UNARY_POSITIVE",
    11: "UNARY_NEGATIVE",
    12: "UNARY_NOT",
    13: "UNARY_CONVERT",
    15: "UNARY_INVERT",
    19: "BINARY_POWER",
    20: "BINARY_MULTIPLY",
    21: "BINARY_DIVIDE",
    22: "BINARY_MODULO",
    23: "BINARY_ADD",
    24: "BINARY_SUBTRACT",
    25: "BINARY_SUBSCR",
    26: "BINARY_FLOOR_DIVIDE",
    27: "BINARY_TRUE_DIVIDE",
    54: "STORE_MAP",
    55: "INPLACE_ADD",
    56: "INPLACE_SUBTRACT",
    57: "INPLACE_MULTIPLY",
    58: "INPLACE_DIVIDE",
    59: "INPLACE_MODULO",
    62: "BINARY_LSHIFT",
    63: "BINARY_RSHIFT",
    64: "BINARY_AND",
    65: "BINARY_XOR",
    66: "BINARY_OR",
    67: "INPLACE_POWER",
    68: "GET_ITER",
    70: "PRINT_EXPR",
    71: "PRINT_ITEM",
    72: "PRINT_NEWLINE",
    73: "PRINT_ITEM_TO",
    74: "PRINT_NEWLINE_TO",
    75: "INPLACE_LSHIFT",
    76: "INPLACE_RSHIFT",
    77: "INPLACE_AND",
    78: "INPLACE_XOR",
    79: "INPLACE_OR",
    80: "BREAK_LOOP",
    82: "LOAD_LOCALS",
    83: "RETURN_VALUE",
    84: "IMPORT_STAR",
    85: "EXEC_STMT",
    86: "YIELD_VALUE",
    87: "POP_BLOCK",
    88: "END_FINALLY",
    89: "BUILD_CLASS",
    90: "STORE_NAME",
    91: "DELETE_NAME",
    92: "UNPACK_SEQUENCE",
    93: "FOR_ITER",
    94: "LIST_APPEND",
    95: "STORE_ATTR",
    96: "DELETE_ATTR",
    97: "STORE_GLOBAL",
    98: "DELETE_GLOBAL",
    99: "DUP_TOPX",
    100: "LOAD_CONST",
    101: "LOAD_NAME",
    102: "BUILD_TUPLE",
    103: "BUILD_LIST",
    104: "BUILD_MAP",
    105: "LOAD_ATTR",
    106: "COMPARE_OP",
    107: "IMPORT_NAME",
    108: "IMPORT_FROM",
    110: "JUMP_FORWARD",
    111: "JUMP_IF_FALSE",
    112: "JUMP_IF_TRUE",
    113: "JUMP_ABSOLUTE",
    116: "LOAD_GLOBAL",
    119: "CONTINUE_LOOP",
    120: "SETUP_LOOP",
    121: "SETUP_EXCEPT",
    122: "SETUP_FINALLY",
    124: "LOAD_FAST",
    125: "STORE_FAST",
    126: "DELETE_FAST",
    130: "RAISE_VARARGS",
    131: "CALL_FUNCTION",
    132: "MAKE_FUNCTION",
    133: "BUILD_SLICE",
    134: "MAKE_CLOSURE",
    135: "LOAD_CLOSURE",
    136: "LOAD_DEREF",
    137: "STORE_DEREF",
    140: "CALL_FUNCTION_VAR",
    141: "CALL_FUNCTION_KW",
    142: "CALL_FUNCTION_VAR_KW",
    143: "EXTENDED_ARG",
}

NAME_ARGUMENT_OPS = {90, 91, 95, 96, 97, 98, 101, 105, 107, 108, 116}
LOCAL_ARGUMENT_OPS = {124, 125, 126}
DEREF_ARGUMENT_OPS = {135, 136, 137}
COMPARES = (
    "<",
    "<=",
    "==",
    "!=",
    ">",
    ">=",
    "in",
    "not in",
    "is",
    "is not",
    "exception match",
    "BAD",
)


def instructions(code: CodeObject) -> list[Instruction]:
    """Decode Python 2.5 word arguments without interpreting control flow."""
    result = []
    offset = 0
    extended = 0
    while offset < len(code.code):
        start = offset
        opcode = code.code[offset]
        offset += 1
        argument = None
        if opcode >= HAVE_ARGUMENT:
            if offset + 2 > len(code.code):
                raise PyoError("truncated bytecode argument")
            argument = code.code[offset] | (code.code[offset + 1] << 8) | extended
            offset += 2
            extended = argument << 16 if opcode == 143 else 0
        resolved = None
        if argument is not None:
            table: tuple[object, ...] | None = None
            if opcode == 100:
                table = code.consts
            elif opcode in NAME_ARGUMENT_OPS:
                table = code.names
            elif opcode in LOCAL_ARGUMENT_OPS:
                table = code.varnames
            elif opcode in DEREF_ARGUMENT_OPS:
                table = code.cellvars + code.freevars
            elif opcode == 106:
                table = COMPARES
            if table is not None and 0 <= argument < len(table):
                resolved = table[argument]
        result.append(
            Instruction(
                start,
                opcode,
                OPNAMES.get(opcode, f"OP_{opcode}"),
                argument,
                resolved,
            )
        )
    return result


def display(value: object) -> str:
    if isinstance(value, bytes):
        return value.decode("latin1", "replace")
    if isinstance(value, CodeObject):
        return f"<code {display(value.name)}>"
    return repr(value)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=pathlib.Path)
    parser.add_argument("--contains", required=True, help="name to locate")
    parser.add_argument("--context", type=int, default=8)
    parser.add_argument("--path", help="entry-path regular expression")
    args = parser.parse_args()

    archive = SdfArchive.open(args.archive)
    needle = args.contains.encode("latin1")
    path_pattern = re.compile(args.path) if args.path else None
    found = False
    for entry in archive.entries:
        if not entry.path.endswith(".pyo"):
            continue
        if path_pattern is not None and path_pattern.search(entry.path) is None:
            continue
        data = archive.read_entry(entry)
        if needle not in data:
            continue
        root = load_pyo(data)
        for code in walk_code(root):
            decoded = instructions(code)
            matches = [
                index
                for index, instruction in enumerate(decoded)
                if instruction.resolved == needle
            ]
            for index in matches:
                found = True
                print(f"{entry.path}:{display(code.name)}:{code.firstlineno}")
                start = max(index - args.context, 0)
                end = min(index + args.context + 1, len(decoded))
                for instruction in decoded[start:end]:
                    argument = (
                        ""
                        if instruction.argument is None
                        else str(instruction.argument)
                    )
                    resolved = (
                        ""
                        if instruction.resolved is None
                        else display(instruction.resolved)
                    )
                    marker = ">" if instruction.offset == decoded[index].offset else " "
                    print(
                        f"{marker} {instruction.offset:04x} {instruction.opname:<22}"
                        f" {argument:<6} {resolved}"
                    )
    return 0 if found else 1


if __name__ == "__main__":
    raise SystemExit(main())
