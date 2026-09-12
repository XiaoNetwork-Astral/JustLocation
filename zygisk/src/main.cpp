#include <android/dlext.h>
#include <android/log.h>
#include <dlfcn.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

#include <cerrno>
#include <cstring>
#include <vector>

#include "loader.hpp"
#include "transport.hpp"
#include "zygisk.hpp"

namespace {

class JustLocationModule final : public zygisk::ModuleBase {
public:
    void onLoad(zygisk::Api* api, JNIEnv* env) override {
        api_ = api;
        env_ = env;
    }

    void preAppSpecialize(zygisk::AppSpecializeArgs* args) override {
        if (args->uid == 1001) {
            // Log process selection because nice_name can be absent during early startup.
            const char* name =
                    args->nice_name ? env_->GetStringUTFChars(args->nice_name, nullptr) : nullptr;
            phone_ = name && std::strcmp(name, "com.android.phone") == 0;
            __android_log_print(ANDROID_LOG_INFO, "JustLocation",
                                "uid 1001 process: name=%s phone=%d", name ? name : "(null)",
                                phone_ ? 1 : 0);
            if (name)
                env_->ReleaseStringUTFChars(args->nice_name, name);
        }
        if (phone_) {
            prepare();
            if (companion_ >= 0 && args->fds_to_ignore && !api_->exemptFd(companion_)) {
                // Some Zygisk implementations do not implement exemptFd. The public
                // specialization argument provides the same exclusion for zygote's FD check.
                auto previous = *args->fds_to_ignore;
                jsize count = previous ? env_->GetArrayLength(previous) : 0;
                auto keep = env_->NewIntArray(count + 1);
                if (keep) {
                    if (count) {
                        std::vector<jint> descriptors(count);
                        env_->GetIntArrayRegion(previous, 0, count, descriptors.data());
                        env_->SetIntArrayRegion(keep, 0, count, descriptors.data());
                    }
                    env_->SetIntArrayRegion(keep, count, 1, &companion_);
                    *args->fds_to_ignore = keep;
                } else {
                    close(companion_);
                    companion_ = -1;
                    failure_ = "companion descriptor exclusion unavailable";
                    env_->ExceptionClear();
                }
            }
        } else
            api_->setOption(zygisk::Option::DLCLOSE_MODULE_LIBRARY);
    }

    void preServerSpecialize(zygisk::ServerSpecializeArgs*) override {
        prepare();
    }

    void postAppSpecialize(const zygisk::AppSpecializeArgs*) override {
        if (phone_)
            start();
    }

    void postServerSpecialize(const zygisk::ServerSpecializeArgs*) override {
        start();
    }

private:
    /**
     * Prepare DEX, runtime and companion transport; retain the exact failing step for diagnostics.
     */
    void prepare() {
        failure_ = nullptr;
        int directory = api_->getModuleDir();
        if (directory < 0) {
            failure_ = "module directory unavailable";
            return;
        }
        int dex = openat(directory, "bridge/classes.dex", O_RDONLY | O_CLOEXEC);
        struct stat st{};
        if (dex >= 0 && fstat(dex, &st) == 0 && st.st_size > 0 && st.st_size < 8 * 1024 * 1024) {
            dex_.resize(st.st_size);
            size_t offset = 0;
            while (offset < dex_.size()) {
                auto count = read(dex, dex_.data() + offset, dex_.size() - offset);
                if (count < 0 && errno == EINTR)
                    continue;
                if (count <= 0) {
                    dex_.clear();
                    break;
                }
                offset += count;
            }
        }
        if (dex >= 0)
            close(dex);
        if (dex_.empty()) {
            close(directory);
            failure_ = "bridge DEX unreadable";
            return;
        }
        int libraries = openat(directory, "lib", O_RDONLY | O_DIRECTORY | O_CLOEXEC);
        void* runtime = nullptr;
        if (libraries >= 0) {
            if (prepare_shadowhook(libraries))
                runtime = load_library(libraries, "libjustlocation_runtime.so");
            close(libraries);
        }
        if (runtime)
            start_ = reinterpret_cast<Start>(
                    dlsym(runtime, phone_ ? "justlocation_start_phone" : "justlocation_start"));
        if (!start_) {
            close(directory);
            failure_ = "runtime library unavailable";
            return;
        }
        int control = api_->connectCompanion();
        if (control < 0) {
            close(directory);
            failure_ = "companion unavailable";
            return;
        }
        // Retry descriptor handoff across early boot contention. Each attempt uses a new companion
        // socket and a two-second receive timeout.
        for (int attempt = 0; attempt < 3 && companion_ < 0; attempt++) {
            if (attempt > 0) {
                close(control);
                usleep(200 * 1000);
                control = api_->connectCompanion();
                if (control < 0)
                    break;
            }
            if (!transport_timeout(control, 2))
                continue;
            companion_ = receive_descriptor(control);
        }
        close(control);
        if (companion_ < 0) {
            failure_ = "companion descriptor unavailable";
        }
        close(directory);
    }

    void start() {
        if (start_ && companion_ >= 0) {
            if (!start_(env_, dex_.data(), dex_.size(), companion_)) {
                __android_log_print(ANDROID_LOG_ERROR, "JustLocation",
                                    "%s bridge initialization failed", phone_ ? "Phone" : "System");
            }
        } else {
            __android_log_print(ANDROID_LOG_ERROR, "JustLocation", "%s injection incomplete: %s",
                                phone_ ? "Phone" : "System",
                                failure_ ? failure_ : "unknown reason");
        }
    }

    bool phone_ = false;
    /** Injection preparation failure, or nullptr when no failure was recorded. */
    const char* failure_ = nullptr;
    zygisk::Api* api_ = nullptr;
    JNIEnv* env_ = nullptr;
    int companion_ = -1;
    using Start = bool (*)(JNIEnv*, const void*, size_t, int);
    Start start_ = nullptr;
    std::vector<char> dex_;
};

}  // namespace

REGISTER_ZYGISK_MODULE(JustLocationModule)
