plugins {
    id("com.android.library")
}

dependencies {
    testImplementation("junit:junit:4.13.2")
}

android {
    namespace = "me.idk.justlocation.bridge"
    compileSdk = 36
    defaultConfig {
        minSdk = 35
        testInstrumentationRunner = "me.idk.justlocation.bridge.TelephonyObjectChecks"
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}
