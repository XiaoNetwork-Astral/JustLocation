#include <android/log.h>
#include <arpa/inet.h>
#include <fcntl.h>
#include <sys/types.h>
#include <sys/un.h>
#include <unistd.h>

#include <string>
#include <vector>

#include "io.hpp"
#include "transport.hpp"
#include "zygisk.hpp"

namespace {
std::string status(unsigned char installed, unsigned wifi_calls, const std::string& subscriptions,
                   const std::string& gnss_raw, char operation) {
    if (access("/data/adb/modules/justlocation/disable", F_OK) == 0 ||
        access("/data/adb/modules/justlocation/remove", F_OK) == 0)
        return {};
    int fd = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
    if (fd < 0)
        return {};
    timeval timeout{2, 0};
    setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
    setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout));
    sockaddr_un address{};
    address.sun_family = AF_UNIX;
    constexpr char path[] = "/data/adb/justlocation/control.sock";
    __builtin_memcpy(address.sun_path, path, sizeof(path));
    std::string request =
            std::string("{\"version\":1,\"op\":\"hook_status\",\"installed\":") +
            ((installed & 1) ? "true" : "false") +
            ",\"gnss\":" + ((installed & 2) ? "true" : "false") +
            ",\"nmea\":" + ((installed & 4) ? "true" : "false") + ",\"cell_callbacks\":" +
            ((installed & 8) ? "true" : "false")
            // Report scan and connection hooks separately.
            + ",\"wifi_scan\":" + ((installed & 16) ? "true" : "false") + ",\"wifi_connection\":" +
            ((installed & 32) ? "true" : "false")
            // Both raw measurement and navigation hooks must be installed for the raw GNSS flag.
            + ",\"gnss_raw\":" +
            ((installed & 64) ? "true" : "false")
            // Callback counts report actual invocations independently of readiness.
            + ",\"wifi_calls\":" +
            std::to_string(wifi_calls)
            // GNSS counters use channel:index fields with semicolon-separated channels and
            // comma-separated values.
            + ",\"gnss_raw_detail\":" + (gnss_raw.empty() ? "null" : ("\"" + gnss_raw + "\"")) +
            "}\n";
    if (operation == 'T') {
        if (gnss_raw.empty() || gnss_raw.find_first_not_of("0123456789") != std::string::npos) {
            close(fd);
            return {};
        }
        request = std::string("{\"version\":1,\"op\":\"step_hook_status\",\"installed\":") +
                  ((installed & 1) ? "true" : "false") + ",\"events\":" + gnss_raw + "}\n";
    } else if (installed & 128) {
        // Phone operator fields use a pipe separator. Literal newlines would invalidate the JSON
        // request.
        std::vector<std::string> operators{"", "", "", ""};
        {
            size_t start = 0;
            for (size_t index = 0; index < operators.size() && start <= gnss_raw.size(); index++) {
                auto end = gnss_raw.find('|', start);
                if (end == std::string::npos)
                    end = gnss_raw.size();
                operators[index] = gnss_raw.substr(start, end - start);
                start = end + 1;
            }
        }
        auto field = [](const char* name, const std::string& value) {
            // Omit unavailable operator fields; absence differs from an explicitly empty value.
            return value.empty() ? std::string()
                                 : (std::string(",\"") + name + "\":\"" + value + "\"");
        };
        request = std::string("{\"version\":1,\"op\":\"telephony_hook_status\",\"cells\":") +
                  ((installed & 1) ? "true" : "false") +
                  ",\"sim\":" + ((installed & 2) ? "true" : "false") +
                  ",\"subscriptions\":" + subscriptions + field("network_alpha", operators[0]) +
                  field("sim_alpha", operators[1]) + field("network_numeric", operators[2]) +
                  field("sim_numeric", operators[3]) + "}\n";
    }
    std::string result;
    if (connect(fd, reinterpret_cast<sockaddr*>(&address), sizeof(address)) == 0 &&
        send_all(fd, request.data(), request.size())) {
        char buffer[4096];
        while (result.size() < 131072) {
            auto count = recv(fd, buffer, sizeof(buffer), 0);
            if (count < 0 && errno == EINTR)
                continue;
            if (count <= 0)
                break;
            result.append(buffer, count);
            if (result.back() == '\n')
                break;
        }
    }
    close(fd);
    if (result.empty() || result.back() != '\n' || result.size() > 131072)
        return {};
    return result;
}

void companion(int control) {
    int client = handoff_transport(control);
    close(control);
    if (client < 0)
        return;
    char operation;
    static int frames = 0;
    static int phone_frames = 0;
    while (receive_all(client, &operation, 1)) {
        // Log only the first frames, before parsing, to distinguish arrival from decoding
        // failures.
        if (frames < 6) {
            __android_log_print(ANDROID_LOG_INFO, "JustLocation", "companion frame: op=%c",
                                operation);
            frames++;
        }
        if (operation != 'S' && operation != 'P' && operation != 'T')
            break;
        char header[3];
        if (!receive_all(client, header, sizeof(header)))
            break;
        auto installed = static_cast<unsigned char>(header[0]);
        // Big-endian 16-bit callback count; the phone process supplies zero.
        unsigned wifi_calls = (static_cast<unsigned char>(header[1]) << 8) |
                              static_cast<unsigned char>(header[2]);
        std::string gnss_raw;
        uint32_t extra_size;
        if (!receive_all(client, &extra_size, sizeof(extra_size)))
            break;
        extra_size = ntohl(extra_size);
        if (extra_size > 2048)
            break;
        if (extra_size) {
            gnss_raw.resize(extra_size);
            if (!receive_all(client, gnss_raw.data(), extra_size))
                break;
            if (gnss_raw.find('\n') != std::string::npos)
                break;
        }
        std::string subscriptions = "null";
        if (operation == 'P') {
            uint32_t size;
            if (!(installed & 128) || !receive_all(client, &size, sizeof(size)))
                break;
            size = ntohl(size);
            if (size == 0 || size > 4096)
                break;
            subscriptions.resize(size);
            if (!receive_all(client, subscriptions.data(), size))
                break;
            if (subscriptions.find('\n') != std::string::npos)
                break;
        }
        // Log the first decoded phone frames to distinguish transport and payload failures.
        if (operation == 'P' && phone_frames < 3) {
            phone_frames++;
            __android_log_print(ANDROID_LOG_INFO, "JustLocation",
                                "phone frame: installed=%u extra=%zu subs=%zu", installed,
                                gnss_raw.size(), subscriptions.size());
        }
        auto response = status(installed, wifi_calls, subscriptions, gnss_raw, operation);
        uint32_t length = htonl(response.size());
        if (!send_all(client, &length, sizeof(length)) ||
            !send_all(client, response.data(), response.size()))
            break;
    }
    close(client);
}
}  // namespace

REGISTER_ZYGISK_COMPANION(companion)
