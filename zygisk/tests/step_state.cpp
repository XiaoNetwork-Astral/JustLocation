// Device-side probe for the step and raw-motion state machine.
//
// The production hook runs inside the real SensorService, so the routing and restore paths cannot be
// exercised on a host. This probe calls the state entry points directly with a minimal JNI array
// shim and then reads back what the delivery hook currently holds. It links the production
// step_hooks.cpp, so the code under test is the real one, only without the platform around it.
#include <jni.h>

#include <cstdio>
#include <string>
#include <vector>

#include "step_hooks.hpp"

namespace {
int failures = 0;

void check(bool condition, const char* message) {
    std::printf("%s %s\n", condition ? "ok  " : "FAIL", message);
    if (!condition) failures += 1;
}

// Minimal stand-ins for the JNI arrays the entry point reads: only the accessors it actually calls.
struct Array {
    jint length;
};
struct ObjectArray : Array {
    std::vector<std::string> utf8;
};
struct IntArray : Array {
    std::vector<jint> values;
};
struct FloatArray : Array {
    std::vector<jfloat> values;
};
struct StringObject {
    std::vector<jchar> chars;
};

StringObject* as_string(jobject value) {
    return reinterpret_cast<StringObject*>(value);
}
// These match the C-side JNI signatures the fake table exposes: the environment pointer only, and
// the object type, with no second receiver argument.
jint JNICALL array_length(JNIEnv*, jarray array) {
    return reinterpret_cast<Array*>(array)->length;
}
jobject JNICALL object_element(JNIEnv*, jobjectArray array, jsize index) {
    auto* element = new StringObject();
    for (char byte : reinterpret_cast<ObjectArray*>(array)->utf8[index])
        element->chars.push_back(static_cast<jchar>(byte));
    return reinterpret_cast<jobject>(element);
}
const jchar* JNICALL string_chars(JNIEnv*, jstring value, jboolean*) {
    auto* text = as_string(reinterpret_cast<jobject>(value));
    return text->chars.data();
}
jsize JNICALL string_length(JNIEnv*, jstring value) {
    return static_cast<jsize>(as_string(reinterpret_cast<jobject>(value))->chars.size());
}
void JNICALL release_chars(JNIEnv*, jstring, const jchar*) {}
void JNICALL delete_ref(JNIEnv*, jobject value) {
    delete as_string(value);
}
void JNICALL int_region(JNIEnv*, jintArray array, jsize start, jsize length, jint* out) {
    auto* values = reinterpret_cast<IntArray*>(array);
    for (jsize index = 0; index < length; ++index) out[index] = values->values[start + index];
}
void JNICALL float_region(JNIEnv*, jfloatArray array, jsize start, jsize length, jfloat* out) {
    auto* values = reinterpret_cast<FloatArray*>(array);
    for (jsize index = 0; index < length; ++index) out[index] = values->values[start + index];
}

JNIEnv* fake_env() {
    static JNINativeInterface table{};
    table.GetArrayLength = array_length;
    table.GetObjectArrayElement = object_element;
    table.GetStringChars = string_chars;
    table.GetStringLength = string_length;
    table.ReleaseStringChars = release_chars;
    table.DeleteLocalRef = delete_ref;
    table.GetIntArrayRegion = int_region;
    table.GetFloatArrayRegion = float_region;
    // In C++ a JNIEnv is a pointer to a pointer to the function table.
    static JNINativeInterface* pointer = &table;
    return reinterpret_cast<JNIEnv*>(&pointer);
}

jobjectArray to_objects(const std::vector<std::string>& names) {
    auto* array = new ObjectArray();
    array->length = static_cast<jint>(names.size());
    array->utf8 = names;
    return reinterpret_cast<jobjectArray>(array);
}
jintArray to_ints(const std::vector<jint>& values) {
    auto* array = new IntArray();
    array->length = static_cast<jint>(values.size());
    array->values = values;
    return reinterpret_cast<jintArray>(array);
}
jfloatArray to_floats(const std::vector<jfloat>& values) {
    auto* array = new FloatArray();
    array->length = static_cast<jint>(values.size());
    array->values = values;
    return reinterpret_cast<jfloatArray>(array);
}

// Sensor type constants from <android/sensor.h>, repeated so the probe does not need those headers.
constexpr jint kAccelerometer = 1;
constexpr jint kGyroscope = 4;
constexpr jint kStepDetector = 18;
constexpr jint kStepCounter = 19;
}  // namespace

int main() {
    JNIEnv* env = fake_env();
    std::printf("probe: step and raw-motion state machine\n");

    // Nothing published yet: the hook must report a stale, empty state rather than guessing.
    auto empty = step_state_summary();
    check(!empty.active && empty.packages == 0 && empty.sensors == 0 && empty.stale,
          "an unpublished state is stale and empty");

    // A step-only session: both step sensors are eligible, the raw sensors are not.
    update_step_state(env, true, false, to_objects({"example.walker"}), 120, 7,
                      to_ints({101, 102, 103, 104}),
                      to_ints({kAccelerometer, kGyroscope, kStepDetector, kStepCounter}), false,
                      nullptr, nullptr);
    auto stepper = step_state_summary();
    check(stepper.active && !stepper.all && stepper.packages == 1 && stepper.total == 120,
          "a step-only session keeps its package list and counter");
    check(stepper.sensors == 2 && !stepper.motion,
          "step events route while the two raw sensors stay on system output");
    check(!stepper.stale, "a freshly published state is not stale");

    // The raw channel adds exactly the two motion sensors and carries the motion state through.
    update_step_state(env, true, false, to_objects({"example.walker"}), 121, 7,
                      to_ints({101, 102, 103, 104}),
                      to_ints({kAccelerometer, kGyroscope, kStepDetector, kStepCounter}), true,
                      to_floats({0.1f, 9.9f, 0.0f}), to_floats({0.0f, 0.7f, 0.0f}));
    auto running = step_state_summary();
    check(running.sensors == 4 && running.motion,
          "the raw channel adds accelerometer and gyroscope to the routed set");

    // Malformed axes must not switch the channel on with values that were never supplied.
    update_step_state(env, true, false, to_objects({"example.walker"}), 121, 7, to_ints({101}),
                      to_ints({kAccelerometer}), true, to_floats({1.0f}),
                      to_floats({1.0f, 1.0f, 1.0f}));
    auto malformed = step_state_summary();
    check(!malformed.motion && malformed.sensors == 0,
          "a short axis array leaves the raw channel off instead of reading past it");
    check(!malformed.stale, "the session itself stays published");

    // An all-apps session keeps routing without a package list.
    update_step_state(env, true, true, nullptr, 5, 8, nullptr, nullptr, false, nullptr, nullptr);
    auto every = step_state_summary();
    check(every.active && every.all && every.packages == 0 && every.sensors == 0,
          "an all-apps session is accepted without a package list and without handles");

    // Removing the session clears everything, which is what restores real sensor output.
    update_step_state(env, false, false, nullptr, 0, 0, nullptr, nullptr, false, nullptr, nullptr);
    auto removed = step_state_summary();
    check(!removed.active && removed.sensors == 0 && !removed.motion && removed.stale,
          "removing the session clears the state and reports it stale");

    std::printf(failures == 0 ? "JUSTLOCATION_STEP_PROBE_OK\n"
                              : "JUSTLOCATION_STEP_PROBE_FAILED %d\n",
                failures);
    return failures == 0 ? 0 : 1;
}
