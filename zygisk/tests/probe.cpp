#include <dlfcn.h>
#include <jni.h>
#include <sys/socket.h>
#include <unistd.h>

#include <lsplant.hpp>
#include <string>
#include <vector>

#include "hook_engine.hpp"
#include "io.hpp"

// Keep ART's JNI entry lookup from resolving ShadowHook's SDK-specific JNI_OnLoad.
extern "C" JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM*, void*) {
    // ART can unload JNI libraries while destroying the VM. Its native hooks must
    // remain mapped through shutdown, just like the retained dlopen in the module.
    Dl_info self{};
    if (!dladdr(reinterpret_cast<void*>(JNI_OnLoad), &self) ||
        !dlopen(self.dli_fname, RTLD_NOW | RTLD_NODELETE))
        return JNI_ERR;
    return JNI_VERSION_1_6;
}

extern "C" JNIEXPORT jboolean JNICALL
Java_me_idk_justlocation_bridge_RuntimeProbe_initialize(JNIEnv* env, jclass) {
    return justlocation_init_hooks(env);
}

extern "C" JNIEXPORT jobject JNICALL Java_me_idk_justlocation_bridge_RuntimeProbe_hook(
        JNIEnv* env, jclass, jobject target, jobject receiver, jobject callback) {
    return lsplant::Hook(env, target, receiver, callback);
}

extern "C" JNIEXPORT jboolean JNICALL
Java_me_idk_justlocation_bridge_RuntimeProbe_unhook(JNIEnv* env, jclass, jobject target) {
    return lsplant::UnHook(env, target);
}
