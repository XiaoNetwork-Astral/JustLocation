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
// 只共享 Android 构建工具链；系统桥接与两个 App 没有 Gradle 项目依赖。
include(":bridge", ":joystick", ":probe")
project(":bridge").projectDir = file("../../zygisk/bridge")
project(":joystick").projectDir = file("../../app/joystick")
project(":probe").projectDir = file("../../app/probe")
