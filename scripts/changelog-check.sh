#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/changelog-check.sh [--base-ref <git-ref>] [--head-ref <git-ref>]
USAGE
}

fail() {
  echo "::error::$*" >&2
  exit 1
}

compare_descending() {
  local previous="$1"
  local current="$2"
  IFS=. read -r prev_major prev_minor prev_patch <<<"$previous"
  IFS=. read -r curr_major curr_minor curr_patch <<<"$current"

  if (( curr_major < prev_major )); then
    return 0
  fi
  if (( curr_major > prev_major )); then
    return 1
  fi
  if (( curr_minor < prev_minor )); then
    return 0
  fi
  if (( curr_minor > prev_minor )); then
    return 1
  fi
  (( curr_patch < prev_patch ))
}

base_ref=""
head_ref="HEAD"

while (($# > 0)); do
  case "$1" in
    --base-ref)
      base_ref="${2:?missing value for --base-ref}"
      shift 2
      ;;
    --head-ref)
      head_ref="${2:?missing value for --head-ref}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      fail "Unknown argument: $1"
      ;;
  esac
done

cd "$(dirname "$0")/.."

if [ ! -f CHANGELOG.md ]; then
  fail "CHANGELOG.md is missing"
fi

if ! grep -qx '## \[Unreleased\]' CHANGELOG.md; then
  fail "CHANGELOG.md must contain a '## [Unreleased]' section"
fi

if [ -n "$base_ref" ]; then
  mapfile -t changed_files < <(git diff --name-only "$base_ref" "$head_ref")
  code_changed=false
  changelog_changed=false

  for path in "${changed_files[@]}"; do
    case "$path" in
      CHANGELOG.md)
        changelog_changed=true
        ;;
      *.rs|Cargo.toml|Cargo.lock|*/Cargo.toml|scripts/*)
        code_changed=true
        ;;
    esac
  done

  if [ "$code_changed" = true ] && [ "$changelog_changed" = false ]; then
    fail "Code changes in Rust/Cargo/scripts must update CHANGELOG.md"
  fi
fi

version_header_count=0
last_version=""
declare -A seen_versions=()

while IFS= read -r line; do
  case "$line" in
    '## [Unreleased]')
      continue
      ;;
    '## ['*)
      if [[ ! "$line" =~ ^##\ \[([0-9]+\.[0-9]+\.[0-9]+)\]\ -\ ([0-9]{4}-[0-9]{2}-[0-9]{2})$ ]]; then
        fail "Malformed changelog header: $line"
      fi

      version="${BASH_REMATCH[1]}"
      date_part="${BASH_REMATCH[2]}"

      if ! date -u -d "$date_part" +%F >/dev/null 2>&1; then
        fail "Invalid ISO 8601 date in changelog header: $line"
      fi
      if [ "$(date -u -d "$date_part" +%F)" != "$date_part" ]; then
        fail "Date must be zero-padded ISO 8601 in changelog header: $line"
      fi
      if [[ -v "seen_versions[$version]" ]]; then
        fail "Duplicate changelog version header: $version"
      fi
      seen_versions["$version"]="$date_part"
      if [ -n "$last_version" ] && ! compare_descending "$last_version" "$version"; then
        fail "Changelog versions must be in descending order: $last_version before $version"
      fi
      last_version="$version"
      version_header_count=$((version_header_count + 1))
      ;;
  esac
done < CHANGELOG.md

if [ "$version_header_count" -eq 0 ]; then
  fail "CHANGELOG.md must contain at least one released version header"
fi

echo "CHANGELOG.md passed validation."
