"""Smoke-test an installed eltdx MCP stdio entry point."""

from __future__ import annotations

import asyncio
import sys

from mcp import Client, StdioServerParameters, stdio_client


async def _check() -> None:
    parameters = StdioServerParameters(
        command=sys.executable,
        args=["-m", "eltdx.mcp"],
    )
    async with Client(stdio_client(parameters), mode="legacy") as client:
        tools = await client.list_tools()
        if len(tools.tools) != 17:
            raise RuntimeError(f"expected 17 MCP tools, got {len(tools.tools)}")
        resources = await client.list_resources()
        if len(resources.resources) != 8:
            raise RuntimeError(f"expected 8 MCP resources, got {len(resources.resources)}")
        document = await client.read_resource("eltdx://docs/mcp")
        if "# MCP" not in document.contents[0].text:
            raise RuntimeError("bundled MCP documentation is unavailable")


if __name__ == "__main__":
    asyncio.run(_check())
