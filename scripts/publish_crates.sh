#!/usr/bin/env bash
set -euo pipefail

# Idempotently publishes all unpublished Strata workspace crates to crates.io,
# then publishes strata-earth from the sibling earth workspace.
# Requires: `cargo login` (or CARGO_REGISTRY_TOKEN env var).

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STRATA_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
EARTH_ROOT="$(cd "$STRATA_ROOT/../earth" && pwd)"

EXCLUDES=()
while IFS= read -r crate_name; do
  [[ -n "$crate_name" ]] && EXCLUDES+=("--exclude" "$crate_name")
done < <(python3 -c '
import glob, tomllib, urllib.request, urllib.error, json

base = "'"$STRATA_ROOT"'"
for p in sorted(glob.glob(f"{base}/crates/**/Cargo.toml", recursive=True)):
    with open(p, "rb") as f:
        d = tomllib.load(f)
    pkg = d.get("package", {})
    if pkg.get("publish") is False:
        continue
    name = pkg.get("name")
    ver = pkg.get("version")
    if isinstance(ver, dict):
        ver = "0.1.0"
    url = f"https://crates.io/api/v1/crates/{name}"
    req = urllib.request.Request(url, headers={"User-Agent": "strata-publish/1.0"})
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read().decode())
            versions = [v["num"] for v in data.get("versions", [])]
            if ver in versions:
                print(name)
    except urllib.error.HTTPError as e:
        if e.code != 404:
            pass
    except Exception:
        pass
')

echo "Publishing remaining Strata workspace crates from $STRATA_ROOT..."
if [[ ${#EXCLUDES[@]} -gt 0 ]]; then
  echo "Skipping already-published crates (${#EXCLUDES[@]} args)..."
fi

cd "$STRATA_ROOT"
cargo publish --workspace "${EXCLUDES[@]}" "$@"

if [[ -d "$EARTH_ROOT" ]]; then
  echo "Publishing strata-earth from $EARTH_ROOT..."
  cd "$EARTH_ROOT"
  cargo publish -p strata-earth "$@"
fi
