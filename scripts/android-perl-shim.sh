#!/usr/bin/env bash
# Assemble the Perl modules OpenSSL's build needs, without root.
#
# Only useful on a Fedora box where you can't or won't run:
#
#     sudo dnf install perl-FindBin perl-IPC-Cmd perl-version perl-Time-Piece
#
# That command is strictly better. This exists because the machine this was
# first built on couldn't run it, and rediscovering which four modules OpenSSL
# wants — one build failure at a time, each one a five-minute round trip — is
# not worth anybody's afternoon twice.
#
# Downloads the same RPMs dnf would install, unpacks them somewhere harmless,
# and assembles a shim directory that `scripts/android-env.sh` puts on PERL5LIB.
set -euo pipefail

WORK=/tmp/perlfix
EXT="$WORK/ext"
SHIM="$WORK/shim"

PKGS=(perl-FindBin perl-IPC-Cmd perl-version perl-Time-Piece)

mkdir -p "$WORK"
cd "$WORK"

echo "==> downloading: ${PKGS[*]}"
# --resolve pulls Params::Check, Module::Load::Conditional and
# Locale::Maketext::Simple, which IPC::Cmd needs and which are packaged apart
# from it.
dnf download --resolve --alldeps "${PKGS[@]}" >/dev/null

echo "==> unpacking"
rm -rf "$EXT" && mkdir -p "$EXT"
cd "$EXT"
for f in "$WORK"/*.rpm; do
    rpm2cpio "$f" | cpio -idmu 2>/dev/null || true
    # The filesystem package lays down /usr/lib64 mode 0555, which silently
    # blocks every later archive from writing a perl5 tree into it. Re-open it
    # after each one rather than reasoning about extraction order.
    chmod -R u+w usr 2>/dev/null || true
done

echo "==> assembling shim"
rm -rf "$SHIM"
mkdir -p "$SHIM/Time" "$SHIM/auto/Time/Piece" "$SHIM/Locale/Maketext"

# Copied one file at a time on purpose. These live in the *core* lib directory
# alongside Config.pm, and putting that whole directory on PERL5LIB makes perl
# abort with "Perl lib version doesn't match executable" whenever the downloaded
# RPMs are a different point release than the installed interpreter — which they
# usually are.
cp "$EXT/usr/lib64/perl5/Time/Piece.pm"                  "$SHIM/Time/"
cp "$EXT/usr/lib64/perl5/Time/Seconds.pm"                "$SHIM/Time/"
cp "$EXT/usr/lib64/perl5/auto/Time/Piece/Piece.so"       "$SHIM/auto/Time/Piece/"
cp "$EXT/usr/share/perl5/FindBin.pm"                     "$SHIM/"
cp "$EXT/usr/share/perl5/Locale/Maketext/Simple.pm"      "$SHIM/Locale/Maketext/"

export PERL5LIB="$SHIM:$EXT/usr/share/perl5/vendor_perl:$EXT/usr/lib64/perl5/vendor_perl"
if perl -e 'use FindBin; use IPC::Cmd; use version; use Time::Piece;' 2>/dev/null; then
    echo "==> ok — now: source scripts/android-env.sh"
else
    echo "==> failed: modules still not loadable; install the RPMs properly" >&2
    exit 1
fi
