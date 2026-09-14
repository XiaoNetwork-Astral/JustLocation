#include "step_hooks.hpp"

#include <android/log.h>
#include <android/sensor.h>
#include <shadowhook.h>
#include <time.h>

#include <atomic>
#include <mutex>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <vector>

namespace {
using Send = int (*)(void*, const ASensorEvent*, size_t, ASensorEvent*, const void*);
using Destroy = void (*)(void*);
using HasSensor = bool (*)(const void*, int);

// Android String16 is one pointer with non-trivial destruction. Declaring a destructor preserves
// AArch64's indirect return convention; never copy connection layout or private member offsets.
void (*destroy_string)(void*) = nullptr;
struct String16 {
    const char16_t* data;
    ~String16() {
        destroy_string(this);
    }
};
using Package = String16 (*)(const void*);
Send original_send = nullptr;
Destroy original_destroy = nullptr;
HasSensor has_sensor = nullptr;
Package package_name = nullptr;
std::atomic<bool> ready{false};
std::atomic<uint64_t> events{0};
std::mutex state_mutex;

int64_t elapsed_ns() {
    timespec now{};
    clock_gettime(CLOCK_BOOTTIME, &now);
    return now.tv_sec * 1'000'000'000LL + now.tv_nsec;
}

struct Sensor {
    int handle;
    int type;
};
/**
 * Motion state behind the raw six-axis channel. The daemon owns the physics; this side stores the
 * sample it was given and delivers it to in-scope subscribers.
 */
struct Motion {
    bool active = false;
    double accelerometer[3]{};
    double gyroscope[3]{};
    int64_t updated = 0;
};
struct Snapshot {
    bool active = false;
    bool all = false;
    std::unordered_set<std::u16string> packages;
    std::vector<Sensor> sensors;
    uint64_t total = 0;
    uint64_t epoch = 0;
    uint64_t revision = 0;
    int64_t received = 0;
    Motion motion;
} state;
struct Cursor {
    uint64_t total;
    uint64_t revision;
    int64_t timestamp;
};
std::unordered_map<void*, Cursor> cursors;

bool is_step(const ASensorEvent& event) {
    return event.type == ASENSOR_TYPE_STEP_DETECTOR || event.type == ASENSOR_TYPE_STEP_COUNTER;
}

bool is_motion_sensor(int type) {
    return type == ASENSOR_TYPE_ACCELEROMETER || type == ASENSOR_TYPE_GYROSCOPE;
}

/**
 * Deliver one six-axis sample exactly as the daemon computed it.
 *
 * The physics lives in one place (the daemon's motion model), so this side must not re-shape the
 * waveform: a second implementation here could disagree with the position and the step count it is
 * supposed to match. Scope, routing and restoration are this side's responsibility.
 */
void motion_sample(const Motion& motion, float accelerometer[3], float gyroscope[3]) {
    for (int axis = 0; axis < 3; ++axis) {
        accelerometer[axis] = static_cast<float>(motion.accelerometer[axis]);
        gyroscope[axis] = static_cast<float>(motion.gyroscope[axis]);
    }
}

void destroy(void* connection) {
    {
        std::lock_guard lock(state_mutex);
        cursors.erase(connection);
    }
    original_destroy(connection);
}

int send(void* connection, const ASensorEvent* input, size_t count, ASensorEvent* scratch,
         const void* flush_map) {
    Snapshot current;
    {
        std::lock_guard lock(state_mutex);
        if (!state.active || elapsed_ns() - state.received >= 20'000'000'000LL) {
            // Keep original calls outside our lock: sensor-service locks may already be held.
            current.active = false;
        } else {
            current = state;
        }
    }
    if (!current.active)
        return original_send(connection, input, count, scratch, flush_map);
    auto package = package_name(connection);
    if (!package.data || !*package.data ||
        (!current.all && !current.packages.contains(package.data))) {
        return original_send(connection, input, count, scratch, flush_map);
    }

    std::vector<Sensor> subscribed;
    for (const auto& sensor : current.sensors) {
        if (has_sensor(connection, sensor.handle))
            subscribed.push_back(sensor);
    }
    if (subscribed.empty())
        return original_send(connection, input, count, scratch, flush_map);

    // Preserve flush-map indices for normal batches. Cached activation events have no scratch
    // buffer and are already filtered, so remove their real step entries instead.
    std::vector<ASensorEvent> original;
    original.reserve(count);
    for (size_t i = 0; i < count; ++i) {
        auto event = input[i];
        if (is_step(event)) {
            if (!scratch)
                continue;
            event.sensor = -1;
        }
        original.push_back(event);
    }
    int result = original_send(connection, original.data(), original.size(), scratch, flush_map);

    const int64_t now = elapsed_ns();
    Cursor previous{};
    bool initial;
    {
        std::lock_guard lock(state_mutex);
        auto cursor = cursors.find(connection);
        initial = cursor == cursors.end() || cursor->second.revision != current.revision;
        previous = initial ? Cursor{current.total, current.revision, now} : cursor->second;
        if (initial || current.total != previous.total)
            cursors[connection] = {current.total, current.revision, now};
    }
    const auto added = current.total >= previous.total ? current.total - previous.total : 0;
    bool flushed = false;
    bool activation = false;
    for (size_t i = 0; i < count; ++i)
        flushed |= input[i].type == 0;
    for (size_t i = 0; i < count; ++i)
        activation |= !scratch && is_step(input[i]);
    if (!initial && added == 0 && !flushed && !activation)
        return result;

    std::vector<ASensorEvent> synthetic;
    // Detector events older than the heartbeat lifetime are not replayed after suspend/stalls.
    const auto recent = std::min<uint64_t>(added, 1000);
    for (const auto& sensor : subscribed) {
        if (sensor.type == ASENSOR_TYPE_STEP_DETECTOR && !initial) {
            for (uint64_t index = 0; index < recent; ++index) {
                ASensorEvent event{};
                event.version = sizeof(event);
                event.sensor = sensor.handle;
                event.type = sensor.type;
                event.timestamp =
                        previous.timestamp + (now - previous.timestamp) * (index + 1) / recent;
                event.data[0] = 1.0f;
                synthetic.push_back(event);
            }
        } else if (sensor.type == ASENSOR_TYPE_STEP_COUNTER) {
            ASensorEvent event{};
            event.version = sizeof(event);
            event.sensor = sensor.handle;
            event.type = sensor.type;
            event.timestamp = now;
            event.u64.step_counter = current.total;
            synthetic.push_back(event);
        } else if (is_motion_sensor(sensor.type) && current.motion.active) {
            // One sample per delivered batch, exactly as the daemon computed it: the batch rate is
            // the sample rate, and no waveform is regenerated here.
            float accelerometer[3];
            float gyroscope[3];
            motion_sample(current.motion, accelerometer, gyroscope);
            ASensorEvent event{};
            event.version = sizeof(event);
            event.sensor = sensor.handle;
            event.type = sensor.type;
            event.timestamp = now;
            const float* values = sensor.type == ASENSOR_TYPE_ACCELEROMETER ? accelerometer
                                                                            : gyroscope;
            for (int axis = 0; axis < 3; ++axis)
                event.data[axis] = values[axis];
            synthetic.push_back(event);
        }
    }
    if (!synthetic.empty()) {
        std::vector<ASensorEvent> filtered(synthetic.size());
        // Always supply scratch: the platform checks subscriptions, flush state, permissions and
        // AppOps before writing any of our events, including the initial counter value.
        int status = original_send(connection, synthetic.data(), synthetic.size(), filtered.data(),
                                   nullptr);
        if (status == 0)
            events.fetch_add(synthetic.size(), std::memory_order_relaxed);
        if (result == 0)
            result = status;
    }
    return result;
}
}  // namespace

bool install_step_hooks() {
    if (ready.load())
        return true;
    void* library = shadowhook_dlopen("libsensorservice.so");
    if (!library)
        return false;
    void* utils = shadowhook_dlopen("libutils.so");
    auto failed = [&] {
        // prepare() retries unavailable hooks; failed attempts must release resolver handles.
        shadowhook_dlclose(library);
        if (utils)
            shadowhook_dlclose(utils);
        return false;
    };
    destroy_string = reinterpret_cast<decltype(destroy_string)>(
            utils ? shadowhook_dlsym(utils, "_ZN7android8String16D1Ev") : nullptr);
    package_name = reinterpret_cast<Package>(shadowhook_dlsym(
            library, "_ZNK7android13SensorService21SensorEventConnection16getOpPackageNameEv"));
    has_sensor = reinterpret_cast<HasSensor>(shadowhook_dlsym(
            library, "_ZNK7android13SensorService21SensorEventConnection9hasSensorEi"));
    void* target = shadowhook_dlsym(
            library,
            "_ZN7android13SensorService21SensorEventConnection10sendEventsEPK15sensors_event_tmPS2_PKNS_2wpIKS1_EE");
    void* release = shadowhook_dlsym(library,
                                     "_ZN7android13SensorService21SensorEventConnection7destroyEv");
    if (!destroy_string || !package_name || !has_sensor || !target || !release)
        return failed();
    void* destroy_stub = shadowhook_hook_func_addr(release, reinterpret_cast<void*>(destroy),
                                                   reinterpret_cast<void**>(&original_destroy));
    if (!destroy_stub)
        return failed();
    if (!shadowhook_hook_func_addr(target, reinterpret_cast<void*>(send),
                                   reinterpret_cast<void**>(&original_send))) {
        shadowhook_unhook(destroy_stub);
        return failed();
    }
    ready.store(true);
    __android_log_print(ANDROID_LOG_INFO, "JustLocation", "Step sensor hooks installed");
    return true;
}

bool step_hooks_ready() {
    return ready.load();
}
uint64_t step_event_count() {
    return events.load(std::memory_order_relaxed);
}

void update_step_state(JNIEnv* env, bool active, bool all, jobjectArray packages, jlong total,
                       jlong epoch, jintArray handles, jintArray types, bool motion_active,
                       jfloatArray motion_accelerometer, jfloatArray motion_gyroscope) {
    Snapshot next;
    next.active = active;
    next.all = all;
    next.total = total < 0 ? 0 : static_cast<uint64_t>(total);
    next.epoch = static_cast<uint64_t>(epoch);
    next.received = elapsed_ns();
    if (motion_active && motion_accelerometer && motion_gyroscope &&
        env->GetArrayLength(motion_accelerometer) == 3 &&
        env->GetArrayLength(motion_gyroscope) == 3) {
        float accelerometer[3];
        float gyroscope[3];
        env->GetFloatArrayRegion(motion_accelerometer, 0, 3, accelerometer);
        env->GetFloatArrayRegion(motion_gyroscope, 0, 3, gyroscope);
        next.motion.active = true;
        next.motion.updated = next.received;
        for (int axis = 0; axis < 3; ++axis) {
            next.motion.accelerometer[axis] = accelerometer[axis];
            next.motion.gyroscope[axis] = gyroscope[axis];
        }
    }
    if (packages) {
        for (int i = 0; i < env->GetArrayLength(packages); ++i) {
            auto name = static_cast<jstring>(env->GetObjectArrayElement(packages, i));
            const jchar* chars = env->GetStringChars(name, nullptr);
            if (!chars)
                return;
            next.packages.emplace(reinterpret_cast<const char16_t*>(chars),
                                  env->GetStringLength(name));
            env->ReleaseStringChars(name, chars);
            env->DeleteLocalRef(name);
        }
    }
    if (handles && types && env->GetArrayLength(handles) == env->GetArrayLength(types)) {
        const auto count = env->GetArrayLength(handles);
        std::vector<jint> ids(count), kinds(count);
        env->GetIntArrayRegion(handles, 0, count, ids.data());
        env->GetIntArrayRegion(types, 0, count, kinds.data());
        for (int i = 0; i < count; ++i) {
            // Step events always route; raw motion only when the channel is actually generating.
            if (kinds[i] == ASENSOR_TYPE_STEP_DETECTOR || kinds[i] == ASENSOR_TYPE_STEP_COUNTER ||
                (is_motion_sensor(kinds[i]) && next.motion.active))
                next.sensors.push_back({ids[i], kinds[i]});
        }
    }
    std::lock_guard lock(state_mutex);
    next.revision = state.revision + (next.active != state.active || next.epoch != state.epoch ||
                                      next.all != state.all || next.packages != state.packages ||
                                      next.total < state.total ||
                                      next.motion.active != state.motion.active);
    if (!next.active)
        cursors.clear();
    state = std::move(next);
}
