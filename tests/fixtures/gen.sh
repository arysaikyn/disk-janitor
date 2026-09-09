#!/usr/bin/env bash
# Regenerate the SPEC §7 fixture trees (see tests/fixtures/README.md).
# Usage: gen.sh <root> [--set <name>] [--scale N] [--force]
set -euo pipefail
exec cargo run --quiet --bin fixture-gen -- "$@"
