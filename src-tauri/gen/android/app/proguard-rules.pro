# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile

# ── Mobile plugins are found by name at runtime ───────────────────────────────
#
# Tauri instantiates an Android plugin reflectively: Rust passes the class path
# "studio/lightkid/app/InstallPlugin" across JNI and the runtime looks it up, then calls the method
# whose @Command name matches the one the frontend invoked. R8 sees none of that — no Java or Kotlin
# code refers to this class — so in a release build it renames or removes it and the plugin fails to
# register with a ClassNotFoundException, on release only, in a build that compiled cleanly.
#
# The annotations have to survive too: they are how the command names are resolved.
-keep class studio.lightkid.app.InstallPlugin { *; }
-keep class studio.lightkid.app.InstallArgs { *; }
-keep @app.tauri.annotation.TauriPlugin class * { *; }
-keepclassmembers class * {
  @app.tauri.annotation.Command <methods>;
}
-keepattributes *Annotation*