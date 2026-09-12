import groovy.json.JsonSlurper

plugins {
    id("com.android.application")
}

val releaseInfo = JsonSlurper().parse(rootProject.file("../../project-config.json")) as Map<*, *>

android {
    namespace = "me.idk.justlocation.probe"
    compileSdk = 36
    defaultConfig {
        applicationId = "me.idk.justlocation.probe"
        minSdk = 35
        targetSdk = 35
        versionCode = (releaseInfo["versionCode"] as Number).toInt()
        versionName = releaseInfo["version"] as String
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}
