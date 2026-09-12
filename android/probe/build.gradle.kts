plugins {
    id("com.android.application")
}

android {
    namespace = "me.idk.justlocation.probe"
    compileSdk = 36
    defaultConfig {
        applicationId = "me.idk.justlocation.probe"
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
