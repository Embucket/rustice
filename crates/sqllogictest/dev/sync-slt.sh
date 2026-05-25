#!/usr/bin/env bash
# Re-sync the vendored sqllogictest corpus from embucket-labs.
#
#   EMBUCKET_LABS=../../../embucket-labs bash crates/sqllogictest/dev/sync-slt.sh
#
# After running, review `git diff` before committing.
set -euo pipefail

SRC="${EMBUCKET_LABS:-../../../embucket-labs}"
DEST="$(cd "$(dirname "$0")/.." && pwd)/tests/slt"

if [ ! -d "$SRC/test/sql/bronze_scope" ]; then
  echo "error: $SRC/test/sql/bronze_scope not found. Set EMBUCKET_LABS." >&2
  exit 1
fi

mkdir -p "$DEST"
rsync -av --delete "$SRC/test/sql/bronze_scope/" "$DEST/bronze_scope/"
rsync -av --delete "$SRC/test/sql/databend/"     "$DEST/databend/"
echo "Sync complete. Review git diff before committing."
