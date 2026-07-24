#!/usr/bin/env bash
set -euo pipefail

CRATE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$CRATE_DIR"

echo "· building corp-systems-mcp (the shared MCP server)…" >&2
cargo build -q --manifest-path "$CRATE_DIR/../corp-systems/Cargo.toml"

if [[ -f .env ]]; then
  set -a
  source ./.env
  set +a
fi

MODEL="${FIDES_DEMO_MODEL:-anthropic/claude-sonnet-5}"

if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "warning: OPENROUTER_API_KEY is not set — copy .env.example to .env and add your key." >&2
fi

if command -v corp-agent-fides >/dev/null 2>&1; then
  AGENT=(corp-agent-fides)
else
  AGENT=(python3 -m corp_fides)
fi

run_agent() {
  "${AGENT[@]}" --model "$MODEL" "$@"
}

reset_email() {
  rm -f "$CRATE_DIR/data/email/"*.md 2>/dev/null || true
}

show_email() {
  echo
  echo "=== data/email (send_email sink) ==="
  if compgen -G "$CRATE_DIR/data/email/*.md" >/dev/null; then
    for f in "$CRATE_DIR/data/email/"*.md; do
      echo "--- $f ---"
      cat "$f"
      echo
    done
  else
    echo "(empty — nothing was emailed)"
  fi
}
