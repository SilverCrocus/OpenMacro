#!/usr/bin/env bash
# How many people are finding OpenMacro: APK downloads per release, stars,
# and GitHub's last-14-days traffic (views and where visitors came from).
# Needs the GitHub CLI (gh), signed in with push access to the repository.
#
# Downloads count every download of an APK, including in-app updates by
# existing users, so they're downloads, not people. Traffic covers only the
# last 14 days and runs a day or so behind.
set -euo pipefail
repo="${1:-SilverCrocus/OpenMacro}"

gh api "repos/$repo" --jq '"Stars: \(.stargazers_count)   Forks: \(.forks_count)   Watching: \(.subscribers_count)"'

echo
echo "APK downloads by release"
gh api "repos/$repo/releases" --paginate --jq '.[] |
  "  \(.tag_name)\t" + ([.assets[] | select(.name | endswith(".apk")) | "\(.name | sub("openmacro-[0-9.]+-"; "") | sub(".apk"; "")): \(.download_count)"] | join("   "))'
gh api "repos/$repo/releases" --paginate --jq '"  Total: \([.[].assets[] | select(.name | endswith(".apk")) | .download_count] | add // 0)"'

echo
gh api "repos/$repo/traffic/views" --jq '"Visitors, last 14 days: \(.uniques) people, \(.count) views"'
echo "Where they came from"
gh api "repos/$repo/traffic/popular/referrers" --jq 'if length == 0 then "  (nothing yet)" else .[] | "  \(.referrer): \(.uniques) people, \(.count) views" end'
echo "Most viewed pages"
gh api "repos/$repo/traffic/popular/paths" --jq 'if length == 0 then "  (nothing yet)" else .[:5][] | "  \(.path): \(.uniques) people" end'
