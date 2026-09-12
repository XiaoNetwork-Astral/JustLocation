cmake_minimum_required(VERSION 4.0.2)
project(justlocation LANGUAGES C CXX)

if(NOT ANDROID)
    message(FATAL_ERROR "Configure with the Android NDK toolchain.")
endif()

add_library(justlocation SHARED src/main.cpp src/companion.cpp)
target_include_directories(justlocation PRIVATE include)
target_compile_features(justlocation PRIVATE cxx_std_17)
target_compile_options(justlocation PRIVATE
    -Wall -Wextra -Werror -fno-exceptions -fno-rtti
    -fvisibility=hidden -fvisibility-inlines-hidden)
target_link_options(justlocation PRIVATE -Wl,-z,max-page-size=16384)
target_link_libraries(justlocation PRIVATE log dl android)

include(FetchContent)
set(LSPLANT_BUILD_SHARED OFF CACHE BOOL "" FORCE)
FetchContent_Declare(lsplant
    GIT_REPOSITORY https://github.com/LSPosed/LSPlant.git
    GIT_TAG b2f9279014091c83faac094c5fc7c5498261ace8
    GIT_SUBMODULES lsplant/src/main/jni/external/dex_builder
    GIT_CONFIG http.sslBackend=openssl
    SOURCE_SUBDIR lsplant/src/main/jni)
FetchContent_Declare(shadowhook
    GIT_REPOSITORY https://github.com/bytedance/android-inline-hook.git
    GIT_TAG 854c775c2c3676e57a0f383597ebf420b5204161
    GIT_SUBMODULES ""
    GIT_CONFIG http.sslBackend=openssl
    SOURCE_SUBDIR shadowhook/src/main/cpp)
FetchContent_MakeAvailable(lsplant shadowhook)
include(cmake/shadowhook-arm64.cmake)

# Upstream enables every Clang diagnostic, including warnings introduced after its release.
# Use the project's normal warning groups with NDK 29 while retaining warnings-as-errors.
foreach(dependency shadowhook shadowhook_nothing)
    get_target_property(dependency_options ${dependency} COMPILE_OPTIONS)
    list(REMOVE_ITEM dependency_options -Weverything)
    set_target_properties(${dependency} PROPERTIES COMPILE_OPTIONS "${dependency_options}")
    target_compile_options(${dependency} PRIVATE -Wall -Wextra)
endforeach()
# The upstream export map lists both ARM32 and ARM64 assembly entry points.
target_link_options(shadowhook PRIVATE -Wl,--undefined-version)
# Android resolves $ORIGIN against the real path of an fd-loaded ELF. Allow the
# upstream linker scan to open its sibling without changing process search paths.
target_link_options(shadowhook PRIVATE "-Wl,-rpath,$ORIGIN")

# Kept separate from the tiny Zygisk entry: ordinary applications never load this library.
add_library(justlocation_runtime SHARED src/hook_engine.cpp src/art_symbols.cpp src/runtime.cpp src/bridge_jni.cpp src/step_hooks.cpp)
target_compile_features(justlocation_runtime PRIVATE cxx_std_23)
target_include_directories(justlocation_runtime PRIVATE
    ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/include)
target_link_libraries(justlocation_runtime PRIVATE lsplant_static shadowhook log dl)
target_compile_options(justlocation_runtime PRIVATE -Wall -Wextra -Werror -fvisibility=hidden)
target_link_options(justlocation_runtime PRIVATE -Wl,-z,max-page-size=16384)

# Explicit device-test target only; not built by default or included in module ZIPs.
add_executable(justlocation_loader_probe EXCLUDE_FROM_ALL tests/loader.cpp)
add_executable(justlocation_veneer_probe EXCLUDE_FROM_ALL tests/veneer.cpp)
target_include_directories(justlocation_veneer_probe PRIVATE ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/include)
target_link_libraries(justlocation_veneer_probe PRIVATE shadowhook)
add_executable(justlocation_transport_probe EXCLUDE_FROM_ALL tests/transport.cpp)
target_include_directories(justlocation_transport_probe PRIVATE src)
target_include_directories(justlocation_loader_probe PRIVATE src)
target_link_libraries(justlocation_loader_probe PRIVATE dl log)
add_library(justlocation_bootstrap_probe SHARED EXCLUDE_FROM_ALL tests/bootstrap.cpp)
target_include_directories(justlocation_bootstrap_probe PRIVATE src)
target_link_libraries(justlocation_bootstrap_probe PRIVATE dl log)
add_library(justlocation_probe SHARED EXCLUDE_FROM_ALL
    tests/probe.cpp src/hook_engine.cpp src/art_symbols.cpp)
target_compile_features(justlocation_probe PRIVATE cxx_std_23)
target_include_directories(justlocation_probe PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/src
    ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/include)
add_dependencies(justlocation_probe justlocation_runtime justlocation_loader_probe justlocation_bootstrap_probe justlocation_veneer_probe)
target_link_libraries(justlocation_probe PRIVATE lsplant_static shadowhook log dl)
target_compile_options(justlocation_probe PRIVATE -Wall -Wextra -Werror -fvisibility=hidden)
target_link_options(justlocation_probe PRIVATE -Wl,-z,max-page-size=16384)
