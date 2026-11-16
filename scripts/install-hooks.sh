#!/bin/sh
# Installs this repo's git hooks. Worktrees share them. Run once per clone.
set -eu
root=$(git rev-parse --show-toplevel)
hooks=$(git rev-parse --git-common-dir)/hooks
mkdir -p "$hooks"
for hook in reference-transaction pre-push; do
  install -m 755 "$root/scripts/hooks/$hook" "$hooks/$hook"
done
# A global core.hooksPath hides .git/hooks unless that directory has a hook of this name.
global=$(git config --global core.hooksPath || true)
if [ -n "$global" ] && [ -e "$global/_chain" ] && [ ! -e "$global/reference-transaction" ]; then
  ln -s _chain "$global/reference-transaction"
fi
echo "installed reference-transaction and pre-push"
