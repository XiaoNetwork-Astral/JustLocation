#pragma once
#include <sys/socket.h>

#include <cerrno>
#include <cstddef>

inline bool send_all(int fd, const void* data, size_t size) {
    const auto* bytes = static_cast<const char*>(data);
    while (size) {
        auto sent = send(fd, bytes, size, MSG_NOSIGNAL);
        if (sent < 0 && errno == EINTR)
            continue;
        if (sent <= 0)
            return false;
        bytes += sent;
        size -= sent;
    }
    return true;
}

inline bool receive_all(int fd, void* data, size_t size) {
    auto* bytes = static_cast<char*>(data);
    while (size) {
        auto received = recv(fd, bytes, size, 0);
        if (received < 0 && errno == EINTR)
            continue;
        if (received <= 0)
            return false;
        bytes += received;
        size -= received;
    }
    return true;
}
