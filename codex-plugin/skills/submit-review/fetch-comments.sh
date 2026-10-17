#!/usr/bin/env bash
# Prints the comments, review bodies and inline review comments of a PR.
set -euo pipefail

if [ $# -ne 2 ]; then
  echo "usage: fetch-comments.sh <repository dir> <pr number>" >&2
  exit 1
fi

repo_dir="$1"
pr="$2"

cd "$repo_dir"

gh api --paginate "repos/{owner}/{repo}/issues/$pr/comments" \
  --jq '.[] | "## comment by \(.user.login)\n\(.html_url)\n\n\(.body)\n"'

gh api --paginate "repos/{owner}/{repo}/pulls/$pr/reviews" \
  --jq '.[] | select(.body != "") | "## review by \(.user.login) (\(.state))\n\(.html_url)\n\n\(.body)\n"'

gh api --paginate "repos/{owner}/{repo}/pulls/$pr/comments" \
  --jq '.[] | "## inline comment by \(.user.login) on \(.path):\(.line // .original_line)\n\(.html_url)\n\n\(.body)\n"'
