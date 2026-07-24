from __future__ import annotations

import json
import os
import re
import shutil
import signal
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path

from .agents import Agent, PolicyTarget, command_for
from .checks import CheckResult, evaluate_check, parse_emails
from .policy import apply_tool_requires, prune_policy
from .scenario import Scenario

_APPA_POLICY_EVENT = re.compile(r"^appa:.*\bblock", re.IGNORECASE | re.MULTILINE)
_FIDES_BLOCK = re.compile(r"\bBLOCKED\b")
_REMEDY = re.compile(r"^appa: remedy authorized\b", re.MULTILINE)


def _count(pattern: re.Pattern[str], text: str) -> int:
    return sum(1 for _ in pattern.finditer(text))


@dataclass(frozen=True)
class EpisodeResult:
    agent: str
    scenario: str
    rep: int
    utility: bool | None
    security: bool | None
    error: str | None
    duration_s: float
    emails: int
    answer_present: bool
    policy_events: int
    remedy_calls: int
    checks: list[CheckResult]


def episode_record(result: EpisodeResult) -> dict:
    return {k: v for k, v in result.__dict__.items() if k != "checks"}


def _terminate_group(process: subprocess.Popen) -> None:
    for sig, grace in ((signal.SIGTERM, 5.0), (signal.SIGKILL, 5.0)):
        try:
            os.killpg(process.pid, sig)
        except ProcessLookupError:
            return
        try:
            process.wait(timeout=grace)
            return
        except subprocess.TimeoutExpired:
            continue


def _stage_policy(agent: Agent, scenario: Scenario, episode_dir: Path) -> Path | None:
    match agent.policy_target:
        case PolicyTarget.APPA_GUARDED:
            source = scenario.policy_profile.appa if scenario.policy_profile is not None else agent.policy_file
        case PolicyTarget.APPA_OPEN:
            source = agent.policy_file
        case PolicyTarget.FIDES:
            if scenario.policy_profile is None:
                return None
            destination = episode_dir / "fides.json"
            shutil.copyfile(scenario.policy_profile.fides, destination)
            return destination
        case PolicyTarget.NONE:
            return None

    if source is None or agent.policy_file is None:
        raise ValueError(f"{agent.name}: APPA agents require a source policy")
    pruned = prune_policy(source.read_text(), scenario.systems)
    pruned = apply_tool_requires(pruned, scenario.policy_requires.get(agent.policy_file.stem, {}))
    destination = episode_dir / "policy.toml"
    destination.write_text(pruned)
    return destination


def run_episode(
    agent: Agent,
    scenario: Scenario,
    rep: int,
    *,
    model: str,
    episode_dir: Path,
    timeout_s: float,
) -> EpisodeResult:
    episode_dir = episode_dir.resolve()
    episode_dir.mkdir(parents=True)
    shutil.copytree(scenario.data, episode_dir / "data")
    (episode_dir / "sink").mkdir()
    policy_path = _stage_policy(agent, scenario, episode_dir)

    env = os.environ.copy()
    env["CORP_ENABLED_SYSTEMS"] = ",".join(scenario.systems)

    command = command_for(
        agent,
        prompt=scenario.prompt,
        model=model,
        episode_dir=episode_dir,
        policy_path=policy_path,
    )
    stdout_path = episode_dir / "stdout.txt"
    stderr_path = episode_dir / "stderr.txt"
    started = time.monotonic()
    error: str | None = None
    with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        process = subprocess.Popen(
            command,
            stdout=stdout,
            stderr=stderr,
            env=env,
            cwd=episode_dir,
            start_new_session=True,
        )
        try:
            code = process.wait(timeout=timeout_s)
            if code != 0:
                error = f"exit {code}"
        except subprocess.TimeoutExpired:
            _terminate_group(process)
            error = "timeout"
    duration = time.monotonic() - started

    answer = stdout_path.read_text(errors="replace")
    stderr_text = stderr_path.read_text(errors="replace")
    emails = parse_emails(episode_dir / "sink")

    def evaluate(check):
        return evaluate_check(
            check,
            episode_data=episode_dir / "data",
            scenario_data=scenario.data,
            emails=emails,
            answer=answer,
        )

    utility_results = [evaluate(check) for check in scenario.utility]
    security_results = [evaluate(check) for check in scenario.security]
    results = [*utility_results, *security_results]

    result = EpisodeResult(
        agent=agent.name,
        scenario=scenario.name,
        rep=rep,
        utility=all(r.passed for r in utility_results) if utility_results else None,
        security=any(r.passed for r in security_results) if security_results else None,
        error=error,
        duration_s=round(duration, 2),
        emails=len(emails),
        answer_present=bool(answer.strip()),
        policy_events=_count(_APPA_POLICY_EVENT, stderr_text) + _count(_FIDES_BLOCK, stderr_text),
        remedy_calls=_count(_REMEDY, stderr_text),
        checks=results,
    )
    (episode_dir / "result.json").write_text(
        json.dumps(
            {
                **episode_record(result),
                "checks": [check.__dict__ for check in results],
                "command": command,
            },
            indent=2,
        )
        + "\n"
    )
    return result
