plugins {
    id("com.android.application")
}

dependencies { testImplementation("junit:junit:4.13.2") }

android {
    namespace = "me.idk.justlocation.companion"
    compileSdk = 36
    defaultConfig {
        applicationId = "me.idk.justlocation.companion"
        minSdk = 35
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}
