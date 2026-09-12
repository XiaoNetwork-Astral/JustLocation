#include <android/log.h>
#include <jni.h>

#include <lsplant.hpp>

#include "bridge_jni.hpp"
#include "hook_engine.hpp"

namespace {
bool check(JNIEnv* env) {
    if (!env->ExceptionCheck())
        return true;
    env->ExceptionDescribe();
    env->ExceptionClear();
    return false;
}
}  // namespace

static bool start_bridge(JNIEnv* env, const void* dex, size_t length, int companion, bool phone) {
    __android_log_print(ANDROID_LOG_INFO, "JustLocation", "%s bridge start",
                        phone ? "Phone" : "System");
    auto loader_type = env->FindClass("java/lang/ClassLoader");
    if (!check(env) || !loader_type)
        return false;
    // Android 15 creates this loader in the forked server before specialization.
    // Do not intercept the process-wide ClassLoader path during system startup.
    jobject parent = nullptr;
    if (phone) {
        auto system_loader = env->GetStaticMethodID(loader_type, "getSystemClassLoader",
                                                    "()Ljava/lang/ClassLoader;");
        if (!check(env) || !system_loader)
            return false;
        parent = env->CallStaticObjectMethod(loader_type, system_loader);
    } else {
        auto zygote = env->FindClass("com/android/internal/os/ZygoteInit");
        if (!check(env) || !zygote)
            return false;
        auto cached = env->GetStaticFieldID(zygote, "sCachedSystemServerClassLoader",
                                            "Ljava/lang/ClassLoader;");
        if (!check(env) || !cached)
            return false;
        parent = env->GetStaticObjectField(zygote, cached);
    }
    if (!check(env) || !parent)
        return false;
    if (!justlocation_init_hooks(env))
        return false;
    __android_log_print(ANDROID_LOG_INFO, "JustLocation", "Bridge DEX load begin");
    auto dex_type = env->FindClass("dalvik/system/InMemoryDexClassLoader");
    if (!check(env) || !dex_type)
        return false;
    auto ctor =
            env->GetMethodID(dex_type, "<init>", "(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V");
    if (!check(env) || !ctor)
        return false;
    auto buffer = env->NewDirectByteBuffer(const_cast<void*>(dex), length);
    auto loader = env->NewObject(dex_type, ctor, buffer, parent);
    if (!check(env) || !loader)
        return false;
    if (phone) {
        // Trust this module's in-memory DEX, without changing the process hidden-API policy.
        auto base = env->FindClass("dalvik/system/BaseDexClassLoader");
        auto list_type = env->FindClass("dalvik/system/DexPathList");
        auto element_type = env->FindClass("dalvik/system/DexPathList$Element");
        auto file_type = env->FindClass("dalvik/system/DexFile");
        if (!check(env) || !base || !list_type || !element_type || !file_type)
            return false;
        auto path_field = env->GetFieldID(base, "pathList", "Ldalvik/system/DexPathList;");
        auto elements_field =
                env->GetFieldID(list_type, "dexElements", "[Ldalvik/system/DexPathList$Element;");
        auto file_field = env->GetFieldID(element_type, "dexFile", "Ldalvik/system/DexFile;");
        auto cookie_field = env->GetFieldID(file_type, "mCookie", "Ljava/lang/Object;");
        if (!check(env) || !path_field || !elements_field || !file_field || !cookie_field)
            return false;
        auto path = env->GetObjectField(loader, path_field);
        if (!check(env) || !path)
            return false;
        auto elements = static_cast<jobjectArray>(env->GetObjectField(path, elements_field));
        if (!check(env) || !elements || env->GetArrayLength(elements) == 0)
            return false;
        for (jsize i = 0; i < env->GetArrayLength(elements); ++i) {
            auto element = env->GetObjectArrayElement(elements, i);
            if (!check(env) || !element)
                return false;
            auto file = env->GetObjectField(element, file_field);
            if (!check(env) || !file)
                return false;
            auto cookie = env->GetObjectField(file, cookie_field);
            if (!check(env) || !cookie || !lsplant::MakeDexFileTrusted(env, cookie) || !check(env))
                return false;
        }
    }
    auto load = env->GetMethodID(loader_type, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;");
    if (!check(env) || !load)
        return false;
    auto name = env->NewStringUTF(phone ? "me.idk.justlocation.bridge.PhoneBridge"
                                        : "me.idk.justlocation.bridge.BridgeEntry");
    auto entry = static_cast<jclass>(env->CallObjectMethod(loader, load, name));
    if (!check(env) || !entry)
        return false;
    if (!register_bridge_natives(env, entry, companion, phone) || !check(env))
        return false;
    auto start = env->GetStaticMethodID(entry, "start", "(Ljava/lang/ClassLoader;)V");
    if (!check(env) || !start)
        return false;
    env->CallStaticVoidMethod(entry, start, parent);
    __android_log_print(ANDROID_LOG_INFO, "JustLocation", "Bridge entry returned");
    return check(env);
}

extern "C" __attribute__((visibility("default"))) bool justlocation_start(JNIEnv* env,
                                                                          const void* dex,
                                                                          size_t length,
                                                                          int companion) {
    return start_bridge(env, dex, length, companion, false);
}

extern "C" __attribute__((visibility("default"))) bool justlocation_start_phone(JNIEnv* env,
                                                                                const void* dex,
                                                                                size_t length,
                                                                                int companion) {
    return start_bridge(env, dex, length, companion, true);
}
