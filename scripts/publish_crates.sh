#!/usr/bin/env bash
set -euo pipefail

# Publishes all Strata workspace crates to crates.io in topological order,
# then publishes strata-earth from the sibling earth workspace.
# Requires: `cargo login` (or CARGO_REGISTRY_TOKEN env var).

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STRATA_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
EARTH_ROOT="$(cd "$STRATA_ROOT/../earth" && pwd)"

echo "Publishing Strata workspace crates from $STRATA_ROOT..."
cd "$STRATA_ROOT"
cargo publish --workspace "$@"

if [[ -d "$EARTH_ROOT" ]]; then
  echo "Publishing strata-earth from $EARTH_ROOT..."
  cd "$EARTH_ROOT"
  cargo publish -p strata-earth "$@"
fi
