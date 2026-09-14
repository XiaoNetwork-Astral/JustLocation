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

/// What the delivery hook currently holds, for diagnostics and for the device probe. `sensors` is
/// how many step or motion handles are eligible, and `stale` reports that the state was never
/// published or has aged past the heartbeat window.
struct StepStateSummary {
    bool active;
    bool all;
    int packages;
    int sensors;
    bool motion;
    uint64_t total;
    bool stale;
};
StepStateSummary step_state_summary();
