#!/usr/bin/env bash
# Regenerate the snowplow setup fixtures.
#
# Two setup files are emitted, one per simulated dbt run:
#
#   setup.full_refresh.slt = header (events1 loaded) + Phase A + Phase B
#   setup.slt              = header (events1 loaded) + Phase A + events2 COPY + Phase B
#
# Phase A = run-1 compiled SQL, sentinel-gated: every model lays down its
# schema with 0 rows (`CREATE TABLE <model> AS <full-refresh SQL>`).
#
# Phase B = warm-run compiled SQL: per-model `CREATE OR REPLACE TABLE` for
# `+materialized: table` scratch; `CREATE OR REPLACE __dbt_tmp` + verbatim
# `MERGE INTO` for `+materialized: incremental` derived. The MERGE statements
# are sourced from `target/run/snowplow_web/models/.../<model>.sql` and
# rewritten so they reference the embucket catalog with lowercase identifiers
# (the live dbt project targets a Snowflake account named SNOWPLOW_JAN).
#
# Verification state mapping:
#
#   * full_refresh → state after `load events1.csv; dbt run`: scratch and
#                    derived populated with events1 data. Validated by the
#                    18 leaves under full_refresh/ against captured
#                    Snowflake values in slt_results.full_refresh.txt.
#   * incremental  → state after `load events2.csv; dbt run` (on top of
#                    full_refresh state): scratch and derived populated
#                    with events1+events2 data. Validated by the 18
#                    leaves under incremental/ against
#                    slt_results.incremental.txt.
#
# Both setup files run Phase A + single Phase B. The Phase A CTAS lays
# down schemas with 0 rows (sentinel-gated), then Phase B's CREATE OR
# REPLACE rebuilds scratch from whatever the events table holds, and
# Phase B's MERGE upserts derived rows into Phase A's empty target.
# A literal two-cycle simulation (cycle 1 populates derived with events1
# via MERGE on empty target; cycle 2 re-MERGEs events1+events2 into the
# populated target) would produce duplicate derived rows because the
# dbt-compiled MERGE predicate is a 2-minute window baked in at compile
# time, so the second MERGE WHEN-NOT-MATCHes every row.
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
SETUP_FR_OUT="${HERE}/tests/fixtures/snowplow/setup.full_refresh.slt"
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

# Helper: emit Phase A block for one model — only the full-refresh CTAS,
# nothing else. Real dbt's run-1 compiled SQL is a single
# `create or replace table foo as <SELECT>` per model; the snowplow
# package's full-refresh SELECT is sentinel-gated so the result is empty.
emit_phase_a() {
  local schema_suffix="$1" name="$2" full_rel="$3" _inc_rel="$4" _merge_rel="$5"
  local target="embucket.public_snowplow_manifest_${schema_suffix}.${name}"
  echo "# ${name}"
  echo "# Run 1 (cold start): CREATE TABLE AS <full-refresh SQL> (sentinel-gated → 0 rows)."
  echo "#   ← ${full_rel}"
  echo "statement ok"
  echo "CREATE TABLE ${target} AS"
  grep -v '^[[:space:]]*$' "${SRC_ROOT}/${full_rel}"
  echo ";"
  echo
}

# Helper: emit Phase B block for one model — the warm-run compiled SQL.
# CREATE OR REPLACE TABLE is used (matches dbt's
# `create or replace transient table`) so no explicit DROP is needed
# whether or not the table is left over from Phase A.
emit_phase_b() {
  local schema_suffix="$1" name="$2" _full_rel="$3" inc_rel="$4" merge_rel="$5"
  local target="embucket.public_snowplow_manifest_${schema_suffix}.${name}"
  echo "# ${name}"
  if [[ -n "${merge_rel}" ]]; then
    local tmp_target="embucket.public_snowplow_manifest_derived.${name}__dbt_tmp"
    echo "# Run 2: build the dbt temp source, then MERGE INTO the (empty-from-run-1) persistent table."
    echo "#   ← ${inc_rel}"
    echo "statement ok"
    echo "CREATE OR REPLACE TABLE ${tmp_target} AS"
    grep -v '^[[:space:]]*$' "${SRC_ROOT}/${inc_rel}"
    echo ";"
    echo
    echo "#   ← target/run/.../${merge_rel}"
    echo "statement ok"
    awk 'tolower($0) ~ /merge into/ {flag=1} flag' "${RUN_ROOT}/${merge_rel}" \
      | grep -v '^[[:space:]]*$' \
      | sed -E 's/SNOWPLOW_JAN\.snowplow_manifest_/embucket.public_snowplow_manifest_/g' \
      | sed -E 's/"([A-Z][A-Z0-9_]*)"/\L\1/g'
    echo ";"
  else
    echo "# Run 2: rebuild the scratch table (+materialized: table)."
    echo "#   ← ${inc_rel}"
    echo "statement ok"
    echo "CREATE OR REPLACE TABLE ${target} AS"
    grep -v '^[[:space:]]*$' "${SRC_ROOT}/${inc_rel}"
    echo ";"
  fi
  echo
}

# Phase A loop (cold-start CTAS for every model): assembled once,
# reused across all three setup files.
PHASE_A_TMP="$(mktemp)"
PHASE_B_TMP="$(mktemp)"
trap 'rm -f "${PHASE_A_TMP}" "${PHASE_B_TMP}"' EXIT

{
  cat "${HEADER_PARTIAL}"
  echo
  echo '# ---------------------------------------------------------------------------'
  echo '# Phase A — dbt run #1, cold start. events1.parquet is already loaded (see'
  echo '# header). Each model is `CREATE TABLE <model> AS <full-refresh SQL>`; the'
  echo '# package gates the upstream events with 9999-01-01 sentinels so every model'
  echo '# lays down its schema with zero rows. Matches dbt run-1 compiled SQL verbatim'
  echo '# — no INSERT, no __dbt_tmp, no MERGE. Generated by dev/regen-snowplow-setup.sh.'
  echo '# ---------------------------------------------------------------------------'
  echo
  for entry in "${DAG[@]}"; do
    IFS=':' read -r schema_suffix name full_rel inc_rel merge_rel <<< "${entry}"
    emit_phase_a "${schema_suffix}" "${name}" "${full_rel}" "${inc_rel}" "${merge_rel:-}"
  done
} > "${PHASE_A_TMP}"

# Phase B loop (warm-run CREATE OR REPLACE + MERGE for every model):
# assembled once, reused for the second and third setup files. Whatever
# the events table contains at the moment Phase B runs is what scratch
# and derived tables get populated from.
{
  for entry in "${DAG[@]}"; do
    IFS=':' read -r schema_suffix name full_rel inc_rel merge_rel <<< "${entry}"
    emit_phase_b "${schema_suffix}" "${name}" "${full_rel}" "${inc_rel}" "${merge_rel:-}"
  done
} > "${PHASE_B_TMP}"

# setup.full_refresh.slt: header + Phase A + Phase B.
#
# Reproduces the state after dbt run #2 with events1 alone loaded: the
# warm pass populates every scratch table via CREATE OR REPLACE on the
# events1-only events table, and populates every derived table via
# CREATE OR REPLACE __dbt_tmp + MERGE INTO an empty target (so every
# source row goes through WHEN NOT MATCHED → INSERT exactly once).
# Used by full_refresh/snowplow_web_*.slt leaves to validate the
# events1-only verified-against-Snowflake reference values.
{
  cat "${PHASE_A_TMP}"
  echo '# ---------------------------------------------------------------------------'
  echo '# Phase B — dbt run #2, warm. The events table still holds events1 only; the'
  echo '# warm-run incremental SQL rebuilds every scratch via CREATE OR REPLACE TABLE'
  echo '# and upserts every derived via __dbt_tmp + MERGE on the (empty) target.'
  echo '# ---------------------------------------------------------------------------'
  echo
  cat "${PHASE_B_TMP}"
} > "${SETUP_FR_OUT}"

# setup.slt: header + Phase A + COPY events2 + Phase B (single pass).
#
# Reproduces the state of every model after dbt run #2 where the events
# table holds events1+events2: scratch tables are CREATE OR REPLACE'd
# against the combined data; derived tables MERGE the combined source
# into Phase A's empty target so every row goes through WHEN NOT MATCHED
# → INSERT exactly once. Used by incremental/snowplow_web_*.slt leaves
# to validate the events1+events2 verified-against-Snowflake values.
#
# Note: a faithful "3-cycle" alternative would re-MERGE the events1
# state from setup.full_refresh.slt with the events1+events2 source.
# The dbt-compiled MERGE predicate window is too narrow to match the
# rows already in the target (it's a 2-minute slice baked in at compile
# time), so a second MERGE pass would WHEN-NOT-MATCH every row and
# create duplicates. Single-pass on an empty target is the only way
# to reproduce the captured Snowflake values without depending on a
# compile-time-aligned window.
{
  cat "${PHASE_A_TMP}"
  echo
  echo '# ---------------------------------------------------------------------------'
  echo '# Append events2.parquet so the events table holds both batches before'
  echo '# Phase B runs. This simulates a loader tick between dbt runs #1 and #2.'
  echo '# ---------------------------------------------------------------------------'
  echo
  echo 'control substitution on'
  echo
  echo 'statement ok'
  echo 'COPY INTO embucket.public_snowplow_manifest.events'
  echo "FROM 'file://\${CRATE_ROOT}/tests/fixtures/snowplow/events2.parquet'"
  echo "FILE_FORMAT = ( TYPE = 'PARQUET' );"
  echo
  echo 'control substitution off'
  echo
  echo '# ---------------------------------------------------------------------------'
  echo '# Phase B — dbt run #2, warm with events1+events2. Scratch tables build from'
  echo '# the combined events table; derived tables MERGE into Phase A''s empty'
  echo '# target, so every source row goes through WHEN NOT MATCHED → INSERT.'
  echo '# ---------------------------------------------------------------------------'
  echo
  cat "${PHASE_B_TMP}"
} > "${SETUP_OUT}"

echo "Regenerated ${SETUP_FR_OUT}, ${SETUP_OUT}"
echo "  (${#DAG[@]} models; ${merge_count} with dbt-MERGE, $(( ${#DAG[@]} - merge_count )) as CREATE OR REPLACE)"
