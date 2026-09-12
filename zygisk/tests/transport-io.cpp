#include <sys/wait.h>

#include <cstdio>

#include "io.hpp"
#include "transport.hpp"

// A slow ART bootstrap or GC pause must not permanently disconnect the bridge.
static bool exchange(unsigned delay) {
    int control[2];
    if (socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, control))
        return false;
    transport_timeout(control[0]);
    transport_timeout(control[1]);
    pid_t child = fork();
    if (child < 0)
        return false;
    if (child == 0) {
        close(control[0]);
        int client = receive_descriptor(control[1]);
        close(control[1]);
        if (client < 0)
            _exit(1);
        sleep(delay);
        char reply = 0;
        bool ok = send_all(client, "J", 1) && receive_all(client, &reply, 1) && reply == 'K';
        close(client);
        _exit(ok ? 0 : 2);
    }
    close(control[1]);
    int server = handoff_transport(control[0]);
    close(control[0]);
    char request = 0;
    bool ok = server >= 0 && receive_all(server, &request, 1) && request == 'J' &&
              send_all(server, "K", 1);
    if (server >= 0)
        close(server);
    int status = 0;
    waitpid(child, &status, 0);
    ok = ok && WIFEXITED(status) && WEXITSTATUS(status) == 0;
    printf("%s: transport exchange after %u seconds idle\n", ok ? "PASS" : "FAIL", delay);
    return ok;
}

int main() {
    bool immediate = exchange(0);
    bool delayed = exchange(5);
    return immediate && delayed ? 0 : 1;
}
