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
// :joystick 是模块自带的附属 App（随模块 ZIP 分发、由安装脚本装上），没有启动器与页面；
// :probe 是逐通道自检用的独立 App，不进模块包，需要验收时自己装。
include(":bridge", ":joystick", ":probe")
