#include "io.hpp"
#include <jni.h>
#include <lsplant.hpp>
#include <android/log.h>
#include <arpa/inet.h>
#include <unistd.h>
#include <string>

extern "C" bool justlocation_init_hooks(JNIEnv* env);

namespace {
int companion_fd = -1;

jstring exchange_state(JNIEnv* env, jint installed, jint wifi_calls, const std::string* subscriptions, const std::string& extra) {
    if (companion_fd < 0) return nullptr;
    uint32_t length = 0;
    // Wi-Fi 回调计数走帧里的两个字节（大端）。为什么要它：`installed` 的那几位只说明
    // "方法挂上了"，说明不了回调有没有被走到；2026-09-12 真机验收就是靠这个计数
    // 才把"挂钩没生效"和"挂钩生效但结果被抹掉"分开的。上限 65535，够用且不撑大帧。
    unsigned calls = wifi_calls < 0 ? 0u
        : (wifi_calls > 0xFFFF ? 0xFFFFu : static_cast<unsigned>(wifi_calls));
    const char request[]{subscriptions ? 'P' : 'S', static_cast<char>(installed & 255),
        static_cast<char>((calls >> 8) & 255), static_cast<char>(calls & 255)};
    uint32_t size = subscriptions ? htonl(subscriptions->size()) : 0;
    // 透传诊断串（GNSS 原始通道的计数）。定长帧之后带一个长度前缀的字段。
    //
    // **这个长度头必须无条件写出去，哪怕内容是空的**：接收端（`companion.cpp`）是
    // 无条件先读 4 字节再决定读多少的，少发这 4 字节会让它把后面的字段整体错位——
    // 手机进程那条路（`extra` 恒为空）就是这样：companion 把订阅数据的长度头当成
    // `extra_size`、又把正文当成透传串吃掉，剩下的字节流全乱，最终读失败、连接被断。
    // 症状是"钩子装上了、心跳却永远到不了守护进程"，而 system_server 没事
    // （它的 extra 是 GNSS 计数串、从不为空）。2026-09-12 定位。
    uint32_t extra_size = htonl(static_cast<uint32_t>(extra.size()));
    if (!send_all(companion_fd, request, sizeof(request))
        || !send_all(companion_fd, &extra_size, sizeof(extra_size))
        || (!extra.empty() && !send_all(companion_fd, extra.data(), extra.size()))
        || (subscriptions && (!send_all(companion_fd, &size, sizeof(size))
            || !send_all(companion_fd, subscriptions->data(), subscriptions->size())))
        || !receive_all(companion_fd, &length, sizeof(length))) {
        close(companion_fd); companion_fd = -1; return nullptr;
    }
    length = ntohl(length);
    if (length > 131072) { close(companion_fd); companion_fd = -1; return nullptr; }
    if (!length) return nullptr;
    std::string state(length, '\0');
    if (!receive_all(companion_fd, state.data(), length)) {
        close(companion_fd); companion_fd = -1; return nullptr;
    }
    // JSON is standard UTF-8, whereas NewStringUTF expects JNI's modified UTF-8.
    auto bytes = env->NewByteArray(length);
    if (!bytes) return nullptr;
    env->SetByteArrayRegion(bytes, 0, length, reinterpret_cast<const jbyte*>(state.data()));
    auto type = env->FindClass("java/lang/String");
    auto ctor = env->GetMethodID(type, "<init>", "([BLjava/lang/String;)V");
    auto encoding = env->NewStringUTF("UTF-8");
    return static_cast<jstring>(env->NewObject(type, ctor, bytes, encoding));
}

/** 透传串：读成 std::string。空串与 null 都表示"这一轮没有透传内容"。 */
std::string read_extra(JNIEnv* env, jstring extra) {
    if (!extra) return {};
    const char* text = env->GetStringUTFChars(extra, nullptr);
    if (!text) return {};
    std::string value(text);
    env->ReleaseStringUTFChars(extra, text);
    // 解析失败就当没有透传内容：这条路不能因为一段诊断内容把状态回包弄坏。
    if (env->ExceptionCheck()) { env->ExceptionClear(); return {}; }
    return value.size() > 2048 ? value.substr(0, 2048) : value;
}

jstring read_state(JNIEnv* env, jclass, jint installed, jint wifi_calls, jstring extra) {
    return exchange_state(env, installed, wifi_calls, nullptr, read_extra(env, extra));
}
jstring read_phone_state(JNIEnv* env, jclass, jint installed, jint wifi_calls, jbyteArray metadata, jstring extra) {
    // 有界诊断：手机进程从来不写 /data/adb/justlocation，logcat 是唯一出口。
    // 只在前几次记录，避免每秒一行把环形缓冲冲掉。
    static int calls = 0;
    if (calls < 3) {
        __android_log_print(ANDROID_LOG_INFO, "JustLocation", "read_phone_state called: fd=%d", companion_fd);
        calls++;
    }
    std::string subscriptions = "null";
    if (metadata) {
        jsize size = env->GetArrayLength(metadata);
        if (size <= 0 || size > 4096) return nullptr;
        subscriptions.resize(size);
        env->GetByteArrayRegion(metadata, 0, size, reinterpret_cast<jbyte*>(subscriptions.data()));
        if (env->ExceptionCheck()) return nullptr;
    }
    // 手机进程的透传串放的是**真实运营商值**（换行分隔四项），不是 GNSS 计数——
    // 属性被我们盖住了，而运营商服务里的真值只有这个进程读得到，它是还原最可信的来源。
    return exchange_state(env, installed | 128, wifi_calls, &subscriptions, read_extra(env, extra));
}

jobject hook_method(JNIEnv* env, jclass, jobject target, jobject hooker, jobject callback) {
    return lsplant::Hook(env, target, hooker, callback);
}

jboolean deoptimize_method(JNIEnv* env, jclass, jobject target) {
    return lsplant::Deoptimize(env, target);
}

bool check(JNIEnv* env) {
    if (!env->ExceptionCheck()) return true;
    env->ExceptionDescribe(); env->ExceptionClear(); return false;
}
}

static bool start_bridge(JNIEnv* env, const void* dex, size_t length, int companion, bool phone) {
    __android_log_print(ANDROID_LOG_INFO, "JustLocation", "%s bridge start", phone ? "Phone" : "System");
    companion_fd = companion;
    auto loader_type = env->FindClass("java/lang/ClassLoader");
    if (!check(env) || !loader_type) return false;
    // Android 15 creates this loader in the forked server before specialization.
    // Do not intercept the process-wide ClassLoader path during system startup.
    jobject parent = nullptr;
    if (phone) {
        auto system_loader = env->GetStaticMethodID(loader_type, "getSystemClassLoader", "()Ljava/lang/ClassLoader;");
        if (!check(env) || !system_loader) return false;
        parent = env->CallStaticObjectMethod(loader_type, system_loader);
    } else {
    auto zygote = env->FindClass("com/android/internal/os/ZygoteInit");
    if (!check(env) || !zygote) return false;
    auto cached = env->GetStaticFieldID(zygote, "sCachedSystemServerClassLoader", "Ljava/lang/ClassLoader;");
    if (!check(env) || !cached) return false;
    parent = env->GetStaticObjectField(zygote, cached);
    }
    if (!check(env) || !parent) return false;
    if (!justlocation_init_hooks(env)) return false;
    __android_log_print(ANDROID_LOG_INFO, "JustLocation", "Bridge DEX load begin");
    auto dex_type = env->FindClass("dalvik/system/InMemoryDexClassLoader");
    if (!check(env) || !dex_type) return false;
    auto ctor = env->GetMethodID(dex_type, "<init>", "(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V");
    if (!check(env) || !ctor) return false;
    auto buffer = env->NewDirectByteBuffer(const_cast<void*>(dex), length);
    auto loader = env->NewObject(dex_type, ctor, buffer, parent);
    if (!check(env) || !loader) return false;
    if (phone) {
        // Trust this module's in-memory DEX, without changing the process hidden-API policy.
        auto base = env->FindClass("dalvik/system/BaseDexClassLoader");
        auto list_type = env->FindClass("dalvik/system/DexPathList");
        auto element_type = env->FindClass("dalvik/system/DexPathList$Element");
        auto file_type = env->FindClass("dalvik/system/DexFile");
        if (!check(env) || !base || !list_type || !element_type || !file_type) return false;
        auto path_field = env->GetFieldID(base, "pathList", "Ldalvik/system/DexPathList;");
        auto elements_field = env->GetFieldID(list_type, "dexElements", "[Ldalvik/system/DexPathList$Element;");
        auto file_field = env->GetFieldID(element_type, "dexFile", "Ldalvik/system/DexFile;");
        auto cookie_field = env->GetFieldID(file_type, "mCookie", "Ljava/lang/Object;");
        if (!check(env) || !path_field || !elements_field || !file_field || !cookie_field) return false;
        auto path = env->GetObjectField(loader, path_field);
        if (!check(env) || !path) return false;
        auto elements = static_cast<jobjectArray>(env->GetObjectField(path, elements_field));
        if (!check(env) || !elements || env->GetArrayLength(elements) == 0) return false;
        for (jsize i = 0; i < env->GetArrayLength(elements); ++i) {
            auto element = env->GetObjectArrayElement(elements, i);
            if (!check(env) || !element) return false;
            auto file = env->GetObjectField(element, file_field);
            if (!check(env) || !file) return false;
            auto cookie = env->GetObjectField(file, cookie_field);
            if (!check(env) || !cookie || !lsplant::MakeDexFileTrusted(env, cookie) || !check(env)) return false;
        }
    }
    auto load = env->GetMethodID(loader_type, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;");
    if (!check(env) || !load) return false;
    auto name = env->NewStringUTF(phone ? "me.idk.justlocation.bridge.PhoneBridge" : "me.idk.justlocation.bridge.BridgeEntry");
    auto entry = static_cast<jclass>(env->CallObjectMethod(loader, load, name));
    if (!check(env) || !entry) return false;
    JNINativeMethod natives[] = {
        {const_cast<char*>("readState"), const_cast<char*>(phone ? "(II[BLjava/lang/String;)Ljava/lang/String;" : "(IILjava/lang/String;)Ljava/lang/String;"),
            phone ? reinterpret_cast<void*>(read_phone_state) : reinterpret_cast<void*>(read_state)},
        {const_cast<char*>("hook"), const_cast<char*>("(Ljava/lang/reflect/Method;Ljava/lang/Object;Ljava/lang/reflect/Method;)Ljava/lang/reflect/Method;"), reinterpret_cast<void*>(hook_method)},
        {const_cast<char*>("deoptimize"), const_cast<char*>("(Ljava/lang/reflect/Method;)Z"), reinterpret_cast<void*>(deoptimize_method)},
    };
    if (env->RegisterNatives(entry, natives, phone ? 3 : 2) != JNI_OK || !check(env)) return false;
    auto start = env->GetStaticMethodID(entry, "start", "(Ljava/lang/ClassLoader;)V");
    if (!check(env) || !start) return false;
    env->CallStaticVoidMethod(entry, start, parent);
    __android_log_print(ANDROID_LOG_INFO, "JustLocation", "Bridge entry returned");
    return check(env);
}

extern "C" __attribute__((visibility("default")))
bool justlocation_start(JNIEnv* env, const void* dex, size_t length, int companion) {
    return start_bridge(env, dex, length, companion, false);
}

extern "C" __attribute__((visibility("default")))
bool justlocation_start_phone(JNIEnv* env, const void* dex, size_t length, int companion) {
    return start_bridge(env, dex, length, companion, true);
}
