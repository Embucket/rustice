#!/usr/bin/env bash
# Regenerate the 36 dbt-snowplow-web .slt files from the dbt-compiled SQL at
# ../test-dbt-snowplow-web/queries (override with DBT_QUERIES_DIR).
#
# Each generated .slt wraps the verbatim dbt SQL in `statement ok`, prefixed
# by a relative `include ../../../fixtures/snowplow/setup.slt` directive
# (resolved against the including file's directory by the harness).

set -euo pipefail

SRC_ROOT="${DBT_QUERIES_DIR:-/home/work/workspace/github/test-dbt-snowplow-web/queries}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"
DST_ROOT="${HERE}/tests/slt/dbt_snowplow_web"

if [[ ! -d "${SRC_ROOT}" ]]; then
  echo "Source not found: ${SRC_ROOT}" >&2
  exit 1
fi

mkdir -p "${DST_ROOT}/incremental" "${DST_ROOT}/full_refresh"

count=0
for src_mode_dir in "${SRC_ROOT}/incremental" "${SRC_ROOT}/full-refresh"; do
  [[ -d "${src_mode_dir}" ]] || continue
  src_mode=$(basename "${src_mode_dir}")
  dst_mode="${src_mode//-/_}"
  while IFS= read -r src_sql; do
    name=$(basename "${src_sql}" .sql)
    dst_slt="${DST_ROOT}/${dst_mode}/${name}.slt"
    rel_src="${src_sql#"${SRC_ROOT}/"}"
    {
      echo "# dbt-snowplow-web: ${dst_mode}/${name}"
      echo "# Source: test-dbt-snowplow-web/queries/${rel_src}"
      echo
      echo 'include ../../../fixtures/snowplow/setup.slt'
      echo
      echo 'statement ok'
      # Blank lines terminate sqllogictest records, so strip them from the
      # SQL body. The verbatim shape of the SQL is preserved otherwise.
      grep -v '^[[:space:]]*$' "${src_sql}"
    } > "${dst_slt}"
    count=$((count+1))
  done < <(find "${src_mode_dir}" -name "*.sql" -type f | sort)
done

echo "Generated ${count} .slt files under ${DST_ROOT}"
