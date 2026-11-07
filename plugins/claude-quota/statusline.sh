#!/bin/sh
# Claude Code runs this as its status line. It keeps the payload when it carries the
# account's limits (API-key sign-ins and the first render of a session have none), then
# hands the same payload to the status line that was set before, which prints as ever.
data=$1
payload=$(cat)
case $payload in
*'"five_hour"'* | *'"seven_day"'*)
  umask 077
  tmp="$data/.statusline.$$"
  printf '%s' "$payload" >"$tmp" && mv -f "$tmp" "$data/statusline.json"
  ;;
esac
# Each conversation's own copy, for how full its context window is: the file above holds only
# the conversation that spoke last. One this session has not written yet clears out the old.
id=$(printf '%s' "$payload" | sed -n 's/.*"session_id"[[:space:]]*:[[:space:]]*"\([A-Za-z0-9_-]*\)".*/\1/p' | head -n 1)
case $payload in
*'"context_window"'*)
  if [ -n "$id" ]; then
    (
      umask 077
      mkdir -p "$data/contexts"
      [ -e "$data/contexts/$id.json" ] || find "$data/contexts" -name '*.json' -mtime +7 -delete 2>/dev/null
      tmp="$data/contexts/.$id.$$"
      printf '%s' "$payload" >"$tmp" && mv -f "$tmp" "$data/contexts/$id.json"
    )
  fi
  ;;
esac
if [ -s "$data/original.sh" ]; then
  printf '%s' "$payload" | /bin/sh "$data/original.sh"
fi
