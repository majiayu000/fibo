#!/bin/sh
# CI test step: the whole workspace once; then each failed case of managed_session, and only
# those, once more on its own.
#
# managed_session drives real terminals, agents and the host against wall-clock limits. On a
# shared 3-core runner one random case in it fails per run (a different one each time), the
# same way it does locally under load. A failure anywhere else, a compile error, or a
# managed_session case that fails again alone still fails the step. Every retried case is
# listed in the job summary and in $retries (uploaded as an artifact) so it can be counted and
# made deterministic later.
set -u
# One random case per run is the expected noise; two in one run can still be bad luck. More
# than that is the suite getting worse, not one unlucky case, so it fails without a retry.
max_retries=2
retries=target/ci-test-retries.txt
log=$(mktemp)
# A pipeline's status is tee's; keep cargo's own.
{
	cargo test --workspace --no-fail-fast 2>&1
	echo $? >"$log.status"
} | tee "$log"
if [ "$(cat "$log.status")" = 0 ]; then
	exit 0
fi
if grep -q '^error: could not compile' "$log"; then
	echo "ci-test: compile error, no retry"
	exit 1
fi
# "Running tests/managed_session.rs (...)" starts that binary's output; doc-tests and unit
# tests have their own headers.
# CI colors cargo's output; strip the escapes so the headers match.
esc=$(printf '\033')
failures=$(sed "s/$esc\[[0-9;]*m//g" "$log" | awk '
	/^ *Running / { bin = $2 }
	/^ *Doc-tests / { bin = "doc-tests" }
	/^test .* \.\.\. FAILED$/ { print bin "\t" $2 }
')
if [ -z "$failures" ]; then
	echo "ci-test: cargo test failed without a failed case to retry"
	exit 1
fi
others=$(printf '%s\n' "$failures" | grep -v '^tests/managed_session\.rs	' || true)
if [ -n "$others" ]; then
	echo "ci-test: failures outside managed_session, no retry:"
	printf '%s\n' "$others"
	exit 1
fi
# Each line goes to $retries and, on CI, to the job summary.
report() {
	echo "$1" >>"$retries"
	if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
		echo "- $1" >>"$GITHUB_STEP_SUMMARY"
	fi
}
mkdir -p "$(dirname "$retries")"
: >"$retries"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
	echo "### managed_session retries" >>"$GITHUB_STEP_SUMMARY"
fi
count=$(printf '%s\n' "$failures" | wc -l | tr -d ' ')
if [ "$count" -gt "$max_retries" ]; then
	echo "::error::$count managed_session cases failed, more than $max_retries; no retry"
	for case in $(printf '%s\n' "$failures" | cut -f2); do
		report "managed_session::$case failed, not retried ($count > $max_retries)"
	done
	exit 1
fi
status=0
for case in $(printf '%s\n' "$failures" | cut -f2); do
	echo "ci-test: retrying managed_session::$case"
	if cargo test -p mooring-host --test managed_session -- --exact "$case"; then
		echo "::warning::managed_session::$case failed once and passed on retry"
		report "managed_session::$case failed once and passed on retry"
	else
		echo "::error::managed_session::$case failed twice"
		report "managed_session::$case failed twice"
		status=1
	fi
done
exit $status
