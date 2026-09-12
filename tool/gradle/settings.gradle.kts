pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "JustLocation"

// Share the Android toolchain without dependencies between the bridge and apps.
include(":bridge", ":joystick", ":probe")

project(":bridge").projectDir = file("../../zygisk/bridge")

project(":joystick").projectDir = file("../../app/joystick")

project(":probe").projectDir = file("../../app/probe")
