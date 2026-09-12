import groovy.json.JsonSlurper

plugins {
    id("com.android.library")
}

dependencies {
    testImplementation("junit:junit:4.13.2")
}

val releaseInfo = JsonSlurper().parse(rootProject.file("../../project-config.json")) as Map<*, *>

android {
    buildFeatures { buildConfig = true }
    namespace = "me.idk.justlocation.bridge"
    compileSdk = 36
    defaultConfig {
        minSdk = 35
        buildConfigField("String", "MODULE_VERSION", "\"${releaseInfo["version"]}\"")
        testInstrumentationRunner = "me.idk.justlocation.bridge.TelephonyObjectChecks"
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}
