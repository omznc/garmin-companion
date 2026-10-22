#!/usr/bin/env bash
# Write the TypeScript types of the mobile app from the Rust wire types.
#
# The output goes to apps/mobile/src/data/generated (see .cargo/config.toml).
# It is checked in. CI runs this script and fails when the folder changes.
set -euo pipefail

cd "$(dirname "$0")/.."

# Remove the old files first, so a type that was removed leaves no stale file.
rm -rf apps/mobile/src/data/generated

# `export_bindings` matches the tests that ts-rs writes for each type, and the
# test that writes commands.ts (garmin-ffi/src/contract.rs).
cargo test \
  -p garmin-core -p garmin-api -p garmin-coach -p garmin-ffi \
  --features garmin-ffi/ts-rs \
  -- export_bindings
