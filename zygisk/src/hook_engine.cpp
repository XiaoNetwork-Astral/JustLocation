#include "hook_engine.hpp"

#include <android/log.h>
#include <shadowhook.h>

#include <lsplant.hpp>
#include <mutex>
#include <string>
#include <unordered_map>

#include "art_symbols.hpp"

namespace {
ArtSymbols symbols;
void* art_handle = nullptr;
std::mutex hooks_mutex;
std::unordered_map<void*, void*> hook_stubs;

void* hook(void* target, void* replacement) {
    __android_log_print(ANDROID_LOG_INFO, "JustLocation", "ART inline hook begin: %p", target);
    std::lock_guard lock(hooks_mutex);
    if (hook_stubs.contains(target))
        return nullptr;
    void* original = nullptr;
    void* stub = shadowhook_hook_func_addr(target, replacement, &original);
    __android_log_print(ANDROID_LOG_INFO, "JustLocation", "ART inline hook end: %p, error=%d",
                        target, shadowhook_get_errno());
    if (!stub)
        return nullptr;
    hook_stubs.emplace(target, stub);
    return original;
}

bool unhook(void* target) {
    std::lock_guard lock(hooks_mutex);
    auto found = hook_stubs.find(target);
    if (found == hook_stubs.end() || shadowhook_unhook(found->second) != 0)
        return false;
    hook_stubs.erase(found);
    return true;
}
}  // namespace

extern "C" __attribute__((visibility("default"))) bool justlocation_init_hooks(JNIEnv* env) {
    int error = shadowhook_init(SHADOWHOOK_MODE_UNIQUE, false);
    if (error != 0) {
        __android_log_print(ANDROID_LOG_ERROR, "JustLocation",
                            "ShadowHook initialization failed: %d", error);
        return false;
    }
    __android_log_print(ANDROID_LOG_INFO, "JustLocation", "ART symbols open begin");
    if (!symbols.open()) {
        __android_log_print(ANDROID_LOG_ERROR, "JustLocation", "Cannot read ART symbols");
        return false;
    }
    art_handle = shadowhook_dlopen("libart.so");
    __android_log_print(ANDROID_LOG_INFO, "JustLocation",
                        "ART symbols open complete; LSPlant init begin");
    lsplant::InitInfo info{
            .inline_hooker = hook,
            .inline_unhooker = unhook,
            .art_symbol_resolver =
                    [](std::string_view name) {
                        __android_log_print(ANDROID_LOG_DEBUG, "JustLocation", "ART resolve: %.*s",
                                            static_cast<int>(name.size()), name.data());
                        // ShadowHook's resolver also handles the compressed debug symbol table.
                        void* address =
                                art_handle ? shadowhook_dlsym(art_handle, std::string(name).c_str())
                                           : nullptr;
                        return address ? address : symbols.find(name, false);
                    },
            .art_symbol_prefix_resolver =
                    [](std::string_view name) { return symbols.find(name, true); },
            .executable_memory_allocator = {},
            .executable_memory_recycler = {},
    };
    bool initialized = lsplant::Init(env, info);
    __android_log_print(ANDROID_LOG_INFO, "JustLocation", "LSPlant init end: %d", initialized);
    return initialized;
}
