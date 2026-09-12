#pragma once

#include <jni.h>

// Register state exchange and method hooks using this process's companion descriptor.
bool register_bridge_natives(JNIEnv* env, jclass entry, int companion, bool phone);
