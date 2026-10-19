#!/bin/sh
# Build garmin-ffi for Android and make the bindings.
# Needs: rustup targets aarch64-linux-android, armv7-linux-androideabi and x86_64-linux-android, cargo-ndk, NDK 28.2.
set -eu
cd "$(dirname "$0")/../packages/garmin-bridge"

SDK="${ANDROID_HOME:-$HOME/Android/Sdk}"
NDK="${ANDROID_NDK_ROOT:-$SDK/ndk/28.2.13676358}"
export ANDROID_HOME="$SDK" ANDROID_NDK_ROOT="$NDK" ANDROID_NDK_HOME="$NDK" NDK_HOME="$NDK"

# The vendored OpenSSL needs RANLIB. The NDK has only llvm-ranlib.
# release-rewrite.yml sets the same fix.
LLVM_BIN="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
export RANLIB_aarch64_linux_android="$LLVM_BIN/llvm-ranlib"
export RANLIB_x86_64_linux_android="$LLVM_BIN/llvm-ranlib"
export RANLIB_armv7_linux_androideabi="$LLVM_BIN/llvm-ranlib"

# The OpenSSL build runs a Perl script. Fedora splits some core Perl modules
# into packages. Set PERL_EXTRA to a folder with them if "dnf install" is not possible.
if [ -n "${PERL_EXTRA:-}" ]; then
  export PERL5LIB="$PERL_EXTRA/usr/lib64/perl5:$PERL_EXTRA/usr/lib64/perl5/vendor_perl:$PERL_EXTRA/usr/share/perl5:$PERL_EXTRA/usr/share/perl5/vendor_perl${PERL5LIB:+:$PERL5LIB}"
fi

../../node_modules/.bin/ubrn build android --and-generate --config ubrn.config.yaml --profile release-mobile "$@"

# The generated build.gradle names proguard-rules.pro. A release build fails
# when the file is missing, and ubrn does not always write it. An empty file is enough.
mkdir -p android
[ -f android/proguard-rules.pro ] || : > android/proguard-rules.pro
