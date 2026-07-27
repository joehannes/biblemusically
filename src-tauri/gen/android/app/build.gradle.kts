import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// Signing for a release APK people can actually install.
//
// Android will not install an unsigned APK — "unsigned" is not a thing users can allow, unlike an
// unknown *source*, which is one system toggle. So a release needs a key, and a self-signed one is
// perfectly adequate for sideloading.
//
// The key itself lives outside this repository, and outside the `gen/` directory that
// `tauri android init` regenerates: ~/.config/studio-lightkid/android-release.keystore. Two reasons.
// A private key in a repository is the mistake this project has already made once with the
// entitlement signing key. And Android identifies an app by its signature — an APK signed with a
// *different* key cannot upgrade one signed with the old key, it can only be installed after
// uninstalling, which loses the user's data. Losing this file means every existing install is
// stranded, so it belongs where backups reach and git does not.
val keystoreProperties = Properties().apply {
    val propFile = file("keystore.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}
val hasSigningKey = keystoreProperties.getProperty("storeFile")?.let { file(it).exists() } == true

android {
    compileSdk = 36
    namespace = "studio.lightkid.app"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        // The scheme a Google sign-in comes back on. It is the reverse of the OAuth *client id*,
        // which belongs to whoever builds this — so it arrives from the environment rather than
        // being written into the manifest. Defaulting to the app id keeps the manifest valid when
        // nobody sets it: an empty <data android:scheme=""> is a build failure, not a no-op.
        //
        //   GOOGLE_REDIRECT_SCHEME=com.googleusercontent.apps.1234-abc ./gradlew assembleDebug
        manifestPlaceholders["googleRedirectScheme"] =
            System.getenv("GOOGLE_REDIRECT_SCHEME") ?: "studio.lightkid.app.oauth"
        applicationId = "studio.lightkid.app"
        minSdk = 24
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        create("release") {
            if (hasSigningKey) {
                storeFile = file(keystoreProperties.getProperty("storeFile"))
                storePassword = keystoreProperties.getProperty("storePassword")
                keyAlias = keystoreProperties.getProperty("keyAlias")
                keyPassword = keystoreProperties.getProperty("keyPassword")
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            // Only when a key is actually present. Without one this stays unsigned rather than
            // failing the build, so a contributor with no keystore can still verify a release
            // compiles — they just cannot install what comes out, and the build says so.
            if (hasSigningKey) {
                signingConfig = signingConfigs.getByName("release")
            }
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")