#pragma once

#include <jni.h>

// Shared initialization entry for the runtime and ART probe.
extern "C" bool justlocation_init_hooks(JNIEnv* env);
