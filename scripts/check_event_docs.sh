#!/usr/bin/env bash
# Checks that docs/events.md lists exactly the same (contract, event, action)
# triples as the pinned table in tests/tests/event_topics.rs. Run in CI so the
# human-readable reference cannot silently drift from the mechanical check.
set -euo pipefail

cd "$(dirname "$0")/.."

EVENT_TEST="tests/tests/event_topics.rs"
EVENTS_MD="docs/events.md"

# Pinned triples in the test look like:
#   ("batch_payout", "Executed", "executed"),
test_rows=$(
  grep -oE '\("[a-z_]+", "[A-Za-z][A-Za-z0-9]*", "[a-z_]+"\)' "$EVENT_TEST" \
    | sed -E 's/^\("([a-z_]+)", "([A-Za-z0-9]+)", "([a-z_]+)"\)$/\1 \2 \3/' \
    | sort
)

# Documented triples look like:
#   | `batch_payout` | `Executed` | `executed` | ... |
doc_rows=$(
  grep -oE '^\| `[a-z_]+` \| `[A-Za-z][A-Za-z0-9]*` \| `[a-z_]+` \|' "$EVENTS_MD" \
    | sed -E 's/^\| `([a-z_]+)` \| `([A-Za-z0-9]+)` \| `([a-z_]+)` \|.*/\1 \2 \3/' \
    | sort
)

if [ -z "$test_rows" ]; then
  echo "error: no pinned event triples found in $EVENT_TEST" >&2
  exit 1
fi

if [ "$test_rows" != "$doc_rows" ]; then
  echo "error: $EVENTS_MD is out of sync with $EVENT_TEST" >&2
  echo "--- pinned by the test but missing/mismatched in docs/events.md ---" >&2
  comm -23 <(echo "$test_rows") <(echo "$doc_rows") >&2
  echo "--- in docs/events.md but not pinned by the test ---" >&2
  comm -13 <(echo "$test_rows") <(echo "$doc_rows") >&2
  exit 1
fi

contracts=$(echo "$doc_rows" | awk '{print $1}' | sort -u | wc -l)
if [ "$contracts" -lt 5 ]; then
  echo "error: $EVENTS_MD covers $contracts contracts, expected all five" >&2
  exit 1
fi

echo "ok: $EVENTS_MD matches $EVENT_TEST ($(echo "$test_rows" | wc -l) events across $contracts contracts)"
