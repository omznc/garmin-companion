#!/bin/sh
# Stop the job if R8 removed a constructor that WorkManager calls by reflection.
# Release 1.1.0 lost WorkDatabase_Impl() and crashed at start.
# Usage: check-r8-keeps.sh <seeds.txt of the release build>
set -eu
seeds=$1

for keep in \
  "androidx.work.impl.WorkDatabase_Impl: WorkDatabase_Impl()" \
  "androidx.work.OverwritingInputMerger: OverwritingInputMerger()"; do
  if ! grep -qF "$keep" "$seeds"; then
    echo "R8 removed $keep. Check extraProguardRules in apps/mobile/app.json." >&2
    exit 1
  fi
done
