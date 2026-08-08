#!/usr/bin/env bash
# Environment for building the Android target on this machine.
#
#   source scripts/android-env.sh
#   cd app && pnpm tauri android build
#
# Everything here is host setup, not project configuration — CI installs the
# same things through apt and needs none of it. It exists because building
# Android locally has three sharp edges and all three fail with errors that
# don't say what's wrong.

set -u

# --- 1. SDK and NDK ---------------------------------------------------------
# Not exported by the system anywhere, so Tauri can't find them on its own.
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export NDK_HOME="${NDK_HOME:-$ANDROID_HOME/ndk/28.2.13676358}"
export ANDROID_NDK_ROOT="$NDK_HOME"

_TC="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
export PATH="$_TC:$PATH"

# openssl-sys shells out to a C compiler and ar for the vendored build, and
# picks the host's unless told otherwise — which produces x86_64 objects that
# fail to link into an arm64 .so, several minutes later.
#
# RANLIB is the same idea with a different symptom. The `cc` crate asks for
# `<triple>-ranlib`, which the NDK stopped shipping at r23 — `llvm-ranlib` is
# the only one left. Everything compiles and archives without it, and the build
# then dies on OpenSSL's `make install_dev` with `ranlib: not found`.
export CC_aarch64_linux_android="$_TC/aarch64-linux-android24-clang"
export AR_aarch64_linux_android="$_TC/llvm-ar"
export RANLIB_aarch64_linux_android="$_TC/llvm-ranlib"
export CC_armv7_linux_androideabi="$_TC/armv7a-linux-androideabi24-clang"
export AR_armv7_linux_androideabi="$_TC/llvm-ar"
export RANLIB_armv7_linux_androideabi="$_TC/llvm-ranlib"
export CC_x86_64_linux_android="$_TC/x86_64-linux-android24-clang"
export AR_x86_64_linux_android="$_TC/llvm-ar"
export RANLIB_x86_64_linux_android="$_TC/llvm-ranlib"
export CC_i686_linux_android="$_TC/i686-linux-android24-clang"
export AR_i686_linux_android="$_TC/llvm-ar"
export RANLIB_i686_linux_android="$_TC/llvm-ranlib"

# --- 2. Java ----------------------------------------------------------------
# Gradle refuses JDK 25, which is what this box has on PATH. 17 and 21 are both
# installed; 21 is what the Android Gradle Plugin is happiest on.
for _j in /usr/lib/jvm/java-21-openjdk /usr/lib/jvm/java-17-temurin-jdk; do
    if [ -x "$_j/bin/javac" ]; then
        export JAVA_HOME="$_j"
        export PATH="$JAVA_HOME/bin:$PATH"
        break
    fi
done

# --- 3. Perl ----------------------------------------------------------------
# OpenSSL's ./Configure and its Makefile template are Perl programs, and they
# need FindBin, IPC::Cmd, version and Time::Piece. Fedora's base perl ships none
# of them, and every one fails as an openssl-sys build script error with the
# real cause several lines into stderr.
#
# The supported fix is one command:
#
#     sudo dnf install perl-FindBin perl-IPC-Cmd perl-version perl-Time-Piece
#
# `scripts/android-perl-shim.sh` is the rootless alternative and builds what the
# next line looks for. Note it contributes a *shim* directory holding only the
# handful of modules needed: putting the extracted core lib directory on
# PERL5LIB instead makes perl load a Config.pm from a different point release
# than the running interpreter, which aborts with a version-mismatch that reads
# nothing like a missing module.
_SHIM=/tmp/perlfix/shim
_VENDOR=/tmp/perlfix/ext/usr
if ! perl -e 'use FindBin; use IPC::Cmd; use version; use Time::Piece;' 2>/dev/null; then
    if [ -d "$_SHIM" ]; then
        export PERL5LIB="$_SHIM:$_VENDOR/share/perl5/vendor_perl:$_VENDOR/lib64/perl5/vendor_perl${PERL5LIB:+:$PERL5LIB}"
    fi
    perl -e 'use FindBin; use IPC::Cmd; use version; use Time::Piece;' 2>/dev/null || {
        echo "warning: OpenSSL's Configure needs perl FindBin/IPC::Cmd/version/Time::Piece." >&2
        echo "         run: sudo dnf install perl-FindBin perl-IPC-Cmd perl-version perl-Time-Piece" >&2
        echo "         or:  ./scripts/android-perl-shim.sh   (no root needed)" >&2
    }
fi

unset _TC _j _SHIM _VENDOR
echo "android env: NDK=$NDK_HOME JAVA_HOME=${JAVA_HOME:-<unset>}"
