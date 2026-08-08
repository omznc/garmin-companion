# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# The bridge that hands the webview Android's dynamic colours, and takes back
# which way round to draw the system bar icons. It is only ever called by name
# from JavaScript, so nothing in the bytecode references it and R8 would strip
# the methods out of a release build — leaving Material You working in debug and
# silently absent in the APK anyone actually installs.
-keepclassmembers class com.omznc.garmincompanion.SystemPalette {
    @android.webkit.JavascriptInterface <methods>;
}

# The other half of the same arrangement: the bridge that hands a downloaded
# APK to the system installer. Reached only by name from `lib/android.ts`, so
# R8 has the same reason to remove it — and stripping this one would leave the
# release build, the only build that can actually be updated, as the one that
# silently can't be.
-keepclassmembers class com.omznc.garmincompanion.ApkInstaller {
    @android.webkit.JavascriptInterface <methods>;
}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile