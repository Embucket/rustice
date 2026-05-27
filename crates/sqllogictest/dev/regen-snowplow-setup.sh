#!/usr/bin/env bash
# Regenerate tests/fixtures/snowplow/setup.slt.
#
# The setup materialises the dbt-snowplow-web DAG mirroring the production
# dbt flow on a cold-then-incremental cycle:
#
#   1. Step A (every model): `CREATE TABLE schema.model AS <full-refresh SELECT>`
#      lays down the schema (sentinel 9999 timestamps mean the table is
#      typically empty after this step — i.e. cold start.)
#
#   2. Step B differs by canonical materialisation:
#      - `+materialized: incremental` (the 4 derived models): build a temp
#        source `<model>__dbt_tmp` from the incremental SELECT, then emit the
#        verbatim MERGE INTO statement that dbt-snowflake's incremental
#        materialisation writes to target/run/.../<model>.sql. The MERGE has
#        the full enumerated column list and the unique_key predicate.
#      - All other models: `INSERT INTO schema.model <incremental SELECT>`
#        (matches dbt's `table` materialisation rebuilding from scratch).
#
# The MERGE statements are sourced from a sibling dbt project that has
# already had `dbt run --select <model>` invoked at least once against a live
# embucket — that writes target/run/snowplow_web/models/.../<model>.sql.
# Override DBT_RUN_DIR / DBT_QUERIES_DIR to point at a different checkout.
#
# Leaf .slt files under tests/slt/dbt_snowplow_web/ then re-run their query as
# `statement ok` against the populated upstream tables.

set -euo pipefail

DBT_PROJECT="${DBT_PROJECT:-/home/work/workspace/github/test-dbt-snowplow-web}"
SRC_ROOT="${DBT_QUERIES_DIR:-${DBT_PROJECT}/queries}"
RUN_ROOT="${DBT_RUN_DIR:-${DBT_PROJECT}/target/run/snowplow_web/models}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"
SETUP_OUT="${HERE}/tests/fixtures/snowplow/setup.slt"
HEADER_PARTIAL="${HERE}/tests/fixtures/snowplow/setup.header.slt"

if [[ ! -d "${SRC_ROOT}" ]]; then
  echo "Source not found: ${SRC_ROOT}" >&2
  exit 1
fi
if [[ ! -f "${HEADER_PARTIAL}" ]]; then
  echo "Missing header partial: ${HEADER_PARTIAL}" >&2
  exit 1
fi

# DAG order: each line is
#   "<schema_suffix>:<model_name>:<full_refresh_rel>:<incremental_rel>[:<merge_run_rel>]"
# Schema suffix follows dbt_project.yml: scratch -> _scratch,
# manifest -> _snowplow_manifest, derived/passthrough -> _derived.
# A 5th field <merge_run_rel> opts the model into the MERGE flow: instead of
# `INSERT INTO`, the script materialises a `<model>__dbt_tmp` table from the
# incremental SELECT and then emits the verbatim MERGE statement extracted
# from target/run/.../<merge_run_rel> (dbt's incremental materialisation
# output). These are the 4 canonical `+materialized: incremental` models in
# snowplow_web.
DAG=(
  "snowplow_manifest:snowplow_web_incremental_manifest:full-refresh/base/manifest/snowplow_web_incremental_manifest.sql:incremental/base/manifest/snowplow_web_incremental_manifest.sql"
  "snowplow_manifest:snowplow_web_base_quarantined_sessions:full-refresh/base/manifest/snowplow_web_base_quarantined_sessions.sql:incremental/base/manifest/snowplow_web_base_quarantined_sessions.sql"
  "scratch:snowplow_web_base_new_event_limits:full-refresh/base/scratch/snowplow_web_base_new_event_limits.sql:incremental/base/scratch/snowplow_web_base_new_event_limits.sql"
  "snowplow_manifest:snowplow_web_base_sessions_lifecycle_manifest:full-refresh/base/manifest/snowplow_web_base_sessions_lifecycle_manifest.sql:incremental/base/manifest/snowplow_web_base_sessions_lifecycle_manifest.sql"
  "scratch:snowplow_web_base_sessions_this_run:full-refresh/base/scratch/snowplow_web_base_sessions_this_run.sql:incremental/base/scratch/snowplow_web_base_sessions_this_run.sql"
  "scratch:snowplow_web_base_events_this_run:full-refresh/base/scratch/snowflake/snowplow_web_base_events_this_run.sql:incremental/base/scratch/snowflake/snowplow_web_base_events_this_run.sql"
  "scratch:snowplow_web_pv_engaged_time:full-refresh/page_views/scratch/snowplow_web_pv_engaged_time.sql:incremental/page_views/scratch/snowplow_web_pv_engaged_time.sql"
  "scratch:snowplow_web_pv_scroll_depth:full-refresh/page_views/scratch/snowplow_web_pv_scroll_depth.sql:incremental/page_views/scratch/snowplow_web_pv_scroll_depth.sql"
  "derived:snowplow_web_user_mapping:full-refresh/user_mapping/snowplow_web_user_mapping.sql:incremental/user_mapping/snowplow_web_user_mapping.sql:user_mapping/snowplow_web_user_mapping.sql"
  "scratch:snowplow_web_sessions_this_run:full-refresh/sessions/scratch/snowflake/snowplow_web_sessions_this_run.sql:incremental/sessions/scratch/snowflake/snowplow_web_sessions_this_run.sql"
  "scratch:snowplow_web_page_views_this_run:full-refresh/page_views/scratch/snowflake/snowplow_web_page_views_this_run.sql:incremental/page_views/scratch/snowflake/snowplow_web_page_views_this_run.sql"
  "derived:snowplow_web_sessions:full-refresh/sessions/snowplow_web_sessions.sql:incremental/sessions/snowplow_web_sessions.sql:sessions/snowplow_web_sessions.sql"
  "scratch:snowplow_web_users_sessions_this_run:full-refresh/users/scratch/snowplow_web_users_sessions_this_run.sql:incremental/users/scratch/snowplow_web_users_sessions_this_run.sql"
  "scratch:snowplow_web_users_aggs:full-refresh/users/scratch/snowplow_web_users_aggs.sql:incremental/users/scratch/snowplow_web_users_aggs.sql"
  "scratch:snowplow_web_users_lasts:full-refresh/users/scratch/snowplow_web_users_lasts.sql:incremental/users/scratch/snowplow_web_users_lasts.sql"
  "scratch:snowplow_web_users_this_run:full-refresh/users/scratch/snowplow_web_users_this_run.sql:incremental/users/scratch/snowplow_web_users_this_run.sql"
  "derived:snowplow_web_page_views:full-refresh/page_views/snowplow_web_page_views.sql:incremental/page_views/snowplow_web_page_views.sql:page_views/snowplow_web_page_views.sql"
  "derived:snowplow_web_users:full-refresh/users/snowplow_web_users.sql:incremental/users/snowplow_web_users.sql:users/snowplow_web_users.sql"
)

{
  cat "${HEADER_PARTIAL}"
  echo
  echo '# ---------------------------------------------------------------------------'
  echo '# DAG materialisation: for each model, run its full-refresh SQL as'
  echo '# CREATE TABLE AS to lay down the schema. Then either INSERT INTO (for'
  echo '# `+materialized: table` models) or materialise a `<model>__dbt_tmp`'
  echo '# scratch table from the incremental SELECT and run the verbatim MERGE'
  echo '# extracted from dbt-snowflake target/run/.../<model>.sql (for the 4'
  echo '# canonical `+materialized: incremental` derived models). Mirrors how'
  echo '# dbt-snowplow-web boots a cold warehouse and then runs an incremental'
  echo '# cycle. Generated by dev/regen-snowplow-setup.sh.'
  echo '# ---------------------------------------------------------------------------'
  echo
  merge_count=0
  for entry in "${DAG[@]}"; do
    IFS=':' read -r schema_suffix name full_rel inc_rel merge_rel <<< "${entry}"
    full_path="${SRC_ROOT}/${full_rel}"
    inc_path="${SRC_ROOT}/${inc_rel}"
    if [[ ! -f "${full_path}" ]]; then
      echo "Source not found: ${full_path}" >&2
      exit 1
    fi
    if [[ ! -f "${inc_path}" ]]; then
      echo "Source not found: ${inc_path}" >&2
      exit 1
    fi
    target_schema="embucket.public_snowplow_manifest_${schema_suffix}"
    target="${target_schema}.${name}"
    echo "# ${name}"
    echo "# Full-refresh CTAS (creates table with correct schema)"
    echo "#   ← ${full_rel}"
    echo "statement ok"
    echo "CREATE TABLE ${target} AS"
    grep -v '^[[:space:]]*$' "${full_path}"
    echo ";"
    echo
    if [[ -n "${merge_rel}" ]]; then
      merge_path="${RUN_ROOT}/${merge_rel}"
      if [[ ! -f "${merge_path}" ]]; then
        echo "MERGE source not found: ${merge_path}" >&2
        echo "  Run \`dbt run --select <model>\` in ${DBT_PROJECT} first; even a" >&2
        echo "  failed run writes target/run/.../<model>.sql with the MERGE." >&2
        exit 1
      fi
      if ! grep -qi 'merge into' "${merge_path}"; then
        echo "Expected MERGE INTO in ${merge_path} but found none." >&2
        echo "  Was \`+materialized: table\` overridden? Or did dbt skip the model?" >&2
        exit 1
      fi
      merge_count=$((merge_count+1))
      # dbt's incremental materialisation references a `<model>__dbt_tmp`
      # source. Materialise it from the incremental SELECT so the MERGE has a
      # real source to read from.
      tmp_target="embucket.public_snowplow_manifest_derived.${name}__dbt_tmp"
      echo "# Incremental upsert phase 1 of 2: build the dbt temp source table."
      echo "#   ← ${inc_rel}"
      echo "statement ok"
      echo "CREATE TABLE ${tmp_target} AS"
      grep -v '^[[:space:]]*$' "${inc_path}"
      echo ";"
      echo
      echo "# Incremental upsert phase 2 of 2: MERGE INTO the persistent table,"
      echo "# verbatim from dbt's incremental materialisation output."
      echo "#   ← target/run/.../${merge_rel}"
      echo "statement ok"
      # Slice from the first `merge into` line through end-of-file, strip
      # blank lines so sqllogictest doesn't terminate the record early, then
      # append a trailing semicolon.
      awk 'tolower($0) ~ /merge into/ {flag=1} flag' "${merge_path}" \
        | grep -v '^[[:space:]]*$'
      echo ";"
    else
      echo "# Incremental load (populates with real-timestamp data)"
      echo "#   ← ${inc_rel}"
      echo "statement ok"
      echo "INSERT INTO ${target}"
      grep -v '^[[:space:]]*$' "${inc_path}"
      echo ";"
    fi
    echo
  done
} > "${SETUP_OUT}"

echo "Regenerated ${SETUP_OUT} (${#DAG[@]} models, ${merge_count} with dbt-MERGE, $(( ${#DAG[@]} - merge_count )) as INSERT INTO)"
