#pragma once

#include <android/dlext.h>
#include <android/log.h>
#include <dlfcn.h>
#include <fcntl.h>
#include <unistd.h>

inline void* load_library(int directory, const char* name) {
    int fd = openat(directory, name, O_RDONLY | O_CLOEXEC);
    if (fd < 0) {
        __android_log_print(ANDROID_LOG_ERROR, "JustLocation", "Cannot open %s", name);
        return nullptr;
    }
    android_dlextinfo info{};
    info.flags = ANDROID_DLEXT_USE_LIBRARY_FD;
    info.library_fd = fd;
    void* handle = android_dlopen_ext(name, RTLD_NOW | RTLD_GLOBAL | RTLD_NODELETE, &info);
    close(fd);
    if (!handle) __android_log_print(ANDROID_LOG_ERROR, "JustLocation", "dlopen %s: %s", name, dlerror());
    return handle;
}

// Called only in the system_server child, before specialization drops access to
// the module directory. ShadowHook must load/unload its probe library during init;
// preloading that library would skip the constructor scan that it needs.
inline bool prepare_shadowhook(int directory, bool debug = false) {
    void* shadow = load_library(directory, "libshadowhook.so");
    if (!shadow) return false;
    auto init = reinterpret_cast<int (*)(int, bool)>(dlsym(shadow, "shadowhook_init"));
    if (!init) return false;
    int error = init(1, debug); // SHADOWHOOK_MODE_UNIQUE (avoid a link dependency in the entry).
    if (error) __android_log_print(ANDROID_LOG_ERROR, "JustLocation", "ShadowHook initialization failed: %d", error);
    return error == 0;
}
