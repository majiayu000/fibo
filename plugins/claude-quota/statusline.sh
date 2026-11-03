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
if [ -s "$data/original.sh" ]; then
  printf '%s' "$payload" | /bin/sh "$data/original.sh"
fi
