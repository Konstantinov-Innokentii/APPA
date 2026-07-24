from __future__ import annotations

from agent_framework.security import (
    ConfidentialityLabel,
    ContentLabel,
    IntegrityLabel,
    check_confidentiality_allowed,
    combine_labels,
)

from corp_fides.profile import DEFAULT_PROFILE, Profile, load_profile
from corp_fides.systems import CorpSystemsClient, System
from corp_fides.tools import build_tools

_OFFLINE_CLIENT: CorpSystemsClient = None


def _label(system: System, profile: Profile = DEFAULT_PROFILE) -> ContentLabel:
    label = profile.systems[system]
    return ContentLabel(
        integrity=label.integrity,
        confidentiality=label.confidentiality,
    )


def _send_email_gates(profile: Profile = DEFAULT_PROFILE) -> tuple[bool, ConfidentialityLabel]:
    tools = build_tools(_OFFLINE_CLIENT, profile=profile)
    props = next(t for t in tools if t.name == "send_email").additional_properties
    return props["accepts_untrusted"], ConfidentialityLabel(props["max_allowed_confidentiality"])


def _blocked_by_send_email(context: ContentLabel, profile: Profile = DEFAULT_PROFILE) -> bool:
    accepts_untrusted, max_conf = _send_email_gates(profile)
    integrity_violation = context.integrity == IntegrityLabel.UNTRUSTED and not accepts_untrusted
    confidentiality_violation = not check_confidentiality_allowed(context, max_conf)
    return integrity_violation or confidentiality_violation


def test_injection_flow_is_blocked_at_send_email() -> None:
    context = combine_labels(_label(System.PUBLIC_FORUM), _label(System.HR))
    assert context.integrity == IntegrityLabel.UNTRUSTED
    assert context.confidentiality == ConfidentialityLabel.PRIVATE
    assert _blocked_by_send_email(context)


def test_hr_egress_blocked_even_without_taint() -> None:
    context = _label(System.HR)
    assert context.integrity == IntegrityLabel.TRUSTED
    assert _blocked_by_send_email(context)


def test_finance_egress_blocked_for_every_recipient() -> None:
    context = _label(System.FINANCE)
    assert context.integrity == IntegrityLabel.TRUSTED
    assert context.confidentiality == ConfidentialityLabel.PRIVATE
    assert _blocked_by_send_email(context)


def test_public_internal_data_may_be_emailed() -> None:
    context = _label(System.TASK_TRACKER)
    assert not _blocked_by_send_email(context)


def test_profile_can_raise_email_cap_to_private(tmp_path) -> None:
    path = tmp_path / "audience-intersection.json"
    path.write_text(
        '{"version": 1, "tools": {"send_email": {"max_allowed_confidentiality": "private"}}}',
        encoding="utf-8",
    )
    profile = load_profile(path)
    context = _label(System.FINANCE, profile)
    assert not _blocked_by_send_email(context, profile)
