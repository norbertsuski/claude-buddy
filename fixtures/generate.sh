#!/usr/bin/env bash
# Stamp the fixture registry and the fixture usage file with times and pids that
# are true *now*.
#
# Most of what the widget shows is derived from relative time and from process
# liveness, so a registry file with absolute timestamps frozen into it decays:
# committed today it reads as a row of long-dead sessions next month, and a pid
# written down today belongs to something else — or nothing — by tomorrow. The
# transcripts under projects/ carry nothing time-sensitive and are committed as
# they are; everything with a clock or a pid in it is written here instead, at
# run time, from the cast table below.
#
# Two derivations need care:
#
# * Paused means ten minutes of quiet (`watcher::state::PAUSED_THRESHOLD_MS`),
#   so docs-site is stamped twenty-six minutes back and reaches that threshold
#   the moment the widget reads it.
# * Liveness is `kill(pid, 0)` plus a one-sided process-start comparison: a
#   process that started *after* its registry entry is a recycled pid and reads
#   as dead. A pid invented here would fail the first half and a pid spawned
#   here would fail the second, so the five live sessions borrow the oldest
#   processes actually running on this machine — old enough to back-date a
#   session by hours, and stable enough to still be there while you look at the
#   widget. infra-tools gets 999999 instead, which is above macOS's PID_MAX and
#   so can never be anything at all: that is the whole point of it.
#
# Run by scripts/dev-fixtures.sh, which launches the app against the result.
# Safe to run on its own, and safe to run repeatedly.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
SESSIONS="$ROOT/sessions"
PROJECTS="$ROOT/projects"
USAGE="$ROOT/usage.json"

NOW_S="$(date +%s)"
NOW_MS=$((NOW_S * 1000))

# The oldest processes on this machine, oldest first, as "<age-seconds> <pid>".
# Elapsed time rather than a start timestamp for the same reason the app uses
# it: `ps -o etime=` carries no timezone to get wrong.
oldest_processes() {
  ps -axo pid=,etime= | awk '
    {
      pid = $1
      # launchd is the oldest process on every Mac and would otherwise be
      # borrowed every time, and "pid 1" in the popover reads as a bug rather
      # than as a session.
      if (pid <= 1) next
      elapsed = $2
      days = 0
      if (split(elapsed, dash, "-") == 2) { days = dash[1]; elapsed = dash[2] }
      parts = split(elapsed, clock, ":")
      if (parts == 2) seconds = clock[1] * 60 + clock[2]
      else if (parts == 3) seconds = clock[1] * 3600 + clock[2] * 60 + clock[3]
      else next
      print days * 86400 + seconds, pid
    }
  ' | sort -rn -k1,1 | awk '!seen[$2]++'
}

# `CB_FIXTURE_HOT=1` adds two more working sessions and spends the five-hour
# limit down to 94%, which is the state the crazy-mode screenshots are taken in.
# It is opt-in and additive: the default cast is byte-for-byte what it was, so
# every screenshot taken before this still shows what it says it does.
HOT="${CB_FIXTURE_HOT:-}"
NEEDED=6
[ -n "$HOT" ] && NEEDED=8

BORROWED="$(oldest_processes)"
BORROWED_COUNT="$(printf '%s\n' "$BORROWED" | grep -c . || true)"
[ "$BORROWED_COUNT" -ge "$NEEDED" ] || {
  echo "only $BORROWED_COUNT processes to borrow pids from — cannot build the fixture" >&2
  exit 1
}

# The nth-oldest process's pid, and its age in seconds.
borrowed_pid() { printf '%s\n' "$BORROWED" | sed -n "${1}p" | awk '{print $2}'; }
borrowed_age() { printf '%s\n' "$BORROWED" | sed -n "${1}p" | awk '{print $1}'; }

# A JSON string, or `null` for an empty argument. The registry writes null for
# an absent jobId and omits nothing, so the fixture does the same.
json() { if [ -z "$1" ]; then printf 'null'; else printf '"%s"' "$1"; fi; }

# One `<pid>.json` registry record.
#
# `uptime_s` is how long ago the session is claimed to have started, clamped to
# the age of the borrowed process so the liveness comparison cannot read the
# entry as a recycled pid. `status_age_s` is how long the session has held its
# current state, and is what every state except dead is derived from.
emit_session() {
  local slot="$1" session_id="$2" name="$3" cwd="$4" kind="$5" job_id="$6" \
    status="$7" waiting_for="$8" uptime_s="$9" status_age_s="${10}"

  local pid
  if [ "$slot" = "gone" ]; then
    pid=999999
  else
    pid="$(borrowed_pid "$slot")"
    local age_s
    age_s="$(borrowed_age "$slot")"
    [ "$uptime_s" -le "$age_s" ] || uptime_s="$age_s"
  fi

  local started_ms=$((NOW_MS - uptime_s * 1000))
  local status_ms=$((NOW_MS - status_age_s * 1000))
  # Claude Code writes this in local time, in `ps -o lstart=` format. Nothing
  # reads it — liveness goes through elapsed time precisely because the two
  # disagree about the timezone — but a fixture record should carry the fields
  # a real one carries.
  local proc_start
  proc_start="$(date -r "$((started_ms / 1000))" '+%a %b %e %H:%M:%S %Y')"

  cat > "$SESSIONS/$pid.json" <<JSON
{
  "pid": $pid,
  "sessionId": "$session_id",
  "cwd": "$cwd",
  "startedAt": $started_ms,
  "procStart": "$proc_start",
  "version": "2.1.234",
  "kind": "$kind",
  "entrypoint": "cli",
  "messagingSocketPath": "/tmp/cc-socks/$pid.sock",
  "name": "$name",
  "nameSource": "derived",
  "jobId": $(json "$job_id"),
  "status": $(json "$status"),
  "updatedAt": $status_ms,
  "statusUpdatedAt": $status_ms,
  "waitingFor": $(json "$waiting_for")
}
JSON

  # Match the transcript's mtime to the session's status time. Nothing in this
  # cast depends on it — every entry here reports a status, and a session that
  # reports one is never measured by its transcript — but a fixture whose
  # transcript was last touched at checkout time is a trap for whoever adds a
  # `claude-desktop` entry, where mtime is the only clock there is.
  local transcript
  transcript="$(find "$PROJECTS" -name "$session_id.jsonl" | head -1)"
  [ -n "$transcript" ] || { echo "no transcript for $name ($session_id)" >&2; exit 1; }
  touch -t "$(date -r "$((status_ms / 1000))" '+%Y%m%d%H%M.%S')" "$transcript"

  printf '  %-16s %-8s pid %-7s state age %s\n' \
    "$name" "${status:-none}" "$pid" "$(printf '%dm%02ds' $((status_age_s / 60)) $((status_age_s % 60)))"
}

# Write the one transcript that cannot be committed.
#
# Everything under projects/ is committed because none of the fields the
# widget reads out of a transcript is time-sensitive — except these. A task's
# start record is only believed when it is stamped no earlier than the
# session's own `startedAt` (`watcher::tasks::apply_events`), which stops a
# resumed session inheriting a dead process's tasks, and `startedAt` is
# stamped at run time. A committed timestamp would be dropped by that
# boundary and the session would read `paused` instead of `tasking`.
#
# Two tasks are left running and one is left finished, so the row shows the
# plural detail, the popover lists two lines, and the finished one proves it
# is filtered out of both.
emit_task_transcript() {
  local session_id="$1" cwd="$2" started_ms="$3"
  local dir="$PROJECTS/$(printf '%s' "$cwd" | tr './' '--')"
  mkdir -p "$dir"

  # Comfortably after startedAt and before now, in the format transcripts use.
  local at
  at="$(date -u -r "$(( started_ms / 1000 + 60 ))" '+%Y-%m-%dT%H:%M:%S.000Z')"

  cat > "$dir/$session_id.jsonl" <<JSON
{"type":"user","uuid":"cc000001-0000-4000-8000-000000000001","parentUuid":null,"sessionId":"$session_id","version":"2.1.234","cwd":"$cwd","gitBranch":"ci/flaky-suite","message":{"role":"user","content":[{"type":"text","text":"Run the suite in the background and keep an eye on CI while it goes."}]}}
{"type":"assistant","uuid":"cc000001-0000-4000-8000-000000000002","parentUuid":"cc000001-0000-4000-8000-000000000001","sessionId":"$session_id","version":"2.1.234","cwd":"$cwd","gitBranch":"ci/flaky-suite","effort":"medium","message":{"role":"assistant","model":"claude-sonnet-5","content":[{"type":"tool_use","id":"toolu_fixture_lint","name":"Bash","input":{"command":"npm run lint","description":"Lint the workspace","run_in_background":true}}]}}
{"type":"user","uuid":"cc000001-0000-4000-8000-000000000003","parentUuid":"cc000001-0000-4000-8000-000000000002","sessionId":"$session_id","version":"2.1.234","cwd":"$cwd","gitBranch":"ci/flaky-suite","timestamp":"$at","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_fixture_lint"}]},"toolUseResult":{"backgroundTaskId":"bfixlint01"}}
{"type":"queue-operation","operation":"enqueue","timestamp":"$at","sessionId":"$session_id","content":"<task-notification> <task-id>bfixlint01</task-id> <status>completed</status> <summary>Background command \"npm run lint\" completed</summary> </task-notification>"}
{"type":"assistant","uuid":"cc000001-0000-4000-8000-000000000004","parentUuid":"cc000001-0000-4000-8000-000000000003","sessionId":"$session_id","version":"2.1.234","cwd":"$cwd","gitBranch":"ci/flaky-suite","effort":"medium","message":{"role":"assistant","model":"claude-sonnet-5","content":[{"type":"tool_use","id":"toolu_fixture_test","name":"Bash","input":{"command":"npm test","description":"Run the whole suite","run_in_background":true}}]}}
{"type":"user","uuid":"cc000001-0000-4000-8000-000000000005","parentUuid":"cc000001-0000-4000-8000-000000000004","sessionId":"$session_id","version":"2.1.234","cwd":"$cwd","gitBranch":"ci/flaky-suite","timestamp":"$at","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_fixture_test"}]},"toolUseResult":{"backgroundTaskId":"bfixtest01"}}
{"type":"assistant","uuid":"cc000001-0000-4000-8000-000000000006","parentUuid":"cc000001-0000-4000-8000-000000000005","sessionId":"$session_id","version":"2.1.234","cwd":"$cwd","gitBranch":"ci/flaky-suite","effort":"medium","message":{"role":"assistant","model":"claude-sonnet-5","content":[{"type":"tool_use","id":"toolu_fixture_ci","name":"Monitor","input":{"description":"Watch the CI run"}}]}}
{"type":"user","uuid":"cc000001-0000-4000-8000-000000000007","parentUuid":"cc000001-0000-4000-8000-000000000006","sessionId":"$session_id","version":"2.1.234","cwd":"$cwd","gitBranch":"ci/flaky-suite","timestamp":"$at","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_fixture_ci"}]},"toolUseResult":{"taskId":"bfixci0001","timeoutMs":600000}}
{"type": "custom-title", "customTitle": "Flaky suite triage", "sessionId": "$session_id"}
JSON
}

mkdir -p "$SESSIONS"
# Every run borrows different pids, so last run's files have to go: they name
# processes that have since been reassigned or have gone away, and would show up
# as a second cast of dead sessions.
rm -f "$SESSIONS"/*.json

echo "registry -> $SESSIONS"

# The tasking session's transcript, written before its registry entry because
# `emit_session` requires the transcript to be there. 1560 seconds of quiet, so
# it is past PAUSED_THRESHOLD_MS and would read `paused` if its tasks were not
# running.
TASK_UPTIME_S=4200
emit_task_transcript 6f7a8b9c-8293-4405-9f16-6b8c9dae0fb6 \
  /Users/n/Code/test-runner "$(( NOW_MS - TASK_UPTIME_S * 1000 ))"

# The cast the README's screenshots show, in the order they appear there.
#
#           slot  sessionId                              name              cwd                             kind          jobId              status    waitingFor      uptime  state age
emit_session 1    7b1f0c2a-3d4e-4f50-9a61-0c2d3e4f5a60   api-service       /Users/n/Code/api-service       interactive   ""                 waiting   "input needed"  7920    240
# A background job belongs to whichever session shares its working directory —
# the only link the registry offers — so this one repeats api-service's cwd
# exactly, and is demoted behind it in the row rather than counted as a session.
emit_session 2    9c2e1d3b-4e5f-4061-ab72-1d3e4f5a6b71   migrate-schemas   /Users/n/Code/api-service       bg            job_01hq8w2n4k     busy      ""              540     45
emit_session 3    4d3c2b1a-5f60-4172-bc83-2e4f5a6b7c82   web-app           /Users/n/Code/web-app           interactive   ""                 busy      ""              2820    60
emit_session 4    1a2b3c4d-6071-4283-8d94-3f5a6b7c8d93   design-system     /Users/n/Code/design-system     interactive   ""                 idle      ""              4800    180
# Past PAUSED_THRESHOLD_MS, which is ten minutes.
emit_session 5    5e6f7a8b-7182-4394-8e05-4a6b7c8d9ea4   docs-site         /Users/n/Code/docs-site         interactive   ""                 idle      ""              11100   1560
# Two background shells and a CI watch are running, so ten minutes of quiet
# reads `tasking` rather than `paused`.
emit_session 6    6f7a8b9c-8293-4405-9f16-6b8c9dae0fb6   test-runner       /Users/n/Code/test-runner       interactive   ""                 idle      ""              $TASK_UPTIME_S  1560
# Still says it is busy; the pid says otherwise, and death outranks status.
emit_session gone 2c3d4e5f-8293-44a5-8f16-5b7c8d9eafb5   infra-tools       /Users/n/Code/infra-tools       interactive   ""                 busy      ""              3120    480

# Two more working sessions, so the fire in crazy mode reaches its top step.
# Only under CB_FIXTURE_HOT.
if [ -n "$HOT" ]; then
  emit_session 7  8f9e0d1c-a2b3-4c4d-9e5f-6a7b8c9d0e1f   payments-api      /Users/n/Code/payments-api      interactive   ""                 busy      ""              3600    30
  emit_session 8  3b4c5d6e-f708-4192-a3b4-c5d6e7f80912   search-index      /Users/n/Code/search-index      interactive   ""                 busy      ""              2400    20
fi

# The five-hour meter: 36% spent, two hours forty to the reset, which is what
# the popover in the README reads. `resets_at` is an absolute instant the widget
# counts down to, so it has to be stamped now as well — a lapsed one is dropped
# outright rather than shown as a spent window.
if [ -n "$HOT" ]; then
  UTILIZATION=94
  RESETS_AT="$(date -u -r "$((NOW_S + 18 * 60))" '+%Y-%m-%dT%H:%M:%S+00:00')"
else
  UTILIZATION=36
  RESETS_AT="$(date -u -r "$((NOW_S + 2 * 3600 + 40 * 60))" '+%Y-%m-%dT%H:%M:%S+00:00')"
fi
cat > "$USAGE" <<JSON
{
  "cachedUsageUtilization": {
    "fetchedAtMs": $NOW_MS,
    "utilization": {
      "five_hour": {
        "utilization": $UTILIZATION,
        "resets_at": "$RESETS_AT",
        "limit_dollars": null
      },
      "seven_day": {
        "utilization": 41,
        "resets_at": "$(date -u -r "$((NOW_S + 3 * 86400))" '+%Y-%m-%dT%H:%M:%S+00:00')",
        "limit_dollars": null
      },
      "seven_day_opus": {
        "utilization": 12,
        "resets_at": "$(date -u -r "$((NOW_S + 3 * 86400))" '+%Y-%m-%dT%H:%M:%S+00:00')",
        "limit_dollars": null
      }
    }
  }
}
JSON

echo "usage    -> $USAGE ($UTILIZATION% spent, resets $RESETS_AT)"
