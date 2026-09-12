#pragma once

#include <jni.h>

#include <cstdint>

bool install_step_hooks();
bool step_hooks_ready();
uint64_t step_event_count();
void update_step_state(JNIEnv* env, bool active, bool all, jobjectArray packages, jlong total,
                       jlong epoch, jintArray handles, jintArray types);
