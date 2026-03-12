#!/usr/bin/env bash
set -euo pipefail

# Validates that every HTTP route in router is represented in OpenAPI.
# Notes:
# - Docs endpoints are conditionally registered and can be absent in router literals.
# - We ignore /openapi.json and /docs* from strict comparison.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

ROUTER_FILE="src/interfaces/http/router.rs"
OPENAPI_FILE="src/interfaces/http/openapi.rs"

if [[ ! -f "$ROUTER_FILE" || ! -f "$OPENAPI_FILE" ]]; then
  echo "required files not found: $ROUTER_FILE or $OPENAPI_FILE" >&2
  exit 1
fi

if command -v rg >/dev/null 2>&1; then
  router_paths="$(
    rg -o '"/[^"]+"' "$ROUTER_FILE" \
      | tr -d '"' \
      | grep -vE '^/openapi\.json$' \
      | grep -vE '^/docs(/.*)?$' \
      | sort -u
  )"

  spec_paths="$(
    rg -o 'path = "/[^"]+"' "$OPENAPI_FILE" \
      | sed -E 's/path = "([^"]+)"/\1/' \
      | grep -vE '^/openapi\.json$' \
      | grep -vE '^/docs(/.*)?$' \
      | sort -u
  )"
else
  router_paths="$(
    grep -oE '"/[^"]+"' "$ROUTER_FILE" \
      | tr -d '"' \
      | grep -vE '^/openapi\.json$' \
      | grep -vE '^/docs(/.*)?$' \
      | sort -u
  )"

  spec_paths="$(
    grep -oE 'path = "/[^"]+"' "$OPENAPI_FILE" \
      | sed -E 's/path = "([^"]+)"/\1/' \
      | grep -vE '^/openapi\.json$' \
      | grep -vE '^/docs(/.*)?$' \
      | sort -u
  )"
fi

router_only="$(comm -23 <(echo "$router_paths") <(echo "$spec_paths"))"
spec_only="$(comm -13 <(echo "$router_paths") <(echo "$spec_paths"))"

if [[ -n "$router_only" || -n "$spec_only" ]]; then
  echo "openapi coverage drift detected"
  if [[ -n "$router_only" ]]; then
    echo ""
    echo "router-only paths:"
    echo "$router_only"
  fi
  if [[ -n "$spec_only" ]]; then
    echo ""
    echo "spec-only paths:"
    echo "$spec_only"
  fi
  exit 1
fi

echo "openapi coverage check passed"
