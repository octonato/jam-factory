#!/usr/bin/env bash
# Copies a review comment to the clipboard and opens the comment form of a PR.
set -euo pipefail

if [ $# -ne 2 ]; then
  echo "usage: open-comment.sh <pr number> <comment file>" >&2
  exit 1
fi

pr="$1"
file="$2"

{ printf ':robot: says...\n\n'; cat "$file"; } | pbcopy

gh pr comment "$pr" --web
