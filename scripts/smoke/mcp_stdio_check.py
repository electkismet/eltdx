"""Smoke-test an installed eltdx MCP stdio entry point."""

from __future__ import annotations

import asyncio
import shutil

from mcp import Client, StdioServerParameters, stdio_client


async def _check() -> None:
    executable = shutil.which("eltdx-mcp")
    if executable is None:
        raise RuntimeError("installed eltdx-mcp console script is unavailable")
    parameters = StdioServerParameters(
        command=executable,
        args=[],
    )
    async with Client(stdio_client(parameters), mode="legacy") as client:
        tools = await client.list_tools()
        if len(tools.tools) != 22:
            raise RuntimeError(f"expected 22 MCP tools, got {len(tools.tools)}")
        resources = await client.list_resources()
        if len(resources.resources) != 8:
            raise RuntimeError(f"expected 8 MCP resources, got {len(resources.resources)}")
        document = await client.read_resource("eltdx://docs/mcp")
        if "# MCP" not in document.contents[0].text:
            raise RuntimeError("bundled MCP documentation is unavailable")


if __name__ == "__main__":
    asyncio.run(asyncio.wait_for(_check(), timeout=20.0))
