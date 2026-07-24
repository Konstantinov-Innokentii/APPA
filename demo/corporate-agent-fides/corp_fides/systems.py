from __future__ import annotations

import os
import shutil
import subprocess
from enum import Enum
from pathlib import Path
from typing import Any

from mcp import ClientSession, StdioServerParameters, types
from mcp.client.stdio import get_default_environment, stdio_client

_PACKAGE_DIR = Path(__file__).resolve().parent
_CRATE_DIR = _PACKAGE_DIR.parent
_CORP_SYSTEMS_DIR = (_CRATE_DIR / ".." / "corp-systems").resolve()


class System(str, Enum):

    HR = "hr"
    FINANCE = "finance"
    TASK_TRACKER = "task_tracker"
    PUBLIC_FORUM = "public_forum"
    VENDOR = "vendor"
    EMAIL = "email"

    @property
    def dir_name(self) -> str:
        return self.value


def resolve_corpus_root(explicit: str | os.PathLike[str] | None = None) -> Path:
    if explicit is not None:
        return Path(explicit).resolve()
    env = os.environ.get("CORP_DATA_ROOT", "").strip()
    if env:
        return Path(env).resolve()
    return _CORP_SYSTEMS_DIR / "data"


def resolve_sink_root(explicit: str | os.PathLike[str] | None = None) -> Path:
    if explicit is not None:
        return Path(explicit).resolve()
    env = os.environ.get("CORP_SINK_ROOT", "").strip()
    if env:
        return Path(env).resolve()
    return _CRATE_DIR / "data"


def resolve_server_bin(explicit: str | os.PathLike[str] | None = None) -> Path:
    if explicit is not None:
        return Path(explicit).resolve()
    env = os.environ.get("CORP_SYSTEMS_BIN", "").strip()
    if env:
        return Path(env).resolve()
    manifest = _CORP_SYSTEMS_DIR / "Cargo.toml"
    binary = _CORP_SYSTEMS_DIR / "target" / "debug" / "corp-systems-mcp"
    if shutil.which("cargo") is None:
        if binary.is_file():
            return binary
        raise RuntimeError(
            f"corp-systems-mcp not found at {binary} and cargo is not installed; "
            f"build the sibling server crate first: cargo build --manifest-path {manifest}"
        )
    build = subprocess.run(
        ["cargo", "build", "-q", "--manifest-path", str(manifest)],
        capture_output=True,
        text=True,
    )
    if build.returncode != 0 or not binary.is_file():
        raise RuntimeError(
            f"building corp-systems-mcp failed:\n{build.stderr}\n"
            f"build it manually: cargo build --manifest-path {manifest}"
        )
    return binary


class CorpSystemsClient:

    def __init__(
        self,
        corpus_root: Path,
        sink_root: Path,
        server_bin: str | os.PathLike[str] | None = None,
    ) -> None:
        self._corpus_root = corpus_root
        self._sink_root = sink_root
        self._server_bin = server_bin
        self._transport_cm: Any = None
        self._session_cm: ClientSession | None = None
        self._session: ClientSession | None = None

    async def __aenter__(self) -> "CorpSystemsClient":
        env = get_default_environment()
        env.update({key: value for key, value in os.environ.items() if key.startswith("CORP_")})
        params = StdioServerParameters(
            command=str(resolve_server_bin(self._server_bin)),
            args=[
                "--data-root",
                str(self._corpus_root),
                "--sink-root",
                str(self._sink_root),
            ],
            env=env,
        )
        self._transport_cm = stdio_client(params)
        read, write = await self._transport_cm.__aenter__()
        self._session_cm = ClientSession(read, write)
        self._session = await self._session_cm.__aenter__()
        await self._session.initialize()
        return self

    async def __aexit__(self, *exc_info: Any) -> None:
        if self._session_cm is not None:
            await self._session_cm.__aexit__(*exc_info)
            self._session_cm = None
            self._session = None
        if self._transport_cm is not None:
            await self._transport_cm.__aexit__(*exc_info)
            self._transport_cm = None

    async def call(self, tool: str, arguments: dict[str, Any]) -> tuple[str, bool]:
        if self._session is None:
            raise RuntimeError("CorpSystemsClient used outside its async context")
        result = await self._session.call_tool(tool, arguments)
        text = "".join(c.text for c in result.content if isinstance(c, types.TextContent))
        return text, bool(result.isError)

    async def list_tool_names(self) -> list[str]:
        if self._session is None:
            raise RuntimeError("CorpSystemsClient used outside its async context")
        listing = await self._session.list_tools()
        return [t.name for t in listing.tools]
