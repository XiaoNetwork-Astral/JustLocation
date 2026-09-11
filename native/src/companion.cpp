#include <sys/types.h>
#include "zygisk.hpp"
#include "io.hpp"
#include "transport.hpp"
#include <arpa/inet.h>
#include <sys/un.h>
#include <unistd.h>
#include <string>

namespace {
std::string status(unsigned char installed, const std::string& subscriptions) {
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
        + ",\"cell_callbacks\":" + ((installed & 8) ? "true" : "false") + "}\n";
    if (installed & 128) {
        request = std::string("{\"version\":1,\"op\":\"telephony_hook_status\",\"cells\":")
            + ((installed & 1) ? "true" : "false")
            + ",\"sim\":" + ((installed & 2) ? "true" : "false")
            + ",\"subscriptions\":" + subscriptions + "}\n";
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
    while (receive_all(client, &operation, 1)) {
        if (operation != 'S' && operation != 'P') break;
        char installed;
        if (!receive_all(client, &installed, 1)) break;
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
        auto response = status(static_cast<unsigned char>(installed), subscriptions);
        uint32_t length = htonl(response.size());
        if (!send_all(client, &length, sizeof(length)) ||
            !send_all(client, response.data(), response.size())) break;
    }
    close(client);
}
}

REGISTER_ZYGISK_COMPANION(companion)
