#include "bridge_jni.hpp"

#include <android/log.h>
#include <arpa/inet.h>
#include <unistd.h>

#include <cstdio>
#include <ctime>
#include <lsplant.hpp>
#include <string>

#include "io.hpp"
#include "step_hooks.hpp"

namespace {
int companion_fd = -1;

// The handshake happens once per injection, so a discarded descriptor can never be replaced in
// this process. Every later heartbeat then returns null, the Java snapshots expire at 20 seconds
// and apps silently see real output again. Record why that happened instead of failing mute.
long companion_failures = 0;
long companion_last_failure_ms = 0;
const char* companion_failure = "none";

long monotonic_ms() {
    timespec time{};
    clock_gettime(CLOCK_MONOTONIC, &time);
    return static_cast<long>(time.tv_sec) * 1000 + time.tv_nsec / 1000000;
}

/** Persist the reason and its time; the periodic heartbeat also logs the transition. */
void note_failure(const char* reason) {
    int error = errno;
    ++companion_failures;
    companion_last_failure_ms = monotonic_ms();
    companion_failure = reason;
    __android_log_print(
            ANDROID_LOG_ERROR, "JustLocation",
            "Companion state exchange failed: reason=%s errno=%d failures=%ld; the bridge "
            "cannot rebuild the descriptor and will restore real output when the "
            "current snapshot expires",
            reason, error, companion_failures);
}

jstring exchange_state(JNIEnv* env, jint installed, jint wifi_calls,
                       const std::string* subscriptions, const std::string& extra, char op = 'S') {
    if (companion_fd < 0) {
        // Keep the first failure and its time instead of overwriting it on every poll.
        if (!companion_failures) {
            errno = EBADF;
            note_failure("descriptor unavailable");
        }
        return nullptr;
    }
    uint32_t length = 0;
    // Encode Wi-Fi callback counts as a saturated, big-endian 16-bit value, separate from
    // readiness flags.
    unsigned calls = wifi_calls < 0
                             ? 0u
                             : (wifi_calls > 0xFFFF ? 0xFFFFu : static_cast<unsigned>(wifi_calls));
    const char request[]{subscriptions ? 'P' : op, static_cast<char>(installed & 255),
                         static_cast<char>((calls >> 8) & 255), static_cast<char>(calls & 255)};
    uint32_t size = subscriptions ? htonl(subscriptions->size()) : 0;
    // Always write the extra-field length, including zero. The companion reads it before any
    // optional subscription payload.
    uint32_t extra_size = htonl(static_cast<uint32_t>(extra.size()));
    const char* reason = nullptr;
    errno = 0;
    if (!send_all(companion_fd, request, sizeof(request)) ||
        !send_all(companion_fd, &extra_size, sizeof(extra_size)) ||
        (!extra.empty() && !send_all(companion_fd, extra.data(), extra.size())) ||
        (subscriptions &&
         (!send_all(companion_fd, &size, sizeof(size)) ||
          !send_all(companion_fd, subscriptions->data(), subscriptions->size())))) {
        reason = "request write failed";
    } else if (!receive_all(companion_fd, &length, sizeof(length))) {
        reason = "reply header read failed";
    }
    if (reason) {
        note_failure(reason);
        close(companion_fd);
        companion_fd = -1;
        return nullptr;
    }
    length = ntohl(length);
    if (length > 131072) {
        note_failure("reply exceeds the size limit");
        close(companion_fd);
        companion_fd = -1;
        return nullptr;
    }
    // A zero-length reply reports no state without breaking the connection.
    if (!length)
        return nullptr;
    std::string state(length, '\0');
    if (!receive_all(companion_fd, state.data(), length)) {
        note_failure("reply body read failed");
        close(companion_fd);
        companion_fd = -1;
        return nullptr;
    }
    // JSON is standard UTF-8, whereas NewStringUTF expects JNI's modified UTF-8.
    auto bytes = env->NewByteArray(length);
    if (!bytes)
        return nullptr;
    env->SetByteArrayRegion(bytes, 0, length, reinterpret_cast<const jbyte*>(state.data()));
    auto type = env->FindClass("java/lang/String");
    auto ctor = env->GetMethodID(type, "<init>", "([BLjava/lang/String;)V");
    auto encoding = env->NewStringUTF("UTF-8");
    return static_cast<jstring>(env->NewObject(type, ctor, bytes, encoding));
}

/** Convert the optional diagnostic field; null and empty both mean no content. */
std::string read_extra(JNIEnv* env, jstring extra) {
    if (!extra)
        return {};
    const char* text = env->GetStringUTFChars(extra, nullptr);
    if (!text)
        return {};
    std::string value(text);
    env->ReleaseStringUTFChars(extra, text);
    // Discard invalid diagnostic text without invalidating the state response.
    if (env->ExceptionCheck()) {
        env->ExceptionClear();
        return {};
    }
    return value.size() > 2048 ? value.substr(0, 2048) : value;
}

jstring read_state(JNIEnv* env, jclass, jint installed, jint wifi_calls, jstring extra) {
    auto report = exchange_state(env, step_hooks_ready() ? 1 : 0, 0, nullptr,
                                 std::to_string(step_event_count()), 'T');
    if (report)
        env->DeleteLocalRef(report);
    return exchange_state(env, installed, wifi_calls, nullptr, read_extra(env, extra));
}

jboolean install_steps(JNIEnv*, jclass) {
    return install_step_hooks();
}

void update_steps(JNIEnv* env, jclass, jboolean active, jboolean all, jobjectArray packages,
                  jlong total, jlong epoch, jintArray handles, jintArray types,
                  jboolean motion_active, jfloatArray motion_accelerometer,
                  jfloatArray motion_gyroscope) {
    update_step_state(env, active, all, packages, total, epoch, handles, types, motion_active,
                      motion_accelerometer, motion_gyroscope);
}
jstring read_phone_state(JNIEnv* env, jclass, jint installed, jint wifi_calls, jbyteArray metadata,
                         jstring extra) {
    // Limit early phone-state diagnostics to avoid flooding logcat.
    static int calls = 0;
    if (calls < 3) {
        __android_log_print(ANDROID_LOG_INFO, "JustLocation", "read_phone_state called: fd=%d",
                            companion_fd);
        calls++;
    }
    std::string subscriptions = "null";
    if (metadata) {
        jsize size = env->GetArrayLength(metadata);
        if (size <= 0 || size > 4096)
            return nullptr;
        subscriptions.resize(size);
        env->GetByteArrayRegion(metadata, 0, size, reinterpret_cast<jbyte*>(subscriptions.data()));
        if (env->ExceptionCheck())
            return nullptr;
    }
    // The phone process supplies operator metadata in the extra field rather than GNSS counters.
    return exchange_state(env, installed | 128, wifi_calls, &subscriptions, read_extra(env, extra));
}

jobject hook_method(JNIEnv* env, jclass, jobject target, jobject hooker, jobject callback) {
    return lsplant::Hook(env, target, hooker, callback);
}

jboolean deoptimize_method(JNIEnv* env, jclass, jobject target) {
    return lsplant::Deoptimize(env, target);
}

/** Report the last companion failure so the heartbeat can log state transitions. */
jstring companion_diagnostics(JNIEnv* env, jclass) {
    char text[192];
    snprintf(text, sizeof(text), "{\"failures\":%ld,\"reason\":\"%s\",\"age_ms\":%ld}",
             companion_failures, companion_failure,
             companion_last_failure_ms ? monotonic_ms() - companion_last_failure_ms : -1L);
    return env->NewStringUTF(text);
}

}  // namespace

bool register_bridge_natives(JNIEnv* env, jclass entry, int companion, bool phone) {
    companion_fd = companion;
    JNINativeMethod natives[] = {
            {const_cast<char*>("readState"),
             const_cast<char*>(phone ? "(II[BLjava/lang/String;)Ljava/lang/String;"
                                     : "(IILjava/lang/String;)Ljava/lang/String;"),
             phone ? reinterpret_cast<void*>(read_phone_state)
                   : reinterpret_cast<void*>(read_state)},
            {const_cast<char*>("hook"),
             const_cast<char*>(
                     "(Ljava/lang/reflect/Method;Ljava/lang/Object;Ljava/lang/reflect/Method;)Ljava/lang/reflect/Method;"),
             reinterpret_cast<void*>(hook_method)},
            {const_cast<char*>("deoptimize"), const_cast<char*>("(Ljava/lang/reflect/Method;)Z"),
             reinterpret_cast<void*>(deoptimize_method)},
    };
    if (env->RegisterNatives(entry, natives, phone ? 3 : 2) != JNI_OK)
        return false;
    if (!phone) {
        JNINativeMethod steps[] = {
                {const_cast<char*>("companionDiagnostics"),
                 const_cast<char*>("()Ljava/lang/String;"),
                 reinterpret_cast<void*>(companion_diagnostics)},
                {const_cast<char*>("installSteps"), const_cast<char*>("()Z"),
                 reinterpret_cast<void*>(install_steps)},
                {const_cast<char*>("updateSteps"),
                 const_cast<char*>("(ZZ[Ljava/lang/String;JJ[I[IZ[F[F)V"),
                 reinterpret_cast<void*>(update_steps)},
        };
        return env->RegisterNatives(entry, steps, 3) == JNI_OK;
    }
    return true;
}
