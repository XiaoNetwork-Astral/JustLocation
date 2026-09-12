import groovy.json.JsonSlurper

plugins {
    id("com.android.application")
}

dependencies { testImplementation("junit:junit:4.13.2") }

val releaseInfo = JsonSlurper().parse(rootProject.file("../../project-config.json")) as Map<*, *>

android {
    namespace = "me.idk.justlocation.joystick"
    compileSdk = 36
    defaultConfig {
        applicationId = "me.idk.justlocation.joystick"
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
