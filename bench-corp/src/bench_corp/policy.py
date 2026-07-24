from __future__ import annotations

import tomllib
from functools import lru_cache

import tomli_w

from .checks import KNOWN_SYSTEMS

REQUIRED_SYSTEMS_OF_TOOL: dict[str, frozenset[str]] = {
    f"{verb}_{system}": frozenset({system})
    for system in KNOWN_SYSTEMS
    if system != "email"
    for verb in ("search", "read", "create")
} | {
    "send_email": frozenset({"email"}),
    "share_legal_packet": frozenset({"finance", "email"}),
}


class PolicyError(ValueError):
    pass


@lru_cache(maxsize=None)
def prune_policy(policy_toml: str, enabled_systems: tuple[str, ...]) -> str:
    data = tomllib.loads(policy_toml)
    enabled = set(enabled_systems)
    kept = []
    for tool in data.get("tool", []):
        name = tool.get("name", "")
        required = REQUIRED_SYSTEMS_OF_TOOL.get(name)
        if required is None:
            raise PolicyError(
                f"policy declares tool {name!r} with no known systems; extend REQUIRED_SYSTEMS_OF_TOOL"
            )
        if required <= enabled:
            kept.append(tool)
    data["tool"] = kept
    return tomli_w.dumps(data)


def apply_tool_requires(policy_toml: str, overrides: dict[str, dict]) -> str:
    if not overrides:
        return policy_toml
    data = tomllib.loads(policy_toml)
    by_name = {tool.get("name", ""): tool for tool in data.get("tool", [])}
    for name, requires in overrides.items():
        tool = by_name.get(name)
        if tool is None:
            raise PolicyError(f"scenario overrides requires of tool {name!r}, absent from the pruned policy")
        tool["requires"] = requires
    return tomli_w.dumps(data)
