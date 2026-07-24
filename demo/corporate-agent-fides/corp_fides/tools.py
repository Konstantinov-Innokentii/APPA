from __future__ import annotations

from collections.abc import Collection
from typing import Any

from agent_framework import Content, tool
from agent_framework.security import ConfidentialityLabel, IntegrityLabel

from .profile import ALL_TOOL_NAMES, DEFAULT_PROFILE, Profile, ResultLabel
from .systems import CorpSystemsClient, System

_NEUTRAL = ResultLabel(IntegrityLabel.TRUSTED, ConfidentialityLabel.PUBLIC)


def _labeled(text: str, label: ResultLabel) -> Content:
    return Content.from_text(
        text,
        additional_properties={
            "security_label": {
                "integrity": label.integrity.value,
                "confidentiality": label.confidentiality.value,
            }
        },
    )


def build_tools(
    client: CorpSystemsClient,
    available: Collection[str] | None = None,
    profile: Profile = DEFAULT_PROFILE,
) -> list[Any]:
    if available is None:
        available = ALL_TOOL_NAMES

    async def forward(name: str, arguments: dict[str, Any], label: ResultLabel) -> list[Content]:
        text, is_error = await client.call(name, arguments)
        return [_labeled(text, _NEUTRAL if is_error else label)]

    def make_search(system: System):
        label = profile.systems[system]

        async def _search(query: str) -> list[Content]:
            return await forward(f"search_{system.dir_name}", {"query": query}, label)

        return _search

    def make_read(system: System):
        label = profile.systems[system]

        async def _read(file: str) -> list[Content]:
            return await forward(f"read_{system.dir_name}", {"file": file}, label)

        return _read

    def make_create(system: System):
        async def _create(file: str, content: str) -> list[Content]:
            return await forward(f"create_{system.dir_name}", {"file": file, "content": content}, _NEUTRAL)

        return _create

    tools: list[Any] = []

    def add(name: str, fn: Any, description: str, props: dict[str, Any]) -> None:
        if name in available:
            tools.append(tool(fn, name=name, description=description, additional_properties=props))

    def policy_props(name: str, source_integrity: IntegrityLabel | None = None) -> dict[str, Any]:
        policy = profile.tools[name]
        props: dict[str, Any] = {"accepts_untrusted": policy.accepts_untrusted}
        if source_integrity is not None:
            props["source_integrity"] = source_integrity.value
        if policy.max_allowed_confidentiality is not None:
            props["max_allowed_confidentiality"] = policy.max_allowed_confidentiality.value
        return props

    descriptions = {
        System.HR: "the HR system (employee records, org roster, policies)",
        System.FINANCE: "the finance system (invoices, budgets, expense policy)",
        System.TASK_TRACKER: "the task tracker (tickets, tasks, assignments)",
        System.PUBLIC_FORUM: "the public forum (external, untrusted user-posted content)",
        System.VENDOR: "the vendor system (contracts, legal packets, vendor records)",
    }
    for system, blurb in descriptions.items():
        source_props = policy_props(f"search_{system.dir_name}", profile.systems[system].integrity)
        add(
            f"search_{system.dir_name}",
            make_search(system),
            f"Search {blurb} by keyword.",
            dict(source_props),
        )
        add(
            f"read_{system.dir_name}",
            make_read(system),
            f"Read a file from {blurb} by exact file name.",
            policy_props(f"read_{system.dir_name}", profile.systems[system].integrity),
        )
        add(
            f"create_{system.dir_name}",
            make_create(system),
            f"Create a new file in {blurb}.",
            policy_props(f"create_{system.dir_name}"),
        )

    async def send_email(to: str, subject: str, body: str) -> list[Content]:
        return await forward("send_email", {"to": to, "subject": subject, "body": body}, _NEUTRAL)

    add(
        "send_email",
        send_email,
        "Send an outbound email. Delivers the message to the given recipient.",
        policy_props("send_email"),
    )

    async def share_legal_packet(file: str, to: str) -> list[Content]:
        return await forward(
            "share_legal_packet",
            {"file": file, "to": to},
            profile.systems[System.FINANCE],
        )

    add(
        "share_legal_packet",
        share_legal_packet,
        "Read a finance legal packet and email its exact contents to the given recipient.",
        policy_props("share_legal_packet", profile.systems[System.FINANCE].integrity),
    )

    return tools
