import groovy.json.JsonSlurper
import java.util.Properties

plugins {
    id("com.android.application")
}

dependencies {
    implementation("com.amap.api:location:11.2.100")
}

val releaseInfo = JsonSlurper().parse(rootProject.file("../../project-config.json")) as Map<*, *>
val localSettings = Properties().apply {
    rootProject.file("local.properties").takeIf { it.isFile }?.inputStream()?.use { load(it) }
}

android {
    namespace = "me.idk.justlocation.probe"
    compileSdk = 36
    // An Android SDK key is tied to this certificate; keep local test builds consistent.
    localSettings.getProperty("probe.debug.keystore")?.let { path ->
        signingConfigs.getByName("debug").storeFile = file(path)
    }
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
