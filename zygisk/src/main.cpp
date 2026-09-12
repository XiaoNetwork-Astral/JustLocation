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
        if (args->uid == 1001) {
            // **把判定依据记下来**：`args->nice_name` 在开机早期可能为空，那种情况下
            // 我们认不出手机进程，于是走下面的卸载分支、一句日志都不留——
            // 现象是"钩子从来没装上"而现场什么都看不到（2026-09-12 卡了两轮）。
            const char* name = args->nice_name ? env_->GetStringUTFChars(args->nice_name, nullptr) : nullptr;
            phone_ = name && std::strcmp(name, "com.android.phone") == 0;
            __android_log_print(ANDROID_LOG_INFO, "JustLocation", "uid 1001 process: name=%s phone=%d",
                    name ? name : "(null)", phone_ ? 1 : 0);
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
    /**
     * 准备注入所需的东西。**每一步失败都要说清是哪一步**。
     *
     * <p>以前这里只把失败咽下去，等到 `start()` 才报一句笼统的
     * "Runtime or companion unavailable"，于是"开机后手机进程没接管"这件事
     * 只能靠猜（我猜过一次"开机时序"，猜错了）。失败原因分成四类：
     * 取不到模块目录 / 读不到 DEX / 加载不了运行库 / 拿不到 companion fd，
     * 它们对应完全不同的修法，所以必须分别记下来。
     */
    void prepare() {
        failure_ = nullptr;
        int directory = api_->getModuleDir();
        if (directory < 0) { failure_ = "module directory unavailable"; return; }
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
        if (dex_.empty()) { close(directory); failure_ = "bridge DEX unreadable"; return; }
        int libraries = openat(directory, "lib", O_RDONLY | O_DIRECTORY | O_CLOEXEC);
        void* runtime = nullptr;
        if (libraries >= 0) {
            if (prepare_shadowhook(libraries)) runtime = load_library(libraries, "libjustlocation_runtime.so");
            close(libraries);
        }
        if (runtime) start_ = reinterpret_cast<Start>(dlsym(runtime, phone_ ? "justlocation_start_phone" : "justlocation_start"));
        if (!start_) { close(directory); failure_ = "runtime library unavailable"; return; }
        int control = api_->connectCompanion();
        if (control < 0) { close(directory); failure_ = "companion unavailable"; return; }
        // **重试几次，不要只试一次。**
        //
        // <p>这条 socket 的接收超时是 4 秒（`transport_timeout` 的默认值），而 Zygisk 的
        // companion 端要先把描述符转交给守护进程。开机时 `system_server` 与手机进程几乎
        // 同时来要，慢一步就超时——只试一次的话，这个进程从此再没有机会建立通路，
        // 表现是"钩子装上了、心跳永远到不了"，而且**时好时坏**（2026-09-12 反复踩）。
        // 每次 `connectCompanion` 都会新建一条 socket，所以重试是安全的。
        for (int attempt = 0; attempt < 3 && companion_ < 0; attempt++) {
            if (attempt > 0) {
                close(control);
                usleep(200 * 1000);
                control = api_->connectCompanion();
                if (control < 0) break;
            }
            if (!transport_timeout(control, 2)) continue;
            companion_ = receive_descriptor(control);
        }
        close(control);
        if (companion_ < 0) { failure_ = "companion descriptor unavailable"; }
        close(directory);
    }

    void start() {
        if (start_ && companion_ >= 0) {
            if (!start_(env_, dex_.data(), dex_.size(), companion_)) {
                __android_log_print(ANDROID_LOG_ERROR, "JustLocation", "%s bridge initialization failed", phone_ ? "Phone" : "System");
            }
        } else {
            __android_log_print(ANDROID_LOG_ERROR, "JustLocation", "%s injection incomplete: %s",
                    phone_ ? "Phone" : "System", failure_ ? failure_ : "unknown reason");
        }
    }

    bool phone_ = false;
    /** 注入准备失败在哪一步；`nullptr` 表示没失败。只用于日志。 */
    const char* failure_ = nullptr;
    zygisk::Api* api_ = nullptr;
    JNIEnv* env_ = nullptr;
    int companion_ = -1;
    using Start = bool (*)(JNIEnv*, const void*, size_t, int);
    Start start_ = nullptr;
    std::vector<char> dex_;

};

} // namespace

REGISTER_ZYGISK_MODULE(JustLocationModule)
