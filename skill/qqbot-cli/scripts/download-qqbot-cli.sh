#!/usr/bin/env bash
set -euo pipefail

SKILL_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION='v0.0.0'

case "$(uname -s)/$(uname -m)" in
  Linux/x86_64) ASSET='qqbot-cli-ubuntu-x86_64' ;;
  Darwin/arm64) ASSET='qqbot-cli-macos-aarch64' ;;
  *) printf 'Unsupported OS/architecture: %s/%s\n' "$(uname -s)" "$(uname -m)" >&2; exit 1 ;;
esac

QQ="$SKILL_DIR/bin/qqbot-cli"
mkdir -p "$SKILL_DIR/bin"
TMP="$(mktemp "$SKILL_DIR/bin/.qqbot-cli-download.XXXXXX")"
trap 'rm -f "$TMP"' EXIT

if ! curl --fail --location --show-error --silent \
  "https://github.com/y0sh1ne/qqbot-cli/releases/download/$VERSION/$ASSET" \
  --output "$TMP"; then
  printf 'Failed to download %s from release %s. Verify that this release and asset exist.\n' "$ASSET" "$VERSION" >&2
  exit 1
fi
if [[ ! -s "$TMP" ]]; then
  printf 'Downloaded executable is empty.\n' >&2
  exit 1
fi

chmod +x "$TMP"
mv -f "$TMP" "$QQ"
"$QQ" --help
