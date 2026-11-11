plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

val ferriteVersion: String = providers.gradleProperty("ferriteVersion").get()
val versionParts = ferriteVersion.split(".").map { it.toInt() }

android {
    namespace = "se.ferrite.browser"
    compileSdk = 35

    defaultConfig {
        applicationId = "se.ferrite.browser"
        minSdk = 26
        targetSdk = 35
        versionName = ferriteVersion
        versionCode = versionParts[0] * 1_000_000 + versionParts[1] * 1_000 + versionParts[2]
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        buildConfigField("String", "UPDATE_REPO", "\"felixarvid2/test\"")
    }

    // CI signs with the repository's keystore secrets or a throw-away key
    // (see README.md); without FERRITE_KEYSTORE the release APK is unsigned.
    val keystore = System.getenv("FERRITE_KEYSTORE")
    if (keystore != null) {
        signingConfigs {
            create("release") {
                storeFile = file(keystore)
                storePassword = System.getenv("FERRITE_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("FERRITE_KEY_ALIAS")
                keyPassword = System.getenv("FERRITE_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            if (keystore != null) signingConfig = signingConfigs.getByName("release")
        }
    }

    buildFeatures {
        buildConfig = true
        viewBinding = false
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets {
        getByName("main") {
            // The scriptlet resources and their licence are shared with the desktop browser.
            assets.srcDir("../../resources/adblock")
            kotlin.srcDir("src/main/kotlin")
        }
        getByName("test") { kotlin.srcDir("src/test/kotlin") }
        getByName("androidTest") { kotlin.srcDir("src/androidTest/kotlin") }
    }

    packaging {
        jniLibs.useLegacyPackaging = false
    }

    lint {
        abortOnError = true
        warningsAsErrors = false
        checkReleaseBuilds = true
    }
}

kotlin {
    compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) }
}

dependencies {
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.webkit:webkit:1.12.1")
    implementation("androidx.recyclerview:recyclerview:1.3.2")
    implementation("androidx.swiperefreshlayout:swiperefreshlayout:1.1.0")
    implementation("androidx.preference:preference-ktx:1.2.1")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20240303")

    androidTestImplementation("androidx.test:core-ktx:1.6.1")
    androidTestImplementation("androidx.test.ext:junit-ktx:1.2.1")
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test:rules:1.6.1")
}
