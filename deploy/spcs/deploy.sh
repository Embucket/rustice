#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

SNOW_BIN="${SNOW_BIN:-snow}"
SNOW_CONNECTION="${SNOW_CONNECTION:-default}"
SNOWFLAKE_HOME="${SNOWFLAKE_HOME:-${TMPDIR:-/tmp}/rustice-snowflake-home}"
XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-${SNOWFLAKE_HOME}/xdg}"

RUSTICE_DB="${RUSTICE_DB:-RUSTICE_APP}"
RUSTICE_SCHEMA="${RUSTICE_SCHEMA:-PUBLIC}"
RUSTICE_COMPUTE_POOL="${RUSTICE_COMPUTE_POOL:-RUSTICE_POOL}"
RUSTICE_IMAGE_REPOSITORY="${RUSTICE_IMAGE_REPOSITORY:-RUSTICE_REPO}"
RUSTICE_SERVICE="${RUSTICE_SERVICE:-RUSTICE_SERVICE}"
RUSTICE_CONTAINER_NAME="${RUSTICE_CONTAINER_NAME:-rustice}"
RUSTICE_ENDPOINT_NAME="${RUSTICE_ENDPOINT_NAME:-main}"
RUSTICE_SERVICE_ROLE="${RUSTICE_SERVICE_ROLE:-rustice_user}"
RUSTICE_INSTANCE_FAMILY="${RUSTICE_INSTANCE_FAMILY:-CPU_X64_XS}"
RUSTICE_POOL_MIN_NODES="${RUSTICE_POOL_MIN_NODES:-1}"
RUSTICE_POOL_MAX_NODES="${RUSTICE_POOL_MAX_NODES:-1}"
RUSTICE_MIN_INSTANCES="${RUSTICE_MIN_INSTANCES:-1}"
RUSTICE_MAX_INSTANCES="${RUSTICE_MAX_INSTANCES:-1}"
RUSTICE_AUTO_SUSPEND_SECS="${RUSTICE_AUTO_SUSPEND_SECS:-0}"
RUSTICE_AUTO_RESUME="${RUSTICE_AUTO_RESUME:-TRUE}"
RUSTICE_PORT="${RUSTICE_PORT:-3000}"
RUSTICE_IMAGE_TAG="${RUSTICE_IMAGE_TAG:-latest}"
RUSTICE_SOURCE_IMAGE="${RUSTICE_SOURCE_IMAGE:-embucket/rustice:${RUSTICE_IMAGE_TAG}}"
RUSTICE_LOCAL_IMAGE="${RUSTICE_LOCAL_IMAGE:-rustice-spcs:${RUSTICE_IMAGE_TAG}}"
RUSTICE_BUILD_LOCAL="${RUSTICE_BUILD_LOCAL:-0}"
RUSTICE_REGISTRY_LOGIN="${RUSTICE_REGISTRY_LOGIN:-1}"
RUSTICE_ENABLE_EXPERIMENTAL="${RUSTICE_ENABLE_EXPERIMENTAL:-false}"
RUSTICE_TRUST_SPCS_INGRESS="${RUSTICE_TRUST_SPCS_INGRESS:-1}"
RUSTICE_SKIP_IMAGE_PUSH="${RUSTICE_SKIP_IMAGE_PUSH:-0}"
RUSTICE_REPLACE_SERVICE="${RUSTICE_REPLACE_SERVICE:-1}"
RUSTICE_DRY_RUN="${RUSTICE_DRY_RUN:-0}"

RUSTICE_HORIZON_AUTH="${RUSTICE_HORIZON_AUTH:-pat}"
RUSTICE_HORIZON_DATABASE="${RUSTICE_HORIZON_DATABASE:-EMBUCKET}"
RUSTICE_HORIZON_ROLE="${RUSTICE_HORIZON_ROLE:-}"
RUSTICE_HORIZON_SCHEMAS="${RUSTICE_HORIZON_SCHEMAS:-PUBLIC,public}"
RUSTICE_HORIZON_TABLES="${RUSTICE_HORIZON_TABLES:-}"
RUSTICE_HORIZON_EAGER_LOAD="${RUSTICE_HORIZON_EAGER_LOAD:-0}"
RUSTICE_CONFIGURE_HORIZON_SCHEMA_DEFAULTS="${RUSTICE_CONFIGURE_HORIZON_SCHEMA_DEFAULTS:-1}"
RUSTICE_HORIZON_EXTERNAL_VOLUME="${RUSTICE_HORIZON_EXTERNAL_VOLUME:-SNOWFLAKE_MANAGED}"
RUSTICE_HORIZON_CATALOG="${RUSTICE_HORIZON_CATALOG:-SNOWFLAKE}"
RUSTICE_HORIZON_SERVICE_USER="${RUSTICE_HORIZON_SERVICE_USER:-RUSTICE_HORIZON_SVC}"
RUSTICE_HORIZON_PAT_NAME="${RUSTICE_HORIZON_PAT_NAME:-RUSTICE_HORIZON_PAT}"
RUSTICE_HORIZON_PAT_DAYS="${RUSTICE_HORIZON_PAT_DAYS:-15}"
RUSTICE_CREATE_PAT_AUTH_POLICY="${RUSTICE_CREATE_PAT_AUTH_POLICY:-1}"
RUSTICE_HORIZON_PAT_AUTH_POLICY="${RUSTICE_HORIZON_PAT_AUTH_POLICY:-${RUSTICE_DB}.${RUSTICE_SCHEMA}.RUSTICE_HORIZON_PAT_AUTH_POLICY}"
RUSTICE_HORIZON_SECRET="${RUSTICE_HORIZON_SECRET:-${RUSTICE_DB}.${RUSTICE_SCHEMA}.RUSTICE_HORIZON_PAT}"
RUSTICE_JWT_SECRET="${RUSTICE_JWT_SECRET:-${RUSTICE_DB}.${RUSTICE_SCHEMA}.RUSTICE_JWT_SECRET}"
RUSTICE_ROTATE_JWT_SECRET="${RUSTICE_ROTATE_JWT_SECRET:-0}"
RUSTICE_GRANT_TO_ROLE="${RUSTICE_GRANT_TO_ROLE:-}"
RUSTICE_EGRESS_RULE="${RUSTICE_EGRESS_RULE:-RUSTICE_HORIZON_EGRESS}"
RUSTICE_EAI="${RUSTICE_EAI:-RUSTICE_HORIZON_EAI}"
RUSTICE_WAIT_FOR_READY="${RUSTICE_WAIT_FOR_READY:-1}"
RUSTICE_READY_TIMEOUT_SECS="${RUSTICE_READY_TIMEOUT_SECS:-600}"
RUSTICE_READY_POLL_SECS="${RUSTICE_READY_POLL_SECS:-10}"

RUSTICE_CREATE_INGRESS_PAT="${RUSTICE_CREATE_INGRESS_PAT:-1}"
RUSTICE_INGRESS_ROLE="${RUSTICE_INGRESS_ROLE:-RUSTICE_INGRESS_ROLE}"
RUSTICE_INGRESS_SERVICE_USER="${RUSTICE_INGRESS_SERVICE_USER:-RUSTICE_INGRESS_SVC}"
RUSTICE_INGRESS_PAT_NAME="${RUSTICE_INGRESS_PAT_NAME:-RUSTICE_INGRESS_PAT}"
RUSTICE_INGRESS_PAT_DAYS="${RUSTICE_INGRESS_PAT_DAYS:-1}"
RUSTICE_CREATE_INGRESS_PAT_AUTH_POLICY="${RUSTICE_CREATE_INGRESS_PAT_AUTH_POLICY:-1}"
RUSTICE_INGRESS_PAT_AUTH_POLICY="${RUSTICE_INGRESS_PAT_AUTH_POLICY:-${RUSTICE_DB}.${RUSTICE_SCHEMA}.RUSTICE_INGRESS_PAT_AUTH_POLICY}"

RUSTICE_GENERATE_CLIENT_CONFIG="${RUSTICE_GENERATE_CLIENT_CONFIG:-1}"
RUSTICE_CLIENT_OUTPUT_DIR="${RUSTICE_CLIENT_OUTPUT_DIR:-${SCRIPT_DIR}/generated}"
RUSTICE_CLIENT_CONNECTION="${RUSTICE_CLIENT_CONNECTION:-embucket_spcs}"
RUSTICE_CLIENT_CONFIG="${RUSTICE_CLIENT_CONFIG:-${RUSTICE_CLIENT_OUTPUT_DIR}/config.toml}"
RUSTICE_CLIENT_TOKEN_FILE="${RUSTICE_CLIENT_TOKEN_FILE:-${RUSTICE_CLIENT_OUTPUT_DIR}/embucket_spcs_token}"
RUSTICE_CLIENT_ENV_FILE="${RUSTICE_CLIENT_ENV_FILE:-${RUSTICE_CLIENT_OUTPUT_DIR}/embucket_spcs.env}"
RUSTICE_CLIENT_ACCOUNT="${RUSTICE_CLIENT_ACCOUNT:-embucket}"
RUSTICE_CLIENT_USER="${RUSTICE_CLIENT_USER:-embucket}"
RUSTICE_CLIENT_PASSWORD="${RUSTICE_CLIENT_PASSWORD:-embucket}"
RUSTICE_CLIENT_DATABASE="${RUSTICE_CLIENT_DATABASE:-embucket}"
RUSTICE_CLIENT_SCHEMA="${RUSTICE_CLIENT_SCHEMA:-public}"
RUSTICE_CLIENT_WAREHOUSE="${RUSTICE_CLIENT_WAREHOUSE:-embucket}"

usage() {
  cat <<'USAGE'
Deploy rustice/embucketd to Snowpark Container Services.

Required for the default Horizon PAT mode:
  RUSTICE_HORIZON_DATABASE  Snowflake database that contains Iceberg tables.
  RUSTICE_HORIZON_ROLE      Role granted access to those Iceberg tables.

Common options:
  SNOW_CONFIG_FILE          snowflake-cli config.toml path.
  SNOW_CONNECTION           snowflake-cli connection name. Default: default.
  RUSTICE_BUILD_LOCAL=1     Build the local Dockerfile instead of pulling embucket/rustice.
  RUSTICE_IMAGE_TAG=...     Image tag. Default: latest.
  RUSTICE_REGISTRY_LOGIN=0  Skip snow spcs image-registry login.
  RUSTICE_SKIP_IMAGE_PUSH=1 Only run SQL; assume image already exists in Snowflake.
  RUSTICE_TRUST_SPCS_INGRESS=0
                            Require Embucket demo credentials on login instead of trusting SPCS ingress.
  RUSTICE_HORIZON_AUTH      pat, bearer_token, oauth_token, or none. Default: pat.
  RUSTICE_HORIZON_SCHEMAS   Comma-separated schemas to bootstrap lazily. Default: PUBLIC,public.
  RUSTICE_HORIZON_TABLES    Comma-separated schema.table names to bootstrap lazily.
  RUSTICE_HORIZON_EAGER_LOAD=1
                            Eagerly list Horizon namespaces/tables at startup.
  RUSTICE_CONFIGURE_HORIZON_SCHEMA_DEFAULTS=0
                            Skip setting Horizon schema EXTERNAL_VOLUME/CATALOG defaults.
  RUSTICE_HORIZON_EXTERNAL_VOLUME
                            Horizon CREATE TABLE external volume default. Default: SNOWFLAKE_MANAGED.
  RUSTICE_HORIZON_CATALOG   Horizon CREATE TABLE catalog default. Default: SNOWFLAKE.
  RUSTICE_CREATE_PAT_AUTH_POLICY=0
                            Skip creating a service-user PAT authentication policy.
  RUSTICE_CREATE_INGRESS_PAT=0
                            Skip creating a service-user PAT for SPCS public ingress.
  RUSTICE_GENERATE_CLIENT_CONFIG=0
                            Skip writing deploy/spcs/generated client files for embucket-snow.
  RUSTICE_WAIT_FOR_READY=0   Do not wait for service READY/ingress URL before exiting.
  RUSTICE_AUTO_SUSPEND_SECS Service auto suspend seconds. Must be 0 for public endpoints.
  RUSTICE_EGRESS_HOSTS      Comma-separated hosts allowed from the container.
                            Include Snowflake/Horizon and object-store hosts vended by Horizon.
  RUSTICE_GRANT_TO_ROLE     Grant service endpoint access to this Snowflake role.
  RUSTICE_DRY_RUN=1         Print SQL and docker commands without executing them.

Example:
  SNOW_CONFIG_FILE=config.toml \
  RUSTICE_HORIZON_DATABASE=ANALYTICS \
  RUSTICE_HORIZON_ROLE=DATA_ENGINEER \
  ./deploy/spcs/deploy.sh
USAGE
}

log() {
  printf '>>> %s\n' "$*"
}

die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

require_ident() {
  local name="$1"
  local value="$2"
  [[ "$value" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || die "${name} must be an unquoted Snowflake identifier, got '${value}'"
}

sql_quote() {
  local value="$1"
  value="${value//\'/\'\'}"
  printf "'%s'" "$value"
}

yaml_quote() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  printf '"%s"' "$value"
}

toml_quote() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  printf '"%s"' "$value"
}

shell_quote() {
  printf '%q' "$1"
}

csv_first_value() {
  awk -F, 'NF && NR > 1 { gsub(/\r/, "", $1); gsub(/^"|"$/, "", $1); print $1; exit }'
}

csv_second_value() {
  awk -F, 'NF && NR > 1 { gsub(/\r/, "", $2); gsub(/^"|"$/, "", $2); print $2; exit }'
}

snow_global_args=()
if [[ -n "${SNOW_CONFIG_FILE:-}" ]]; then
  snow_global_args+=(--config-file "$SNOW_CONFIG_FILE")
fi

snow_sql_args=(--connection "$SNOW_CONNECTION")
if [[ -n "${SNOW_ROLE:-}" ]]; then
  snow_sql_args+=(--role "$SNOW_ROLE")
fi
if [[ -n "${SNOW_WAREHOUSE:-}" ]]; then
  snow_sql_args+=(--warehouse "$SNOW_WAREHOUSE")
fi

run_snow_sql() {
  local sql="$1"
  if [[ "$RUSTICE_DRY_RUN" == "1" ]]; then
    printf '%s\n' "$sql"
    return 0
  fi

  local sql_file
  sql_file="$(mktemp "${TMPDIR:-/tmp}/rustice-spcs.XXXXXX.sql")"
  printf '%s\n' "$sql" > "$sql_file"
  SNOWFLAKE_HOME="$SNOWFLAKE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" "$SNOW_BIN" "${snow_global_args[@]}" sql "${snow_sql_args[@]}" --filename "$sql_file"
  rm -f "$sql_file"
}

run_snow_sql_sensitive() {
  local sql="$1"
  local redacted_sql="${2:-<sensitive SQL redacted>}"
  if [[ "$RUSTICE_DRY_RUN" == "1" ]]; then
    printf '%s\n' "$redacted_sql"
    return 0
  fi

  local sql_file
  sql_file="$(mktemp "${TMPDIR:-/tmp}/rustice-spcs.XXXXXX.sql")"
  printf '%s\n' "$sql" > "$sql_file"
  SNOWFLAKE_HOME="$SNOWFLAKE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" "$SNOW_BIN" "${snow_global_args[@]}" sql "${snow_sql_args[@]}" --silent --filename "$sql_file" >/dev/null
  rm -f "$sql_file"
}

snow_scalar() {
  local sql="$1"
  SNOWFLAKE_HOME="$SNOWFLAKE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" "$SNOW_BIN" "${snow_global_args[@]}" sql "${snow_sql_args[@]}" --format CSV --silent --query "$sql" | csv_first_value
}

snow_second_scalar() {
  local sql="$1"
  SNOWFLAKE_HOME="$SNOWFLAKE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" "$SNOW_BIN" "${snow_global_args[@]}" sql "${snow_sql_args[@]}" --format CSV --silent --query "$sql" | csv_second_value
}

snow_endpoint_url() {
  local endpoint_name="$1"
  local endpoint_name_lower
  endpoint_name_lower="$(normalize_lower "$endpoint_name")"
  SNOWFLAKE_HOME="$SNOWFLAKE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" "$SNOW_BIN" "${snow_global_args[@]}" sql "${snow_sql_args[@]}" --format CSV --silent --query "SHOW ENDPOINTS IN SERVICE ${RUSTICE_DB}.${RUSTICE_SCHEMA}.${RUSTICE_SERVICE}" \
    | awk -F, -v endpoint="$endpoint_name_lower" '
        NF && NR > 1 {
          gsub(/\r/, "", $1);
          gsub(/\r/, "", $6);
          gsub(/^"|"$/, "", $1);
          gsub(/^"|"$/, "", $6);
          name = tolower($1);
          if (name == endpoint) {
            print $6;
            exit;
          }
        }'
}

docker_cmd() {
  if [[ -n "${CONTAINER_CLI:-}" ]]; then
    printf '%s' "$CONTAINER_CLI"
  elif command -v docker >/dev/null 2>&1; then
    printf 'docker'
  elif command -v podman >/dev/null 2>&1; then
    printf 'podman'
  else
    die "docker or podman is required"
  fi
}

validate_container_cli() {
  local cli="$1"
  if [[ "$RUSTICE_DRY_RUN" == "1" ]]; then
    return 0
  fi

  if ! "$cli" version >/dev/null 2>&1; then
    die "${cli} is present but not usable. Enable Docker Desktop WSL integration, start the container engine, install podman, or set RUSTICE_SKIP_IMAGE_PUSH=1 if the image is already in Snowflake."
  fi
}

run_cmd() {
  if [[ "$RUSTICE_DRY_RUN" == "1" ]]; then
    printf '+'
    printf ' %q' "$@"
    printf '\n'
  else
    "$@"
  fi
}

wait_for_service_ready() {
  if [[ "$RUSTICE_DRY_RUN" == "1" || "$RUSTICE_WAIT_FOR_READY" != "1" ]]; then
    return 0
  fi

  log "Waiting for SPCS service READY and public ingress URL"
  local waited=0
  local status_json=""
  local ingress_url=""
  while (( waited <= RUSTICE_READY_TIMEOUT_SECS )); do
    status_json="$(snow_scalar "SELECT SYSTEM\$GET_SERVICE_STATUS('${RUSTICE_DB}.${RUSTICE_SCHEMA}.${RUSTICE_SERVICE}')")"
    ingress_url="$(snow_endpoint_url "$RUSTICE_ENDPOINT_NAME")"
    if [[ "$status_json" == *READY* && "$ingress_url" == *.snowflakecomputing.app ]]; then
      RUSTICE_RESOLVED_INGRESS_URL="$ingress_url"
      return 0
    fi
    sleep "$RUSTICE_READY_POLL_SECS"
    waited=$((waited + RUSTICE_READY_POLL_SECS))
  done

  die "SPCS service did not become READY within ${RUSTICE_READY_TIMEOUT_SECS}s. Check with SELECT SYSTEM\$GET_SERVICE_STATUS('${RUSTICE_DB}.${RUSTICE_SCHEMA}.${RUSTICE_SERVICE}')"
}

create_ingress_pat() {
  if [[ "$RUSTICE_CREATE_INGRESS_PAT" != "1" ]]; then
    return 0
  fi

  log "Creating SPCS ingress service user PAT"
  run_snow_sql "
CREATE ROLE IF NOT EXISTS ${RUSTICE_INGRESS_ROLE};
CREATE USER IF NOT EXISTS ${RUSTICE_INGRESS_SERVICE_USER}
  TYPE = SERVICE
  DEFAULT_ROLE = ${RUSTICE_INGRESS_ROLE}
  COMMENT = 'Service user used by embucket-snow to access Rustice SPCS ingress';
GRANT ROLE ${RUSTICE_INGRESS_ROLE} TO USER ${RUSTICE_INGRESS_SERVICE_USER};
GRANT USAGE ON DATABASE ${RUSTICE_DB} TO ROLE ${RUSTICE_INGRESS_ROLE};
GRANT USAGE ON SCHEMA ${RUSTICE_DB}.${RUSTICE_SCHEMA} TO ROLE ${RUSTICE_INGRESS_ROLE};
GRANT SERVICE ROLE ${RUSTICE_DB}.${RUSTICE_SCHEMA}.${RUSTICE_SERVICE}!${RUSTICE_SERVICE_ROLE} TO ROLE ${RUSTICE_INGRESS_ROLE};
"
  if [[ "$RUSTICE_CREATE_INGRESS_PAT_AUTH_POLICY" == "1" ]]; then
    run_snow_sql "
CREATE AUTHENTICATION POLICY IF NOT EXISTS ${RUSTICE_INGRESS_PAT_AUTH_POLICY}
  PAT_POLICY = (
    NETWORK_POLICY_EVALUATION = ENFORCED_NOT_REQUIRED
    REQUIRE_ROLE_RESTRICTION_FOR_SERVICE_USERS = TRUE
  );
ALTER USER IF EXISTS ${RUSTICE_INGRESS_SERVICE_USER}
  SET AUTHENTICATION POLICY ${RUSTICE_INGRESS_PAT_AUTH_POLICY} FORCE;
"
  fi

  run_snow_sql "ALTER USER IF EXISTS ${RUSTICE_INGRESS_SERVICE_USER} REMOVE PROGRAMMATIC ACCESS TOKEN ${RUSTICE_INGRESS_PAT_NAME};" >/dev/null 2>&1 || true
  if [[ "$RUSTICE_DRY_RUN" == "1" ]]; then
    printf '+ snow sql --query %q\n' "ALTER USER IF EXISTS ${RUSTICE_INGRESS_SERVICE_USER} ADD PROGRAMMATIC ACCESS TOKEN ${RUSTICE_INGRESS_PAT_NAME} ROLE_RESTRICTION = '${RUSTICE_INGRESS_ROLE}' DAYS_TO_EXPIRY = ${RUSTICE_INGRESS_PAT_DAYS}"
    RUSTICE_RESOLVED_INGRESS_PAT="dry-run-ingress-token"
  else
    RUSTICE_RESOLVED_INGRESS_PAT="$(snow_second_scalar "ALTER USER IF EXISTS ${RUSTICE_INGRESS_SERVICE_USER} ADD PROGRAMMATIC ACCESS TOKEN ${RUSTICE_INGRESS_PAT_NAME} ROLE_RESTRICTION = '${RUSTICE_INGRESS_ROLE}' DAYS_TO_EXPIRY = ${RUSTICE_INGRESS_PAT_DAYS}")"
  fi
}

write_client_files() {
  local ingress_url="$1"
  local ingress_pat="$2"

  if [[ "$RUSTICE_DRY_RUN" == "1" || "$RUSTICE_GENERATE_CLIENT_CONFIG" != "1" ]]; then
    return 0
  fi

  [[ -n "$ingress_url" && "$ingress_url" == *.snowflakecomputing.app ]] || die "Cannot generate client config without a public ingress URL"

  umask 077
  mkdir -p "$RUSTICE_CLIENT_OUTPUT_DIR"
  cat > "$RUSTICE_CLIENT_CONFIG" <<EOF
default_connection_name = $(toml_quote "$RUSTICE_CLIENT_CONNECTION")

[connections.${RUSTICE_CLIENT_CONNECTION}]
host = $(toml_quote "$ingress_url")
protocol = "https"
port = 443
account = $(toml_quote "$RUSTICE_CLIENT_ACCOUNT")
user = $(toml_quote "$RUSTICE_CLIENT_USER")
password = $(toml_quote "$RUSTICE_CLIENT_PASSWORD")
database = $(toml_quote "$RUSTICE_CLIENT_DATABASE")
schema = $(toml_quote "$RUSTICE_CLIENT_SCHEMA")
warehouse = $(toml_quote "$RUSTICE_CLIENT_WAREHOUSE")
EOF

  if [[ -n "$ingress_pat" ]]; then
    printf '%s' "$ingress_pat" > "$RUSTICE_CLIENT_TOKEN_FILE"
  fi

  cat > "$RUSTICE_CLIENT_ENV_FILE" <<EOF
export EMBUCKET_SPCS_TOKEN_FILE=$(shell_quote "$RUSTICE_CLIENT_TOKEN_FILE")
EOF
}

run_snow_cmd() {
  if [[ "$RUSTICE_DRY_RUN" == "1" ]]; then
    printf '+ env SNOWFLAKE_HOME=%q XDG_CONFIG_HOME=%q %q' "$SNOWFLAKE_HOME" "$XDG_CONFIG_HOME" "$SNOW_BIN"
    printf ' %q' "${snow_global_args[@]}" "$@"
    printf '\n'
  else
    SNOWFLAKE_HOME="$SNOWFLAKE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" "$SNOW_BIN" "${snow_global_args[@]}" "$@"
  fi
}

random_secret() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
  else
    od -An -N32 -tx1 /dev/urandom | tr -d ' \n'
  fi
}

normalize_lower() {
  printf '%s' "$1" | tr '[:upper:]' '[:lower:]'
}

trim() {
  local value="$1"
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"
  printf '%s' "$value"
}

url_host() {
  local url="$1"
  url="${url#*://}"
  url="${url%%/*}"
  url="${url%%:*}"
  printf '%s' "$url"
}

default_egress_hosts() {
  local catalog_host="$1"
  local current_region="$2"
  local hosts="$catalog_host"

  if [[ "$current_region" == AWS_* ]]; then
    local aws_region
    aws_region="$(printf '%s' "${current_region#AWS_}" | tr '[:upper:]' '[:lower:]' | tr '_' '-')"
    hosts+=",s3.${aws_region}.amazonaws.com"
  fi

  printf '%s' "$hosts"
}

require_ident RUSTICE_DB "$RUSTICE_DB"
require_ident RUSTICE_SCHEMA "$RUSTICE_SCHEMA"
require_ident RUSTICE_COMPUTE_POOL "$RUSTICE_COMPUTE_POOL"
require_ident RUSTICE_IMAGE_REPOSITORY "$RUSTICE_IMAGE_REPOSITORY"
require_ident RUSTICE_SERVICE "$RUSTICE_SERVICE"
require_ident RUSTICE_ENDPOINT_NAME "$RUSTICE_ENDPOINT_NAME"
require_ident RUSTICE_EGRESS_RULE "$RUSTICE_EGRESS_RULE"
require_ident RUSTICE_EAI "$RUSTICE_EAI"
require_ident RUSTICE_HORIZON_DATABASE "$RUSTICE_HORIZON_DATABASE"
require_ident RUSTICE_CLIENT_CONNECTION "$RUSTICE_CLIENT_CONNECTION"

case "$RUSTICE_CONFIGURE_HORIZON_SCHEMA_DEFAULTS" in
  0|1)
    ;;
  *)
    die "RUSTICE_CONFIGURE_HORIZON_SCHEMA_DEFAULTS must be 0 or 1"
    ;;
esac
case "$RUSTICE_TRUST_SPCS_INGRESS" in
  0|1)
    ;;
  *)
    die "RUSTICE_TRUST_SPCS_INGRESS must be 0 or 1"
    ;;
esac
case "$RUSTICE_WAIT_FOR_READY" in
  0|1)
    ;;
  *)
    die "RUSTICE_WAIT_FOR_READY must be 0 or 1"
    ;;
esac
case "$RUSTICE_CREATE_INGRESS_PAT" in
  0|1)
    ;;
  *)
    die "RUSTICE_CREATE_INGRESS_PAT must be 0 or 1"
    ;;
esac
case "$RUSTICE_CREATE_INGRESS_PAT_AUTH_POLICY" in
  0|1)
    ;;
  *)
    die "RUSTICE_CREATE_INGRESS_PAT_AUTH_POLICY must be 0 or 1"
    ;;
esac
case "$RUSTICE_GENERATE_CLIENT_CONFIG" in
  0|1)
    ;;
  *)
    die "RUSTICE_GENERATE_CLIENT_CONFIG must be 0 or 1"
    ;;
esac
[[ "$RUSTICE_READY_TIMEOUT_SECS" =~ ^[0-9]+$ ]] || die "RUSTICE_READY_TIMEOUT_SECS must be a non-negative integer"
[[ "$RUSTICE_READY_POLL_SECS" =~ ^[0-9]+$ ]] || die "RUSTICE_READY_POLL_SECS must be a non-negative integer"
(( RUSTICE_READY_POLL_SECS > 0 )) || die "RUSTICE_READY_POLL_SECS must be greater than zero"
if [[ "$RUSTICE_CONFIGURE_HORIZON_SCHEMA_DEFAULTS" == "1" ]]; then
  require_ident RUSTICE_HORIZON_EXTERNAL_VOLUME "$RUSTICE_HORIZON_EXTERNAL_VOLUME"
  require_ident RUSTICE_HORIZON_CATALOG "$RUSTICE_HORIZON_CATALOG"
fi
auth_trust_spcs_ingress_value="false"
if [[ "$RUSTICE_TRUST_SPCS_INGRESS" == "1" ]]; then
  auth_trust_spcs_ingress_value="true"
fi

[[ "$RUSTICE_AUTO_SUSPEND_SECS" =~ ^[0-9]+$ ]] || die "RUSTICE_AUTO_SUSPEND_SECS must be a non-negative integer"
if (( RUSTICE_AUTO_SUSPEND_SECS != 0 )); then
  die "SPCS services with public endpoints do not support AUTO_SUSPEND_SECS > 0. Set RUSTICE_AUTO_SUSPEND_SECS=0 or make the endpoint private in the service spec."
fi

if [[ "$RUSTICE_CREATE_INGRESS_PAT" == "1" ]]; then
  require_ident RUSTICE_INGRESS_ROLE "$RUSTICE_INGRESS_ROLE"
  require_ident RUSTICE_INGRESS_SERVICE_USER "$RUSTICE_INGRESS_SERVICE_USER"
  require_ident RUSTICE_INGRESS_PAT_NAME "$RUSTICE_INGRESS_PAT_NAME"
  [[ "$RUSTICE_INGRESS_PAT_DAYS" =~ ^[0-9]+$ ]] || die "RUSTICE_INGRESS_PAT_DAYS must be a positive integer"
  (( RUSTICE_INGRESS_PAT_DAYS > 0 )) || die "RUSTICE_INGRESS_PAT_DAYS must be a positive integer"
fi

case "$RUSTICE_HORIZON_AUTH" in
  pat)
    [[ -n "$RUSTICE_HORIZON_ROLE" ]] || die "RUSTICE_HORIZON_ROLE is required when RUSTICE_HORIZON_AUTH=pat"
    require_ident RUSTICE_HORIZON_ROLE "$RUSTICE_HORIZON_ROLE"
    require_ident RUSTICE_HORIZON_SERVICE_USER "$RUSTICE_HORIZON_SERVICE_USER"
    require_ident RUSTICE_HORIZON_PAT_NAME "$RUSTICE_HORIZON_PAT_NAME"
    ;;
  bearer_token|oauth_token)
    [[ -n "$RUSTICE_HORIZON_ROLE" ]] || die "RUSTICE_HORIZON_ROLE is required when RUSTICE_HORIZON_AUTH=${RUSTICE_HORIZON_AUTH}"
    require_ident RUSTICE_HORIZON_ROLE "$RUSTICE_HORIZON_ROLE"
    ;;
  none)
    ;;
  -h|--help)
    usage
    exit 0
    ;;
  *)
    die "RUSTICE_HORIZON_AUTH must be pat, bearer_token, oauth_token, or none"
    ;;
esac

mkdir -p "$SNOWFLAKE_HOME" "$XDG_CONFIG_HOME"

container_cli=""
if [[ "$RUSTICE_SKIP_IMAGE_PUSH" != "1" ]]; then
  container_cli="$(docker_cmd)"
  validate_container_cli "$container_cli"
fi

log "Creating Snowflake SPCS resources"
run_snow_sql "
CREATE DATABASE IF NOT EXISTS ${RUSTICE_DB};
CREATE SCHEMA IF NOT EXISTS ${RUSTICE_DB}.${RUSTICE_SCHEMA};
CREATE IMAGE REPOSITORY IF NOT EXISTS ${RUSTICE_DB}.${RUSTICE_SCHEMA}.${RUSTICE_IMAGE_REPOSITORY};
CREATE COMPUTE POOL IF NOT EXISTS ${RUSTICE_COMPUTE_POOL}
  MIN_NODES = ${RUSTICE_POOL_MIN_NODES}
  MAX_NODES = ${RUSTICE_POOL_MAX_NODES}
  INSTANCE_FAMILY = ${RUSTICE_INSTANCE_FAMILY};
"

if [[ "$RUSTICE_DRY_RUN" == "1" ]]; then
  account_identifier="${RUSTICE_ACCOUNT_IDENTIFIER:-example-org-example-account}"
  current_region="${RUSTICE_CURRENT_REGION:-AWS_US_EAST_2}"
  if [[ -z "${RUSTICE_ACCOUNT_IDENTIFIER:-}" ]]; then
    log "Dry run uses placeholder account identifier '${account_identifier}'. Set RUSTICE_ACCOUNT_IDENTIFIER for account-specific SQL."
  fi
else
  account_identifier="${RUSTICE_ACCOUNT_IDENTIFIER:-$(snow_scalar "SELECT LOWER(REPLACE(CURRENT_ORGANIZATION_NAME() || '-' || CURRENT_ACCOUNT_NAME(), '_', '-'))")}"
  current_region="${RUSTICE_CURRENT_REGION:-$(snow_scalar "SELECT CURRENT_REGION()")}"
fi

[[ -n "$account_identifier" ]] || die "Could not resolve Snowflake account identifier"

registry_host="${account_identifier}.registry.snowflakecomputing.com"
repo_url="${registry_host}/$(normalize_lower "$RUSTICE_DB")/$(normalize_lower "$RUSTICE_SCHEMA")/$(normalize_lower "$RUSTICE_IMAGE_REPOSITORY")"
service_image="${repo_url}/rustice:${RUSTICE_IMAGE_TAG}"
catalog_url="${RUSTICE_CATALOG_URL:-https://${account_identifier}.snowflakecomputing.com/polaris/api/catalog}"
catalog_host="$(url_host "$catalog_url")"

egress_hosts="${RUSTICE_EGRESS_HOSTS:-$(default_egress_hosts "$catalog_host" "$current_region")}"
egress_values_sql=""
IFS=',' read -r -a egress_host_array <<< "$egress_hosts"
for host in "${egress_host_array[@]}"; do
  host="$(trim "$host")"
  [[ -n "$host" ]] || continue
  if [[ -n "$egress_values_sql" ]]; then
    egress_values_sql+=", "
  fi
  egress_values_sql+="$(sql_quote "$host")"
done

if [[ "$RUSTICE_HORIZON_AUTH" != "none" ]]; then
  [[ -n "$egress_values_sql" ]] || die "RUSTICE_EGRESS_HOSTS resolved to an empty list"
  log "Horizon catalog URL: ${catalog_url}"
  log "External access hosts: ${egress_hosts}"
  log "Creating External Access Integration for Horizon"
  run_snow_sql "
CREATE OR REPLACE NETWORK RULE ${RUSTICE_DB}.${RUSTICE_SCHEMA}.${RUSTICE_EGRESS_RULE}
  TYPE = HOST_PORT
  MODE = EGRESS
  VALUE_LIST = (${egress_values_sql});

CREATE OR REPLACE EXTERNAL ACCESS INTEGRATION ${RUSTICE_EAI}
  ALLOWED_NETWORK_RULES = (${RUSTICE_DB}.${RUSTICE_SCHEMA}.${RUSTICE_EGRESS_RULE})
  ENABLED = TRUE;
"
fi

if [[ "$RUSTICE_HORIZON_AUTH" != "none" && "$RUSTICE_CONFIGURE_HORIZON_SCHEMA_DEFAULTS" == "1" ]]; then
  horizon_schema_defaults_sql=""
  seen_horizon_schema_defaults="|"
  IFS=',' read -r -a horizon_schema_array <<< "$RUSTICE_HORIZON_SCHEMAS"
  for schema in "${horizon_schema_array[@]}"; do
    schema="$(trim "$schema")"
    [[ -n "$schema" ]] || continue
    require_ident RUSTICE_HORIZON_SCHEMAS "$schema"
    schema_key="$(normalize_lower "$schema")"
    if [[ "$seen_horizon_schema_defaults" == *"|${schema_key}|"* ]]; then
      continue
    fi
    seen_horizon_schema_defaults+="${schema_key}|"
    horizon_schema_defaults_sql+="
ALTER SCHEMA IF EXISTS ${RUSTICE_HORIZON_DATABASE}.${schema}
  SET EXTERNAL_VOLUME = $(sql_quote "$RUSTICE_HORIZON_EXTERNAL_VOLUME");
ALTER SCHEMA IF EXISTS ${RUSTICE_HORIZON_DATABASE}.${schema}
  SET CATALOG = $(sql_quote "$RUSTICE_HORIZON_CATALOG");
"
  done
  if [[ -n "$horizon_schema_defaults_sql" ]]; then
    log "Configuring Horizon schema Iceberg defaults"
    run_snow_sql "$horizon_schema_defaults_sql"
  fi
fi

if [[ "$RUSTICE_SKIP_IMAGE_PUSH" != "1" ]]; then
  log "Publishing image ${service_image}"
  if [[ -n "${RUSTICE_REGISTRY_USER:-}" && -n "${RUSTICE_REGISTRY_PASSWORD:-}" ]]; then
    if [[ "$RUSTICE_DRY_RUN" == "1" ]]; then
      printf '+ %s login %q -u %q --password-stdin\n' "$container_cli" "$registry_host" "$RUSTICE_REGISTRY_USER"
    else
      printf '%s' "$RUSTICE_REGISTRY_PASSWORD" | "$container_cli" login "$registry_host" -u "$RUSTICE_REGISTRY_USER" --password-stdin
    fi
  else
    if [[ "$RUSTICE_REGISTRY_LOGIN" == "1" ]]; then
      log "Logging in to ${registry_host} through Snowflake CLI"
      run_snow_cmd spcs image-registry login "${snow_sql_args[@]}"
    else
      log "Assuming ${container_cli} is already logged in to ${registry_host}"
    fi
  fi

  if [[ "$RUSTICE_BUILD_LOCAL" == "1" ]]; then
    run_cmd "$container_cli" build --platform linux/amd64 --build-arg "ENABLE_EXPERIMENTAL=${RUSTICE_ENABLE_EXPERIMENTAL}" -t "$RUSTICE_LOCAL_IMAGE" "$REPO_ROOT"
  else
    run_cmd "$container_cli" pull --platform linux/amd64 "$RUSTICE_SOURCE_IMAGE"
    RUSTICE_LOCAL_IMAGE="$RUSTICE_SOURCE_IMAGE"
  fi
  run_cmd "$container_cli" tag "$RUSTICE_LOCAL_IMAGE" "$service_image"
  run_cmd "$container_cli" push "$service_image"
fi

if [[ "$RUSTICE_ROTATE_JWT_SECRET" == "1" ]]; then
  jwt_sql="CREATE OR REPLACE SECRET ${RUSTICE_JWT_SECRET} TYPE = GENERIC_STRING SECRET_STRING = $(sql_quote "$(random_secret)");"
else
  jwt_sql="CREATE SECRET IF NOT EXISTS ${RUSTICE_JWT_SECRET} TYPE = GENERIC_STRING SECRET_STRING = $(sql_quote "$(random_secret)");"
fi

log "Creating runtime secrets"
run_snow_sql_sensitive "$jwt_sql" "CREATE SECRET IF NOT EXISTS ${RUSTICE_JWT_SECRET} TYPE = GENERIC_STRING SECRET_STRING = '<redacted>';"

horizon_secret_line=""
horizon_env_lines=""
if [[ "$RUSTICE_HORIZON_AUTH" != "none" ]]; then
  horizon_env_lines+="        CATALOG_URL: $(yaml_quote "$catalog_url")
        ICEBERG_REST_PREFIX: $(yaml_quote "$RUSTICE_HORIZON_DATABASE")
        ICEBERG_REST_SCOPE: $(yaml_quote "session:role:${RUSTICE_HORIZON_ROLE}")
        ICEBERG_REST_SCHEMAS: $(yaml_quote "$RUSTICE_HORIZON_SCHEMAS")
        ICEBERG_REST_EAGER_LOAD: $(yaml_quote "$RUSTICE_HORIZON_EAGER_LOAD")"
  if [[ -n "$RUSTICE_HORIZON_TABLES" ]]; then
    horizon_env_lines+="
        ICEBERG_REST_TABLES: $(yaml_quote "$RUSTICE_HORIZON_TABLES")"
  fi
fi

case "$RUSTICE_HORIZON_AUTH" in
  pat)
    log "Creating service user PAT and storing it in a Snowflake secret"
    run_snow_sql "
CREATE USER IF NOT EXISTS ${RUSTICE_HORIZON_SERVICE_USER}
  TYPE = SERVICE
  DEFAULT_ROLE = ${RUSTICE_HORIZON_ROLE}
  COMMENT = 'Service user used by rustice SPCS to access Horizon Catalog';
GRANT ROLE ${RUSTICE_HORIZON_ROLE} TO USER ${RUSTICE_HORIZON_SERVICE_USER};
"
    if [[ "$RUSTICE_CREATE_PAT_AUTH_POLICY" == "1" ]]; then
      run_snow_sql "
CREATE AUTHENTICATION POLICY IF NOT EXISTS ${RUSTICE_HORIZON_PAT_AUTH_POLICY}
  PAT_POLICY = (
    NETWORK_POLICY_EVALUATION = ENFORCED_NOT_REQUIRED
    REQUIRE_ROLE_RESTRICTION_FOR_SERVICE_USERS = TRUE
  );
ALTER USER IF EXISTS ${RUSTICE_HORIZON_SERVICE_USER}
  SET AUTHENTICATION POLICY ${RUSTICE_HORIZON_PAT_AUTH_POLICY} FORCE;
"
    fi
    run_snow_sql "ALTER USER IF EXISTS ${RUSTICE_HORIZON_SERVICE_USER} REMOVE PROGRAMMATIC ACCESS TOKEN ${RUSTICE_HORIZON_PAT_NAME};" >/dev/null 2>&1 || true
    if [[ "$RUSTICE_DRY_RUN" == "1" ]]; then
      pat_token="dry-run-token"
      printf '+ snow sql --query %q\n' "ALTER USER IF EXISTS ${RUSTICE_HORIZON_SERVICE_USER} ADD PROGRAMMATIC ACCESS TOKEN ${RUSTICE_HORIZON_PAT_NAME} ROLE_RESTRICTION = '${RUSTICE_HORIZON_ROLE}' DAYS_TO_EXPIRY = ${RUSTICE_HORIZON_PAT_DAYS}"
    else
      pat_token="$(snow_second_scalar "ALTER USER IF EXISTS ${RUSTICE_HORIZON_SERVICE_USER} ADD PROGRAMMATIC ACCESS TOKEN ${RUSTICE_HORIZON_PAT_NAME} ROLE_RESTRICTION = '${RUSTICE_HORIZON_ROLE}' DAYS_TO_EXPIRY = ${RUSTICE_HORIZON_PAT_DAYS}")"
    fi
    [[ -n "$pat_token" ]] || die "Could not read token_secret from ALTER USER ADD PROGRAMMATIC ACCESS TOKEN output"
    run_snow_sql_sensitive \
      "CREATE OR REPLACE SECRET ${RUSTICE_HORIZON_SECRET} TYPE = GENERIC_STRING SECRET_STRING = $(sql_quote "$pat_token");" \
      "CREATE OR REPLACE SECRET ${RUSTICE_HORIZON_SECRET} TYPE = GENERIC_STRING SECRET_STRING = '<redacted>';"
    horizon_secret_line="        - snowflakeSecret: ${RUSTICE_HORIZON_SECRET}
          envVarName: ICEBERG_REST_CREDENTIAL
          secretKeyRef: secret_string"
    ;;
  bearer_token)
    horizon_secret_line="        - snowflakeSecret: ${RUSTICE_HORIZON_SECRET}
          envVarName: ICEBERG_REST_BEARER_TOKEN
          secretKeyRef: secret_string"
    ;;
  oauth_token)
    horizon_secret_line="        - snowflakeSecret: ${RUSTICE_HORIZON_SECRET}
          envVarName: ICEBERG_REST_OAUTH_TOKEN
          secretKeyRef: secret_string"
    ;;
esac

secrets_yaml="      secrets:
        - snowflakeSecret: ${RUSTICE_JWT_SECRET}
          envVarName: JWT_SECRET
          secretKeyRef: secret_string"
if [[ -n "$horizon_secret_line" ]]; then
  secrets_yaml+="
${horizon_secret_line}"
fi

drop_service_sql=""
if [[ "$RUSTICE_REPLACE_SERVICE" == "1" ]]; then
  drop_service_sql="DROP SERVICE IF EXISTS ${RUSTICE_DB}.${RUSTICE_SCHEMA}.${RUSTICE_SERVICE};"
fi

service_options=""
service_options+="
  AUTO_SUSPEND_SECS = ${RUSTICE_AUTO_SUSPEND_SECS}"
if [[ "$RUSTICE_HORIZON_AUTH" != "none" ]]; then
  service_options+="
  EXTERNAL_ACCESS_INTEGRATIONS = (${RUSTICE_EAI})"
fi
service_options+="
  AUTO_RESUME = ${RUSTICE_AUTO_RESUME}
  MIN_INSTANCES = ${RUSTICE_MIN_INSTANCES}
  MAX_INSTANCES = ${RUSTICE_MAX_INSTANCES}"

grant_sql=""
if [[ -n "$RUSTICE_GRANT_TO_ROLE" ]]; then
  require_ident RUSTICE_GRANT_TO_ROLE "$RUSTICE_GRANT_TO_ROLE"
  grant_sql="GRANT USAGE ON DATABASE ${RUSTICE_DB} TO ROLE ${RUSTICE_GRANT_TO_ROLE};
GRANT USAGE ON SCHEMA ${RUSTICE_DB}.${RUSTICE_SCHEMA} TO ROLE ${RUSTICE_GRANT_TO_ROLE};
GRANT SERVICE ROLE ${RUSTICE_DB}.${RUSTICE_SCHEMA}.${RUSTICE_SERVICE}!${RUSTICE_SERVICE_ROLE} TO ROLE ${RUSTICE_GRANT_TO_ROLE};"
fi

log "Creating SPCS service"
run_snow_sql "
${drop_service_sql}

CREATE SERVICE ${RUSTICE_DB}.${RUSTICE_SCHEMA}.${RUSTICE_SERVICE}
  IN COMPUTE POOL ${RUSTICE_COMPUTE_POOL}
  FROM SPECIFICATION \$\$
spec:
  containers:
    - name: ${RUSTICE_CONTAINER_NAME}
      image: ${service_image}
      env:
        BUCKET_HOST: \"0.0.0.0\"
        BUCKET_PORT: $(yaml_quote "$RUSTICE_PORT")
        RUST_LOG: $(yaml_quote "${RUST_LOG:-info}")
        AUTH_TRUST_SPCS_INGRESS: $(yaml_quote "$auth_trust_spcs_ingress_value")
${horizon_env_lines}
${secrets_yaml}
      readinessProbe:
        port: ${RUSTICE_PORT}
        path: /health
  endpoints:
    - name: ${RUSTICE_ENDPOINT_NAME}
      port: ${RUSTICE_PORT}
      public: true
capabilities:
  securityContext:
    executeAsCaller: true
serviceRoles:
  - name: ${RUSTICE_SERVICE_ROLE}
    endpoints:
      - ${RUSTICE_ENDPOINT_NAME}
\$\$
${service_options};

${grant_sql}

SHOW SERVICES IN SCHEMA ${RUSTICE_DB}.${RUSTICE_SCHEMA};
SELECT SYSTEM\$GET_SERVICE_STATUS('${RUSTICE_DB}.${RUSTICE_SCHEMA}.${RUSTICE_SERVICE}');
SHOW ENDPOINTS IN SERVICE ${RUSTICE_DB}.${RUSTICE_SCHEMA}.${RUSTICE_SERVICE};
"

RUSTICE_RESOLVED_INGRESS_URL=""
if [[ "$RUSTICE_DRY_RUN" == "1" ]]; then
  RUSTICE_RESOLVED_INGRESS_URL="${RUSTICE_INGRESS_URL:-<ingress-url>}"
elif [[ "$RUSTICE_WAIT_FOR_READY" == "1" ]]; then
  wait_for_service_ready
else
  RUSTICE_RESOLVED_INGRESS_URL="$(snow_endpoint_url "$RUSTICE_ENDPOINT_NAME")"
fi

RUSTICE_RESOLVED_INGRESS_PAT=""
create_ingress_pat

if [[ "$RUSTICE_CREATE_INGRESS_PAT" == "1" && "$RUSTICE_DRY_RUN" != "1" && -z "$RUSTICE_RESOLVED_INGRESS_PAT" ]]; then
  die "Could not read token_secret from ALTER USER ADD PROGRAMMATIC ACCESS TOKEN output"
fi

write_client_files "$RUSTICE_RESOLVED_INGRESS_URL" "$RUSTICE_RESOLVED_INGRESS_PAT"

log "Done"
log "Image: ${service_image}"
log "Catalog URL: ${catalog_url}"
if [[ -n "$RUSTICE_RESOLVED_INGRESS_URL" ]]; then
  log "Ingress URL: ${RUSTICE_RESOLVED_INGRESS_URL}"
fi
if [[ "$RUSTICE_GENERATE_CLIENT_CONFIG" == "1" && "$RUSTICE_DRY_RUN" != "1" ]]; then
  log "embucket-snow config: ${RUSTICE_CLIENT_CONFIG}"
  log "embucket-snow token file: ${RUSTICE_CLIENT_TOKEN_FILE}"
  cat <<EOF

Run a smoke query:
  embucket-snow --config-file $(shell_quote "$RUSTICE_CLIENT_CONFIG") sql -c ${RUSTICE_CLIENT_CONNECTION} -q "SELECT * FROM embucket.public.smoke"

EOF
fi
log "Check logs with: SELECT SYSTEM\$GET_SERVICE_LOGS('${RUSTICE_DB}.${RUSTICE_SCHEMA}.${RUSTICE_SERVICE}', 0, '${RUSTICE_CONTAINER_NAME}', 100);"
