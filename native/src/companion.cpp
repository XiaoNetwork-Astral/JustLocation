#include <sys/types.h>
#include "zygisk.hpp"
#include "io.hpp"
#include "transport.hpp"
#include <android/log.h>
#include <arpa/inet.h>
#include <fcntl.h>
#include <sys/un.h>
#include <unistd.h>
#include <string>
#include <vector>

namespace {
std::string status(unsigned char installed, unsigned wifi_calls, const std::string& subscriptions,
                   const std::string& gnss_raw) {
    if (access("/data/adb/modules/justlocation/disable", F_OK) == 0 ||
        access("/data/adb/modules/justlocation/remove", F_OK) == 0) return {};
    int fd = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
    if (fd < 0) return {};
    timeval timeout{2, 0};
    setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
    setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout));
    sockaddr_un address{};
    address.sun_family = AF_UNIX;
    constexpr char path[] = "/data/adb/justlocation/control.sock";
    __builtin_memcpy(address.sun_path, path, sizeof(path));
    std::string request = std::string("{\"version\":1,\"op\":\"hook_status\",\"installed\":") + ((installed & 1) ? "true" : "false")
        + ",\"gnss\":" + ((installed & 2) ? "true" : "false")
        + ",\"nmea\":" + ((installed & 4) ? "true" : "false")
        + ",\"cell_callbacks\":" + ((installed & 8) ? "true" : "false")
        // Wi-Fi 两项分开报告：扫描与连接信息是两次独立的适配，装上一项不等于另一项也在。
        + ",\"wifi_scan\":" + ((installed & 16) ? "true" : "false")
        + ",\"wifi_connection\":" + ((installed & 32) ? "true" : "false")
        // GNSS 原始数据两条出口（原始测量 + 导航电文）一起装好才置位，见 BridgeEntry.installGnssRaw。
        + ",\"gnss_raw\":" + ((installed & 64) ? "true" : "false")
        // 回调计数：就绪位只说明"挂上了"，这个数才说明"被走到了"。
        + ",\"wifi_calls\":" + std::to_string(wifi_calls)
        // GNSS 原始通道的计数（`下标:注册数,dispatch 次数,该投次数,真投次数,送达数,失败数`，多条用 `;` 分隔）。
        // 同样是"挂钩就绪不等于送达"的观测手段，靠状态回包本身，不依赖 system_server 写文件。
        + ",\"gnss_raw_detail\":" + (gnss_raw.empty() ? "null" : ("\"" + gnss_raw + "\"")) + "}\n";
    if (installed & 128) {
        // 手机进程的透传串是**真实运营商值**，四项用 `|` 分隔。
        //
        // **不要用换行分隔**：这些值会被拼进下面的 JSON 字符串，而 JSON 字符串里不允许
        // 裸换行——整帧会被守护进程拒收（2026-09-12 踩过：手机进程的钩子装上了、
        // 心跳却永远到不了，现象和之前的 companion 帧错位一模一样）。
        std::vector<std::string> operators{"", "", "", ""};
        {
            size_t start = 0;
            for (size_t index = 0; index < operators.size() && start <= gnss_raw.size(); index++) {
                auto end = gnss_raw.find('|', start);
                if (end == std::string::npos) end = gnss_raw.size();
                operators[index] = gnss_raw.substr(start, end - start);
                start = end + 1;
            }
        }
        auto field = [](const char* name, const std::string& value) {
            // 空值时**不发这个字段**：Rust 侧按 `Option<String>` 收，缺省表示"这一轮没有真值"，
            // 与"真值是空串"是两回事。
            return value.empty() ? std::string()
                                 : (std::string(",\"") + name + "\":\"" + value + "\"");
        };
        request = std::string("{\"version\":1,\"op\":\"telephony_hook_status\",\"cells\":")
            + ((installed & 1) ? "true" : "false")
            + ",\"sim\":" + ((installed & 2) ? "true" : "false")
            + ",\"subscriptions\":" + subscriptions
            + field("network_alpha", operators[0])
            + field("sim_alpha", operators[1])
            + field("network_numeric", operators[2])
            + field("sim_numeric", operators[3]) + "}\n";
    }
    std::string result;
    if (connect(fd, reinterpret_cast<sockaddr*>(&address), sizeof(address)) == 0 &&
        send_all(fd, request.data(), request.size())) {
        char buffer[4096];
        while (result.size() < 131072) {
            auto count = recv(fd, buffer, sizeof(buffer), 0);
            if (count < 0 && errno == EINTR) continue;
            if (count <= 0) break;
            result.append(buffer, count);
            if (result.back() == '\n') break;
        }
    }
    close(fd);
    if (result.empty() || result.back() != '\n' || result.size() > 131072) return {};
    return result;
}

void companion(int control) {
    int client = handoff_transport(control);
    close(control);
    if (client < 0) return;
    char operation;
    static int frames = 0;
    static int phone_frames = 0;
    while (receive_all(client, &operation, 1)) {
        // **诊断放在最前面**：放在解析之后的话，一旦中途 `break`（帧没发全、订阅尺寸不对……）
        // 就永远不会打日志，于是"帧到底到没到"又变成猜（2026-09-12 为此多绕了一轮）。
        //
        // **必须有界**：这是每秒一条的心跳，无限打会把 logcat 的环形缓冲冲掉，
        // 连别的诊断一起冲没（2026-09-12 就因此误判过一次）。
        if (frames < 6) {
            __android_log_print(ANDROID_LOG_INFO, "JustLocation", "companion frame: op=%c", operation);
            frames++;
        }
        if (operation != 'S' && operation != 'P') break;
        char header[3];
        if (!receive_all(client, header, sizeof(header))) break;
        auto installed = static_cast<unsigned char>(header[0]);
        // 大端两字节的 Wi-Fi 回调计数；两个进程都会带，手机进程恒为 0。
        unsigned wifi_calls = (static_cast<unsigned char>(header[1]) << 8)
            | static_cast<unsigned char>(header[2]);
        std::string gnss_raw;
        uint32_t extra_size;
        if (!receive_all(client, &extra_size, sizeof(extra_size))) break;
        extra_size = ntohl(extra_size);
        if (extra_size > 2048) break;
        if (extra_size) {
            gnss_raw.resize(extra_size);
            if (!receive_all(client, gnss_raw.data(), extra_size)) break;
            if (gnss_raw.find('\n') != std::string::npos) break;
        }
        std::string subscriptions = "null";
        if (operation == 'P') {
            uint32_t size;
            if (!(installed & 128) || !receive_all(client, &size, sizeof(size))) break;
            size = ntohl(size);
            if (size == 0 || size > 4096) break;
            subscriptions.resize(size);
            if (!receive_all(client, subscriptions.data(), size)) break;
            if (subscriptions.find('\n') != std::string::npos) break;
        }
        // **诊断：把手机进程那条路（'P'）实际收到的内容也记一行。**
        // 与上面那条一起看，就能分清"帧没到"与"帧到了但解析失败"。
        // 之前只能靠"有没有回应"反推，猜错过两轮（2026-09-12）。
        if (operation == 'P' && phone_frames < 3) {
            phone_frames++;
            __android_log_print(ANDROID_LOG_INFO, "JustLocation",
                    "phone frame: installed=%u extra=%zu subs=%zu", installed, gnss_raw.size(), subscriptions.size());
        }
        auto response = status(installed, wifi_calls, subscriptions, gnss_raw);
        uint32_t length = htonl(response.size());
        if (!send_all(client, &length, sizeof(length)) ||
            !send_all(client, response.data(), response.size())) break;
    }
    close(client);
}
}

REGISTER_ZYGISK_COMPANION(companion)
