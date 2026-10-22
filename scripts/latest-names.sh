#!/bin/sh
# Copy each installer to a name without the version, so the README can link to
# releases/latest/download/<name>. Print the paths of the copies.
# Usage: latest-names.sh <dir> <version>
set -eu
dir=$1
v=$2

copy() {
  cp "$dir/$1" "$dir/$2"
  echo "$dir/$2"
}

copy "garmin-companion_${v}_x64-setup.exe" garmin-companion_x64-setup.exe
copy "garmin-companion_${v}_aarch64.dmg" garmin-companion_aarch64.dmg
copy "garmin-companion_${v}_x64.dmg" garmin-companion_x64.dmg
copy "garmin-companion_${v}_amd64.deb" garmin-companion_amd64.deb
copy "garmin-companion-${v}-1.x86_64.rpm" garmin-companion.x86_64.rpm
copy "garmin-companion_${v}_amd64.AppImage" garmin-companion_amd64.AppImage
copy "garmin-companion_${v}.apk" garmin-companion.apk
copy "garmin-companion_${v}_unsigned.ipa" garmin-companion_unsigned.ipa
