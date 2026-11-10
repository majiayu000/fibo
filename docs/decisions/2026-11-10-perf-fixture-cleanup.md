---
status: accepted
date: 2026-11-10
decision-makers: maintainer
---

# Performance fixtures own their cleanup

## Context

Two old performance fixtures left 120 stand-in Agent terminals running. This is
also recorded in `docs/plan.md` (2026-11-05): a probe can exit its host before the
old RPC-based stop runs, leaving the terminals behind. Preparation could fail
outside the gate's `finally`, and all invocations shared one `state.json`.

## Decision

Give every preparation an explicit, fresh run directory. Record its host before
initialization and its process identities before returning. Stop only those
process groups, including surviving descendants after the host exits. Handle
SIGTERM through cleanup as well as ordinary exceptions and Ctrl-C. Keep the
performance gate's machine-wide lock: process isolation does not eliminate
measurement interference.

This changes test scripts, not application terminal ownership or idle-session
policy. Do not kill user sessions merely because they are idle.

## Consequences and implementation

`perf-fixture.py` takes `RUN_DIR` for prepare/probe/live/stop. `perf-gate.py` and
`theme-live.py` enclose preparation and measurement in cleanup. Cleanup failure
is an error, not a successful measurement. Match recorded process start times
before signalling; never use a process-name search.

The former global state file is left untouched and is no longer read. Older
script copies retain their old behavior; there is no migration or fallback.
SIGKILL and machine failure cannot execute Python cleanup. After a successful
prepare, `stop RUN_DIR` can still clean its recorded processes.

## Verification

`python3 -I scripts/test_perf_fixture.py target/release` launches real host/PTY
processes without a window or model. It covers partial preparation failure,
SIGTERM, host exit before stop, independent runs, repeated stop, stale PID
identity, command-line use, and gate preparation failure/probe timeout. CI runs
it after building the host.

## Alternatives and reopening

A global scan of all new performance directories can select another run's
processes. Application auto-termination of idle sessions would change normal
user behavior and does not fix these test leaks. Revisit application cleanup
only with evidence of a real closed pane retaining its processes.
