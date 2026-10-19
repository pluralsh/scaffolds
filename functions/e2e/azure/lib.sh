# shellcheck shell=bash
# The variables are used by the scripts that source this file.
# shellcheck disable=SC2034
# Shared settings and helpers of the Azure end-to-end test scripts. Sourced, not run.
#
# Settings (environment):
#   E2E_PREFIX    prefix of every resource group (default cfe2e)
#   E2E_LOCATION  Azure location of the fixtures (default eastus)
#   E2E_SUB       subscription ID (default: the az CLI's current subscription)

set -euo pipefail

E2E_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FUNCTIONS_DIR="$(cd "$E2E_DIR/../.." && pwd)"
WORK="$E2E_DIR/.work"
FIXTURES_ENV="$WORK/fixtures.env"

P="${E2E_PREFIX:-cfe2e}"
LOCATION="${E2E_LOCATION:-eastus}"

if ! [[ "$P" =~ ^[a-z][a-z0-9]{2,11}$ ]]; then
  echo "E2E_PREFIX must be 3-12 lowercase letters and digits, starting with a letter" >&2
  exit 2
fi

for tool in az jq; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 2; }
done

SUB="${E2E_SUB:-$(az account show --query id -o tsv)}"

# Resource groups. AKS creates RG_NODE itself as the cluster's node resource group.
RG_AKS="$P-aks"
RG_NODE="$P-node"
RG_VMS="$P-vms"
RG_DB="$P-db"
AKS="$P-aks"

# Every resource group the fixtures create carries this tag, so cleanup can find them.
TAG_KEY="plural-e2e"

rg_id() { echo "/subscriptions/$SUB/resourceGroups/$1"; }

mkdir -p "$WORK"
chmod 700 "$WORK"

log() { printf '\033[1;34m==>\033[0m %s\n' "$*" >&2; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# Deterministic 31 hex digits, for frontend names that look like AKS ones (a<service uid>).
hex31() { printf '%s' "$P/$1" | shasum | cut -c1-31; }

# Loads the IDs the fixtures recorded.
load_fixtures() {
  [[ -f "$FIXTURES_ENV" ]] || die "no fixtures: run ./fixtures.sh up first"
  # shellcheck source=/dev/null
  source "$FIXTURES_ENV"
}

# Records NAME=VALUE in the fixtures file, replacing an earlier value.
record() {
  local name="$1" value="$2"
  touch "$FIXTURES_ENV"
  grep -v "^$name=" "$FIXTURES_ENV" > "$FIXTURES_ENV.tmp" || true
  printf '%s=%q\n' "$name" "$value" >> "$FIXTURES_ENV.tmp"
  mv "$FIXTURES_ENV.tmp" "$FIXTURES_ENV"
  chmod 600 "$FIXTURES_ENV"
}

