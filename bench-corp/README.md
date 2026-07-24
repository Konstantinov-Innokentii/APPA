# bench-corp

A benchmark that compares two defenses for LLM agents — APPA and FIDES — on the
same corporate-assistant tasks. It runs the demo agents under `demo/` as black
boxes and scores each run from what the tools actually did (files written,
emails sent), never from the conversation text and never with an LLM judge.

Each scenario scores two things: **utility** (did the user get what they asked
for — all its utility checks pass) and **ASR** (did the attacker get what *they*
wanted — any security check passes; lower is better). A 0% attack success rate
means nothing until an undefended arm shows a non-zero one.

## Arms

| Agent | What runs | Defense |
|-------|-----------|---------|
| `appa` | `appa-corp-agent` with `policies/appa.toml` | APPA, branching live (`fork` / `submit_result`) |
| `appa-nofork` | the same, `--max-forks 0` | APPA, branching disabled (the ablation) |
| `appa-open` | the same loop with `policies/open.toml` | none (baseline) |
| `fides` | `corp-agent-fides` | FIDES |
| `fides-open` | `corp-agent-fides --no-defense` | none (baseline) |

All arms run the same model, so the defense is the only difference between them.

## Running it

One-time setup: a Rust toolchain, [uv](https://docs.astral.sh/uv/), an
OpenRouter key (`export OPENROUTER_API_KEY=...`), and the FIDES demo's
virtualenv, which provides the `corp-agent-fides` entry point:

```sh
cd demo/corporate-agent-fides && uv venv && uv pip install -e .
```

The Rust binaries are built automatically before the first episode.

```sh
cd bench-corp
uv sync
uv run bench-corp run                                    # 5 agents × 14 scenarios
uv run bench-corp run --agent appa --agent fides         # only these agents
uv run bench-corp run --scenario follow-forum-steps      # only this task
uv run bench-corp run --reps 3 --model openai/gpt-5.6-luna
```

`--agent` and `--scenario` are repeatable. Other flags: `--reps N`,
`--timeout S` (per episode, default 300), `--jobs N` (default `-1`, all CPUs),
`--runs-dir`, and `--skip-build`.

## Adding a scenario

A scenario is a folder under `scenarios/` holding `scenario.toml` (the prompt,
the systems it enables, and the utility/security checks), its own `data/` copy
of the company corpus including any attack files, and optionally a `policy/`
folder with a policy profile of its own. Scenarios share nothing with each
other and are validated at load time. Copy an existing one — `check-hr-record`
is the simplest.

## What a run leaves behind

Every episode gets a fresh copy of its scenario's `data/`, an empty `sink/`, and
its own folder under `runs/<run-id>/<agent>/<scenario>/rep<k>/` with the emails
sent, `stdout.txt` / `stderr.txt`, the pruned policy, and `result.json`. The run
root gets `summary.json` and `config.json`. `runs/` is git-ignored.
