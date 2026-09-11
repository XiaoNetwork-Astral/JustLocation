#include <sys/types.h>
#include "zygisk.hpp"
#include "loader.hpp"
#include "transport.hpp"
#include <android/dlext.h>
#include <android/log.h>
#include <dlfcn.h>
#include <cerrno>
#include <cstring>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>
#include <vector>

namespace {

class JustLocationModule final : public zygisk::ModuleBase {
public:
    void onLoad(zygisk::Api* api, JNIEnv* env) override {
        api_ = api;
        env_ = env;
    }

    void preAppSpecialize(zygisk::AppSpecializeArgs* args) override {
        if (args->uid == 1001 && args->nice_name) {
            const char* name = env_->GetStringUTFChars(args->nice_name, nullptr);
            phone_ = name && std::strcmp(name, "com.android.phone") == 0;
            if (name) env_->ReleaseStringUTFChars(args->nice_name, name);
        }
        if (phone_) prepare();
        else api_->setOption(zygisk::Option::DLCLOSE_MODULE_LIBRARY);
    }

    void preServerSpecialize(zygisk::ServerSpecializeArgs*) override {
        prepare();
    }

    void postAppSpecialize(const zygisk::AppSpecializeArgs*) override {
        if (phone_) start();
    }

    void postServerSpecialize(const zygisk::ServerSpecializeArgs*) override {
        start();
    }

private:
    void prepare() {
        int directory = api_->getModuleDir();
        if (directory < 0) return;
        int dex = openat(directory, "bridge/classes.dex", O_RDONLY | O_CLOEXEC);
        struct stat st{};
        if (dex >= 0 && fstat(dex, &st) == 0 && st.st_size > 0 && st.st_size < 8 * 1024 * 1024) {
            dex_.resize(st.st_size);
            size_t offset = 0;
            while (offset < dex_.size()) {
                auto count = read(dex, dex_.data() + offset, dex_.size() - offset);
                if (count < 0 && errno == EINTR) continue;
                if (count <= 0) { dex_.clear(); break; }
                offset += count;
            }
        }
        if (dex >= 0) close(dex);
        if (!dex_.empty()) {
            int libraries = openat(directory, "lib", O_RDONLY | O_DIRECTORY | O_CLOEXEC);
            void* runtime = nullptr;
            if (libraries >= 0) {
                if (prepare_shadowhook(libraries)) runtime = load_library(libraries, "libjustlocation_runtime.so");
                close(libraries);
            }
            if (runtime) start_ = reinterpret_cast<Start>(dlsym(runtime, phone_ ? "justlocation_start_phone" : "justlocation_start"));
            if (start_) {
                int control = api_->connectCompanion();
                if (control >= 0) {
                    if (transport_timeout(control)) companion_ = receive_descriptor(control);
                    close(control);
                }
            }
        }
        close(directory);
    }

    void start() {
        if (start_ && companion_ >= 0) {
            if (!start_(env_, dex_.data(), dex_.size(), companion_)) {
                __android_log_print(ANDROID_LOG_ERROR, "JustLocation", "%s bridge initialization failed", phone_ ? "Phone" : "System");
            }
        } else {
            __android_log_print(ANDROID_LOG_ERROR, "JustLocation", "Runtime or companion unavailable; no location hooks installed");
        }
    }

    bool phone_ = false;
    zygisk::Api* api_ = nullptr;
    JNIEnv* env_ = nullptr;
    int companion_ = -1;
    using Start = bool (*)(JNIEnv*, const void*, size_t, int);
    Start start_ = nullptr;
    std::vector<char> dex_;

};

} // namespace

REGISTER_ZYGISK_MODULE(JustLocationModule)
