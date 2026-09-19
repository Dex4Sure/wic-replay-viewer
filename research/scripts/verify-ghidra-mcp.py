#!/usr/bin/env python3
"""Start the local Ghidra MCP server and make one real read-only tool call."""

from __future__ import annotations

import asyncio
import json
import sys
from datetime import timedelta
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


def result_payload(result):
    payload = result.structuredContent
    if payload is None:
        payload = [getattr(item, "text", repr(item)) for item in result.content]
    return payload


async def verify() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    server = StdioServerParameters(
        command=str(repo_root / "research" / "scripts" / "ghidra-mcp.sh"),
        cwd=repo_root,
    )

    async with stdio_client(server) as (read_stream, write_stream):
        print("MCP transport connected", file=sys.stderr, flush=True)
        async with ClientSession(
            read_stream,
            write_stream,
            read_timeout_seconds=timedelta(seconds=120),
        ) as session:
            print("Initializing MCP session", file=sys.stderr, flush=True)
            await session.initialize()
            print("Listing MCP tools", file=sys.stderr, flush=True)
            tools = await session.list_tools()
            tool_names = sorted(tool.name for tool in tools.tools)
            if "list_project_binaries" not in tool_names:
                raise RuntimeError("Ghidra MCP did not expose list_project_binaries")

            print("Calling list_project_binaries", file=sys.stderr, flush=True)
            result = await session.call_tool("list_project_binaries", {})
            print("Tool call completed", file=sys.stderr, flush=True)
            if result.isError:
                raise RuntimeError(f"list_project_binaries failed: {result.content!r}")

            binaries = result_payload(result)
            programs = binaries.get("programs", [])
            if not programs or any(
                not program.get("analysis_complete") for program in programs
            ):
                raise RuntimeError(
                    f"Existing analyzed programs reported incomplete: {binaries!r}"
                )

            print("Calling search_symbols_by_name", file=sys.stderr, flush=True)
            result = await session.call_tool(
                "search_symbols_by_name",
                {
                    "binary_name": "/wic.exe",
                    "query": ".*",
                    "functions_only": True,
                    "limit": 1,
                },
            )
            print("Analysis tool call completed", file=sys.stderr, flush=True)
            if result.isError:
                raise RuntimeError(f"search_symbols_by_name failed: {result.content!r}")
            symbol_search = result_payload(result)

            print(
                json.dumps(
                    {
                        "tool_count": len(tool_names),
                        "list_project_binaries": binaries,
                        "search_symbols_by_name": symbol_search,
                    },
                    indent=2,
                    sort_keys=True,
                ),
                flush=True,
            )


if __name__ == "__main__":
    asyncio.run(verify())
