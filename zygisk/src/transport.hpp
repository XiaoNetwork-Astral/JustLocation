#pragma once
#include <sys/socket.h>
#include <unistd.h>

#include <cerrno>
#include <cstring>

inline bool transport_timeout(int fd, int receive_seconds = 4) {
    timeval receive_timeout{receive_seconds, 0};
    timeval timeout{4, 0};
    return setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &receive_timeout, sizeof(receive_timeout)) ==
                   0 &&
           setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout)) == 0;
}

inline bool send_descriptor(int socket, int fd) {
    char tag = 'J';
    iovec data{&tag, 1};
    alignas(cmsghdr) char control[CMSG_SPACE(sizeof(int))]{};
    msghdr message{};
    message.msg_iov = &data;
    message.msg_iovlen = 1;
    message.msg_control = control;
    message.msg_controllen = sizeof(control);
    auto header = CMSG_FIRSTHDR(&message);
    header->cmsg_level = SOL_SOCKET;
    header->cmsg_type = SCM_RIGHTS;
    header->cmsg_len = CMSG_LEN(sizeof(int));
    std::memcpy(CMSG_DATA(header), &fd, sizeof(fd));
    ssize_t result;
    do {
        result = sendmsg(socket, &message, MSG_NOSIGNAL);
    } while (result < 0 && errno == EINTR);
    return result == 1;
}

inline int receive_descriptor(int socket) {
    char tag = 0;
    iovec data{&tag, 1};
    alignas(cmsghdr) char control[CMSG_SPACE(sizeof(int))]{};
    msghdr message{};
    message.msg_iov = &data;
    message.msg_iovlen = 1;
    message.msg_control = control;
    message.msg_controllen = sizeof(control);
    ssize_t result;
    do {
        result = recvmsg(socket, &message, MSG_CMSG_CLOEXEC);
    } while (result < 0 && errno == EINTR);
    auto header = CMSG_FIRSTHDR(&message);
    if (result != 1 || !header || header->cmsg_level != SOL_SOCKET ||
        header->cmsg_type != SCM_RIGHTS || header->cmsg_len != CMSG_LEN(sizeof(int)))
        return -1;
    int fd;
    std::memcpy(&fd, CMSG_DATA(header), sizeof(fd));
    if (tag != 'J' || (message.msg_flags & (MSG_CTRUNC | MSG_TRUNC))) {
        close(fd);
        return -1;
    }
    return fd;
}

// Create endpoints in the root companion's SELinux domain, not in zygote.
// Configure timeouts here too: system_server cannot set options on a root socket.
inline int handoff_transport(int control) {
    int pair[2];
    if (socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, pair))
        return -1;
    // The server waits for a request across bootstrap and GC pauses. Client reads
    // and both writes remain bounded; closing the client still wakes the server.
    bool sent = transport_timeout(pair[0]) && transport_timeout(pair[1], 0) &&
                send_descriptor(control, pair[0]);
    close(pair[0]);
    if (!sent) {
        close(pair[1]);
        return -1;
    }
    return pair[1];
}
