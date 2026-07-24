# APPA — Agentic Permissions Policy Algebra

APPA is a value-granular information-flow policy engine for LLM agents. It sits
between the agent and its tools and answers one question before every proposed
flow: *can this value, derived from these sources, legally flow into this sink?*

## Motivation

LLM agents operating with broad authority over tools and data frequently
process mixed-confidentiality contexts, creating severe security risks.
Unauthorized data exfiltration, unintended tool execution, and policy
violations can arise not only from adversarial prompt injections but also from
model hallucinations, ambiguous instructions, or agent reasoning errors.
Imperative guardrails relying on ad-hoc conditional logic (`if` statements)
fail to scale as interaction complexity grows exponentially with the number of
tools and their properties, whereas static access controls severely degrade
utility. Dynamic information flow control (IFC) tracks security labels to
enforce structural security, but observing unvetted data permanently taints an
agent's execution trajectory. APPA builds upon this foundation by preventing
trajectory taint accumulation before it occurs while allowing developers to
embed custom domain logic cleanly via modular authority handlers and sanitizer
implementations.

## Branching for taint confinement

APPA is built on the principle of *branching for taint confinement*. It
prospectively detects Label narrowing and requires explicit acceptance before
dispatching narrowing calls. To isolate unvetted data, APPA provides a
host-managed branch lifecycle where a label-seeded child trajectory absorbs
label descent, keeping the parent context untainted. The child can then be
discarded or merged through a policy-checked exit point, allowing a trusted
sanitizer to return a bounded derivative without polluting the parent context.
This guarantees parent label preservation and checked return while respecting
the non-reversibility of label descent.

APPA is governed by two monoids: a security label (audience × trust) and a
globally shared event log that maintains action visibility across isolated
trajectories. Pre-read acquisition checks construct Remedy plans that present
sound options to accept or decline narrowing calls, alongside advisory branch
suggestions, while per-call rulings authorize controlled exceptions without
granting persistent privileges. Conditional on host-managed branch execution,
parent label preservation, monotone descent, merge confinement, and
completeness over the implemented Remedy subset are formally provable. APPA is
evaluated on a multi-turn benchmark suite designed for complex tool-chaining
and context-branching scenarios — `bench-corp` in this repository.
