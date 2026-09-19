#!/usr/bin/env python3
"""Launch pyghidra-mcp without eagerly decompiling every function at startup.

The upstream 0.2.3 server automatically schedules a full semantic index for
small projects. WiC has thousands of functions per binary, so that work should
be requested deliberately rather than blocking every short-lived MCP session.
All normal MCP tools remain registered; only automatic startup indexing is
suppressed.
"""

from __future__ import annotations

import functools
import json
import logging
from collections.abc import Callable
from typing import Any

from mcp_stdio import install_posix_stdio_transport
from pyghidra_mcp import mcp_tools
from pyghidra_mcp.context import PyGhidraContext
from pyghidra_mcp.indexing_mixin import IndexingMixin


install_posix_stdio_transport()


def skip_startup_indexing(
    self: IndexingMixin, *, max_binaries: int | None = 10
) -> None:
    del self, max_binaries
    logging.getLogger("pyghidra_mcp.indexing_mixin").info(
        "Skipping eager semantic indexing; use MCP search tools to index deliberately."
    )


IndexingMixin.schedule_startup_indexing = skip_startup_indexing


# pyghidra-mcp 0.2.3 initializes every program loaded from an existing project
# with ghidra_analysis_complete=False. The normal no-analysis startup path never
# corrects that flag, so almost every MCP tool rejects programs that Ghidra has
# already analyzed. Derive the state from Ghidra's persisted analyzed marker.
_original_init_program_info = PyGhidraContext._init_program_info


def init_program_info_with_analysis_state(self: PyGhidraContext, program: Any) -> Any:
    from ghidra.program.util import GhidraProgramUtilities

    program_info = _original_init_program_info(self, program)
    program_info.ghidra_analysis_complete = (
        not GhidraProgramUtilities.shouldAskToAnalyze(program)
    )
    return program_info


PyGhidraContext._init_program_info = init_program_info_with_analysis_state


# Upstream get_program_info() also starts a complete semantic/string index as a
# side effect of every metadata, byte-read, decompile, or symbol tool. Keep those
# ordinary tools lightweight. Only an explicit code/string search may request its
# corresponding background index.
def get_program_info_without_implicit_indexing(
    self: PyGhidraContext, binary_name: str
) -> Any:
    program_info = self._lookup_program_info(binary_name)
    if program_info is None:
        available_programs = list(self.programs.keys())
        raise ValueError(
            f"Binary {binary_name} not found. Available binaries: {available_programs}"
        )
    if not program_info.analysis_complete:
        raise RuntimeError(
            json.dumps(
                {
                    "message": f"Analysis incomplete for binary '{binary_name}'.",
                    "binary_name": binary_name,
                    "ghidra_analysis_complete": program_info.ghidra_analysis_complete,
                    "code_indexed": program_info.code_collection is not None,
                    "strings_indexed": program_info.strings is not None,
                    "suggestion": "Analyze the binary, then retry the tool call.",
                }
            )
        )
    return program_info


PyGhidraContext.get_program_info = get_program_info_without_implicit_indexing


def request_search_index(
    function: Callable[..., Any], *, code: bool, strings: bool
) -> Callable[..., Any]:
    """Retain lazy indexing for explicit search tools only."""

    @functools.wraps(function)
    def wrapped(*args: Any, **kwargs: Any) -> Any:
        binary_name = kwargs.get("binary_name", args[0] if args else None)
        context = kwargs.get("ctx")
        if context is None:
            context = next(
                (arg for arg in args if hasattr(arg, "request_context")), None
            )
        if binary_name is None or context is None:
            raise RuntimeError("Unable to resolve search indexing context")
        pyghidra_context = context.request_context.lifespan_context
        pyghidra_context.schedule_indexing(binary_name, code=code, strings=strings)
        return function(*args, **kwargs)

    return wrapped


mcp_tools.search_code = request_search_index(
    mcp_tools.search_code, code=True, strings=False
)
mcp_tools.search_strings = request_search_index(
    mcp_tools.search_strings, code=False, strings=True
)

from pyghidra_mcp.server import main  # noqa: E402


if __name__ == "__main__":
    main()
