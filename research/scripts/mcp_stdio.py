"""POSIX MCP STDIO transport that does not use thread-backed file reads.

Codex 0.147's Linux command sandbox can leave AnyIO's ``AsyncFile`` STDIO
reader idle indefinitely.  Drive the pipe file descriptors from asyncio's
selector instead so project-local MCP servers can complete their handshake in
both sandboxed and ordinary sessions.
"""

from __future__ import annotations

import asyncio
import os
import sys
from contextlib import asynccontextmanager
from typing import AsyncIterator

import anyio
import mcp.types as types
from anyio.streams.memory import MemoryObjectReceiveStream, MemoryObjectSendStream
from mcp.shared.message import SessionMessage


MessageOrError = SessionMessage | Exception


async def _wait_writable(loop: asyncio.AbstractEventLoop, file_descriptor: int) -> None:
    ready = loop.create_future()

    def mark_ready() -> None:
        if not ready.done():
            ready.set_result(None)

    loop.add_writer(file_descriptor, mark_ready)
    try:
        await ready
    finally:
        loop.remove_writer(file_descriptor)


async def _write_all(
    loop: asyncio.AbstractEventLoop, file_descriptor: int, data: bytes
) -> None:
    remaining = memoryview(data)
    while remaining:
        try:
            written = os.write(file_descriptor, remaining)
        except BlockingIOError:
            await _wait_writable(loop, file_descriptor)
            continue
        except InterruptedError:
            continue
        remaining = remaining[written:]


@asynccontextmanager
async def posix_stdio_server() -> AsyncIterator[
    tuple[
        MemoryObjectReceiveStream[MessageOrError],
        MemoryObjectSendStream[SessionMessage],
    ]
]:
    """Expose MCP streams using selector-driven POSIX stdin and stdout."""

    if os.name != "posix":
        raise RuntimeError("The selector-driven MCP STDIO transport requires POSIX")

    stdin_fd = sys.stdin.fileno()
    stdout_fd = sys.stdout.fileno()
    stdin_was_blocking = os.get_blocking(stdin_fd)
    stdout_was_blocking = os.get_blocking(stdout_fd)
    os.set_blocking(stdin_fd, False)
    os.set_blocking(stdout_fd, False)

    read_stream_writer, read_stream = anyio.create_memory_object_stream[MessageOrError](
        0
    )
    write_stream, write_stream_reader = anyio.create_memory_object_stream[
        SessionMessage
    ](0)

    loop = asyncio.get_running_loop()
    input_queue: asyncio.Queue[bytes | Exception] = asyncio.Queue()

    def read_ready() -> None:
        try:
            chunk = os.read(stdin_fd, 65536)
        except BlockingIOError:
            return
        except OSError as error:
            input_queue.put_nowait(error)
            loop.remove_reader(stdin_fd)
            return

        input_queue.put_nowait(chunk)
        if not chunk:
            loop.remove_reader(stdin_fd)

    async def stdin_reader() -> None:
        buffer = b""
        async with read_stream_writer:
            while True:
                chunk = await input_queue.get()
                if isinstance(chunk, Exception):
                    await read_stream_writer.send(chunk)
                    return
                if not chunk:
                    if buffer:
                        await _send_line(buffer)
                    return

                buffer += chunk
                while b"\n" in buffer:
                    line, buffer = buffer.split(b"\n", 1)
                    await _send_line(line)

    async def _send_line(line: bytes) -> None:
        try:
            message = types.JSONRPCMessage.model_validate_json(line)
        except Exception as error:
            await read_stream_writer.send(error)
            return
        await read_stream_writer.send(SessionMessage(message))

    async def stdout_writer() -> None:
        async with write_stream_reader:
            async for session_message in write_stream_reader:
                payload = session_message.message.model_dump_json(
                    by_alias=True, exclude_none=True
                )
                await _write_all(loop, stdout_fd, (payload + "\n").encode("utf-8"))

    loop.add_reader(stdin_fd, read_ready)
    try:
        async with anyio.create_task_group() as task_group:
            task_group.start_soon(stdin_reader)
            task_group.start_soon(stdout_writer)
            try:
                yield read_stream, write_stream
            finally:
                task_group.cancel_scope.cancel()
    finally:
        loop.remove_reader(stdin_fd)
        loop.remove_writer(stdout_fd)
        os.set_blocking(stdin_fd, stdin_was_blocking)
        os.set_blocking(stdout_fd, stdout_was_blocking)


def install_posix_stdio_transport() -> None:
    """Install the selector-driven transport into MCP SDK 1.x FastMCP."""

    if os.name != "posix":
        return

    from mcp.server.fastmcp import server as fastmcp_server

    fastmcp_server.stdio_server = posix_stdio_server
