---
status: accepted
date: 2026-11-06
decision-makers: maintainer
---

# Agent identity is available before its conversation

## Context

Issue #94 reports Codex starting without a tab or pane icon while associated Claude sessions have theirs. The host already recognizes the CLI process, but quiet processes do not publish an identity, and desktop marks only consult conversations.

## Decision

Publish the recognized CLI independently of transcript availability or explanatory notes, using the existing `RecognitionStatus.terminal_agents`. Desktop marks and the pane's CLI name use the current conversation, then the recognized process, then the previous conversation for the existing historical display. Identity alone does not create a conversation, status, warning, or title.

## Consequences

A Codex before its first message can display its icon. A newly recognized CLI takes precedence over a previous conversation. Exiting an unbound CLI clears its identity through existing process-exit handling. Historical conversations keep their existing display after exit.

## Implementation Plan

- `host/mod.rs`: report identity for non-wrapper pending processes and publish identity-only changes; keep notes separate.
- `workbench/frame_facts.rs`, `render.rs`, `layout/{pane_view,drag}.rs`: share the identity choice for tab and pane marks and the pane name.
- Reuse process recognition and exit subscriptions; do not add polling or infer identity from directory names.

## Verification

`cargo test -p fibo-host --lib quiet_agent_identity`

`cargo test -p fibo-app pane_agent`

`MOORING_FIXTURE_UNBOUND_AGENT=1 python3 scripts/visual-check.py shots <fix-commit> <out> --pages UnboundAgent@1225`

## Alternatives

Waiting for a first prompt leaves a running CLI looking unrecognized. Matching terminal titles or directories gives weaker identity evidence than the process tree already available. Fabricating a session would misrepresent whether conversation content is accessible.

## More Information

Source: https://github.com/majiayu000/mooring/issues/94. Revisit if the process layer can no longer distinguish the foreground CLI; that requires better evidence there, rather than guessed desktop labels.
