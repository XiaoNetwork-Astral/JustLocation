#pragma once

#include <jni.h>

#include <cstdint>

bool install_step_hooks();
bool step_hooks_ready();
uint64_t step_event_count();
/// `motion_*` carry the raw six-axis state from one heartbeat; passing motion_active false keeps
/// the raw sensors on system output while the step events continue.
void update_step_state(JNIEnv* env, bool active, bool all, jobjectArray packages, jlong total,
                       jlong epoch, jintArray handles, jintArray types, bool motion_active,
                       jfloatArray motion_accelerometer, jfloatArray motion_gyroscope);
