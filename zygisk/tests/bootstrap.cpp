#include <jni.h>
#include <sys/socket.h>
#include <string>
#include <vector>
#include "io.hpp"
#include "loader.hpp"
#include "transport.hpp"

extern "C" JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM*, void*) { return JNI_VERSION_1_6; }

namespace {
int peer = -1;
std::vector<char> bridge_dex;
}

extern "C" JNIEXPORT jboolean JNICALL
Java_me_idk_justlocation_bridge_RuntimeProbe_bootstrap(JNIEnv* env, jclass, jstring directory, jbyteArray dex) {
    const char* chars = env->GetStringUTFChars(directory, nullptr);
    if (!chars) return false;
    std::string path(chars);
    env->ReleaseStringUTFChars(directory, chars);
    int libraries = open(path.c_str(), O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (libraries < 0) return false;
    void* runtime = prepare_shadowhook(libraries) ? load_library(libraries, "libjustlocation_runtime.so") : nullptr;
    close(libraries);
    if (!runtime) return false;
    using Start = bool (*)(JNIEnv*, const void*, size_t, int);
    auto start = reinterpret_cast<Start>(dlsym(runtime, "justlocation_start"));
    if (!start) return false;
    bridge_dex.resize(env->GetArrayLength(dex));
    env->GetByteArrayRegion(dex, 0, bridge_dex.size(), reinterpret_cast<jbyte*>(bridge_dex.data()));
    if (env->ExceptionCheck()) return false;
    int pair[2];
    if (socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, pair)) return false;
    transport_timeout(pair[0]); transport_timeout(pair[1]);
    peer = handoff_transport(pair[0]);
    int endpoint = peer >= 0 ? receive_descriptor(pair[1]) : -1;
    close(pair[0]); close(pair[1]);
    return endpoint >= 0 && start(env, bridge_dex.data(), bridge_dex.size(), endpoint);
}

extern "C" JNIEXPORT jboolean JNICALL
Java_me_idk_justlocation_bridge_RuntimeProbe_ready(JNIEnv*, jclass) {
    for (int attempt = 0; attempt < 5; ++attempt) {
        char request[2];
        if (!receive_all(peer, request, sizeof(request)) || request[0] != 'S') return false;
        // No configuration: every production callback continues using original output.
        uint32_t empty = 0;
        if (!send_all(peer, &empty, sizeof(empty))) return false;
        if (request[1] == 1) return true;
    }
    return false;
}
