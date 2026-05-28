#!/usr/bin/env bash
# Regenerate tests/fixtures/snowplow/setup.slt.
#
# Simulates a two-batch dbt-snowplow-web cycle:
#
#   Phase A (cold start, events1.csv loaded by setup.header.slt):
#     For each of the 18 models:
#       1. `CREATE TABLE <model> AS <full-refresh SQL>` — lays down the
#          schema. The full-refresh SQL has 9999-01-01 sentinels, so the
#          table is empty after this step (matches dbt full-refresh).
#       2a. `+materialized: incremental` (4 derived models):
#           `CREATE TABLE <model>__dbt_tmp AS <incremental SQL>` then the
#           verbatim `MERGE INTO` from target/run/.../<model>.sql.
#       2b. All other models (`+materialized: table`):
#           `INSERT INTO <model> <incremental SQL>` populates the table.
#
#   Phase B (incremental refresh, events2.csv appended):
#     1. `COPY INTO enriched_raw FROM events2.csv` — append the second
#        batch. enriched_raw now holds 400 rows.
#     2. `DROP TABLE events; CREATE TABLE events AS <typed CTAS extracted
#        from setup.header.slt>` — rebuild the typed events table so the
#        DAG sees both batches.
#     3. For each of the 18 models, re-run the incremental SQL:
#        - `+materialized: incremental`: `DROP TABLE __dbt_tmp;
#          CREATE TABLE __dbt_tmp AS <incremental SQL>; <MERGE>;`
#          (MERGE upserts the new rows into the existing destination.)
#        - `+materialized: table`: `DROP TABLE <model>;
#          CREATE TABLE <model> AS <incremental SQL>;`
#          (matches dbt's per-run rebuild of `_this_run` scratch tables.)
#
# The MERGE statements are sourced from a sibling dbt project that has
# already had `dbt run --select <model>` invoked at least once against a
# live embucket — that writes target/run/snowplow_web/models/.../<model>.sql.
# Override DBT_RUN_DIR / DBT_QUERIES_DIR to point at a different checkout.
#
# Leaf .slt files under tests/slt/dbt_snowplow_web/ then run their
# verification queries against the populated upstream tables.

set -euo pipefail

HERE="$(cd "$(dirname "$0")/.." && pwd)"
# Default to a sibling checkout of test-dbt-snowplow-web. Override with
# DBT_PROJECT (or DBT_QUERIES_DIR / DBT_RUN_DIR individually) for
# non-default layouts.
DBT_PROJECT="${DBT_PROJECT:-${HERE}/../../../test-dbt-snowplow-web}"
SRC_ROOT="${DBT_QUERIES_DIR:-${DBT_PROJECT}/queries}"
RUN_ROOT="${DBT_RUN_DIR:-${DBT_PROJECT}/target/run/snowplow_web/models}"
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
# A 5th field <merge_run_rel> opts the model into the MERGE flow.
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

# Pre-flight: every model's source files must exist before we emit anything.
merge_count=0
for entry in "${DAG[@]}"; do
  IFS=':' read -r _schema name full_rel inc_rel merge_rel <<< "${entry}"
  [[ -f "${SRC_ROOT}/${full_rel}" ]] || { echo "Source not found: ${SRC_ROOT}/${full_rel}" >&2; exit 1; }
  [[ -f "${SRC_ROOT}/${inc_rel}" ]]  || { echo "Source not found: ${SRC_ROOT}/${inc_rel}" >&2; exit 1; }
  if [[ -n "${merge_rel:-}" ]]; then
    if [[ ! -f "${RUN_ROOT}/${merge_rel}" ]]; then
      echo "MERGE source not found: ${RUN_ROOT}/${merge_rel}" >&2
      echo "  Run \`dbt run --select ${name}\` in ${DBT_PROJECT} first; even a" >&2
      echo "  failed run writes target/run/.../<model>.sql with the MERGE." >&2
      exit 1
    fi
    if ! grep -qi 'merge into' "${RUN_ROOT}/${merge_rel}"; then
      echo "Expected MERGE INTO in ${RUN_ROOT}/${merge_rel} but found none." >&2
      exit 1
    fi
    merge_count=$((merge_count+1))
  fi
done

# Helper: emit Phase A block for one model (CTAS + INSERT or __dbt_tmp+MERGE).
emit_phase_a() {
  local schema_suffix="$1" name="$2" full_rel="$3" inc_rel="$4" merge_rel="$5"
  local target="embucket.public_snowplow_manifest_${schema_suffix}.${name}"
  echo "# ${name}"
  echo "# Full-refresh CTAS (creates table with correct schema)"
  echo "#   ← ${full_rel}"
  echo "statement ok"
  echo "CREATE TABLE ${target} AS"
  grep -v '^[[:space:]]*$' "${SRC_ROOT}/${full_rel}"
  echo ";"
  echo
  if [[ -n "${merge_rel}" ]]; then
    local tmp_target="embucket.public_snowplow_manifest_derived.${name}__dbt_tmp"
    echo "# Incremental upsert phase 1 of 2: build the dbt temp source table."
    echo "#   ← ${inc_rel}"
    echo "statement ok"
    echo "CREATE TABLE ${tmp_target} AS"
    grep -v '^[[:space:]]*$' "${SRC_ROOT}/${inc_rel}"
    echo ";"
    echo
    echo "# Incremental upsert phase 2 of 2: MERGE INTO the persistent table,"
    echo "# verbatim from dbt's incremental materialisation output."
    echo "#   ← target/run/.../${merge_rel}"
    echo "statement ok"
    awk 'tolower($0) ~ /merge into/ {flag=1} flag' "${RUN_ROOT}/${merge_rel}" \
      | grep -v '^[[:space:]]*$'
    echo ";"
  else
    echo "# Incremental load (populates with real-timestamp data)"
    echo "#   ← ${inc_rel}"
    echo "statement ok"
    echo "INSERT INTO ${target}"
    grep -v '^[[:space:]]*$' "${SRC_ROOT}/${inc_rel}"
    echo ";"
  fi
  echo
}

# Helper: emit Phase B block for one model. Re-runs incremental SQL after
# events2 has been appended. Table-mat models DROP+CREATE; incremental-mat
# models DROP __dbt_tmp + CREATE __dbt_tmp + MERGE.
emit_phase_b() {
  local schema_suffix="$1" name="$2" _full_rel="$3" inc_rel="$4" merge_rel="$5"
  local target="embucket.public_snowplow_manifest_${schema_suffix}.${name}"
  echo "# ${name}"
  if [[ -n "${merge_rel}" ]]; then
    local tmp_target="embucket.public_snowplow_manifest_derived.${name}__dbt_tmp"
    echo "# Phase B: rebuild dbt temp source then re-MERGE into persistent table."
    echo "#   ← ${inc_rel}"
    echo "statement ok"
    echo "DROP TABLE ${tmp_target};"
    echo
    echo "statement ok"
    echo "CREATE TABLE ${tmp_target} AS"
    grep -v '^[[:space:]]*$' "${SRC_ROOT}/${inc_rel}"
    echo ";"
    echo
    echo "#   ← target/run/.../${merge_rel}"
    echo "statement ok"
    awk 'tolower($0) ~ /merge into/ {flag=1} flag' "${RUN_ROOT}/${merge_rel}" \
      | grep -v '^[[:space:]]*$'
    echo ";"
  else
    echo "# Phase B: drop and rebuild scratch table (matches dbt +materialized: table)."
    echo "#   ← ${inc_rel}"
    echo "statement ok"
    echo "DROP TABLE ${target};"
    echo
    echo "statement ok"
    echo "CREATE TABLE ${target} AS"
    grep -v '^[[:space:]]*$' "${SRC_ROOT}/${inc_rel}"
    echo ";"
  fi
  echo
}

{
  cat "${HEADER_PARTIAL}"
  echo
  echo '# ---------------------------------------------------------------------------'
  echo '# Phase A — cold start: events1.csv is already loaded (see header). For each'
  echo '# model run full-refresh CTAS (lays down schema, empty due to sentinels), then'
  echo '# either INSERT INTO (`+materialized: table`) or build __dbt_tmp + MERGE INTO'
  echo '# (`+materialized: incremental`). Generated by dev/regen-snowplow-setup.sh.'
  echo '# ---------------------------------------------------------------------------'
  echo
  for entry in "${DAG[@]}"; do
    IFS=':' read -r schema_suffix name full_rel inc_rel merge_rel <<< "${entry}"
    emit_phase_a "${schema_suffix}" "${name}" "${full_rel}" "${inc_rel}" "${merge_rel:-}"
  done

  echo '# ---------------------------------------------------------------------------'
  echo '# Phase B — incremental: append events2.csv to enriched_raw, rebuild the'
  echo '# typed events table, then re-run the per-model incremental SQL. Scratch'
  echo '# (`+materialized: table`) models are DROP+CREATEd; derived'
  echo '# (`+materialized: incremental`) models build a fresh __dbt_tmp and MERGE'
  echo '# into the persistent table (upserts new rows / updates existing).'
  echo '# ---------------------------------------------------------------------------'
  echo
  echo 'control substitution on'
  echo
  echo '# Append events2 directly to the events table. The parquet file is'
  echo '# already in the typed events shape (produced upstream by the'
  echo '# snowplow-events-parquet pipeline), so no staging / CTAS needed.'
  echo 'statement ok'
  echo 'COPY INTO embucket.public_snowplow_manifest.events'
  echo "FROM 'file://\${CRATE_ROOT}/tests/fixtures/snowplow/events2.parquet'"
  echo "FILE_FORMAT = ( TYPE = 'PARQUET' );"
  echo
  echo 'control substitution off'
  echo
  for entry in "${DAG[@]}"; do
    IFS=':' read -r schema_suffix name full_rel inc_rel merge_rel <<< "${entry}"
    emit_phase_b "${schema_suffix}" "${name}" "${full_rel}" "${inc_rel}" "${merge_rel:-}"
  done
} > "${SETUP_OUT}"

echo "Regenerated ${SETUP_OUT} (${#DAG[@]} models per phase × 2 phases, ${merge_count} with dbt-MERGE, $(( ${#DAG[@]} - merge_count )) as INSERT/DROP+CREATE)"
