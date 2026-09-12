#include <android/dlext.h>
#include <dlfcn.h>
#include <fcntl.h>
#include <unistd.h>
#include <cstdio>
#include <string>
#include "loader.hpp"

// No DT_NEEDED dependency or LD_LIBRARY_PATH: reproduce the Zygisk fd loader.
int main(int argc, char** argv) {
    if (argc != 2) return 2;
    int fd = open(argv[1], O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (fd < 0) { perror("open library directory"); return 3; }
    bool ready = prepare_shadowhook(fd, true);
    close(fd);
    printf("ShadowHook fd-loader init: %s\n", ready ? "PASS" : "FAIL");
    return ready ? 0 : 1;
}
