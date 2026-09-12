#include "transport.hpp"
#include "io.hpp"
#include <fcntl.h>
#include <sys/wait.h>
#include <cstdio>

static bool label(const char* attribute, const char* value) {
    int fd = open(attribute, O_WRONLY | O_CLOEXEC);
    if (fd < 0) return false;
    bool ok = write(fd, value, strlen(value)) == static_cast<ssize_t>(strlen(value));
    close(fd);
    return ok;
}

int main() {
    if (getuid() != 0) { puts("Root required for disposable SELinux transition probe"); return 1; }
    int control[2];
    if (socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, control)) return 2;
    transport_timeout(control[0]); transport_timeout(control[1]);
    pid_t child = fork();
    if (child < 0) return 3;
    if (child == 0) {
        close(control[0]);
        // Label just this test socket as zygote. The stock policy does not permit
        // an arbitrary process to perform zygote's real system-server transition.
        if (!label("/proc/thread-self/attr/sockcreate", "u:r:zygote:s0")) _exit(10);
        int old[2];
        if (socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, old)) _exit(11);
        if (!label("/proc/thread-self/attr/sockcreate", "")) _exit(18);
        transport_timeout(old[0]);
        int endpoint = receive_descriptor(control[1]);
        close(control[1]);
        if (endpoint < 0) _exit(12);
        if (setresgid(1000, 1000, 1000) || setresuid(1000, 1000, 1000)) _exit(13);
        if (!label("/proc/thread-self/attr/current", "u:r:system_server:s0")) _exit(14);
        errno = 0;
        if (send_all(old[0], "X", 1) || errno != EACCES) _exit(15);
        if (!send_all(endpoint, "J", 1)) _exit(16);
        char reply = 0;
        if (!receive_all(endpoint, &reply, 1) || reply != 'K') _exit(17);
        _exit(0);
    }
    close(control[1]);
    int endpoint = handoff_transport(control[0]);
    close(control[0]);
    char request = 0;
    bool exchanged = endpoint >= 0 && receive_all(endpoint, &request, 1) && request == 'J' && send_all(endpoint, "K", 1);
    if (endpoint >= 0) close(endpoint);
    int status = 0;
    waitpid(child, &status, 0);
    printf("Disposable child status=%d, root transport exchange=%s\n", status, exchanged ? "PASS" : "FAIL");
    if (!exchanged || !WIFEXITED(status) || WEXITSTATUS(status) != 0) return 4;
    puts("PASS: zygote socket denied; root-created transport survives UID 1000 / system_server transition");
}
