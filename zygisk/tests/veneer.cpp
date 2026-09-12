#include <shadowhook.h>
#include <sys/mman.h>
#include <unistd.h>

#include <cstdint>
#include <cstdio>
#include <cstring>

// Reproduce an existing ARM64 hook's LDR X17 / BR X17 entry veneer.
// The destination is filled at runtime to avoid Android text relocations.
asm(".text\n.balign 16\n.global existing_veneer\n.type existing_veneer,%function\n"
    "existing_veneer:\nldr x17, 1f\nbr x17\nnop\nnop\n1: .quad 0\n"
    ".size existing_veneer, .-existing_veneer\n");
extern "C" int existing_veneer(int);
static int (*original)(int);
__attribute__((noinline)) static int destination(int value) {
    return value + 7;
}
static int replacement(int value) {
    return original(value) + 100;
}

int main() {
    setbuf(stdout, nullptr);
    alarm(5);  // A broken trampoline loops; bound failure to this disposable process.
    auto address = reinterpret_cast<uintptr_t>(existing_veneer);
    auto page = address & ~(static_cast<uintptr_t>(getpagesize()) - 1);
    if (mprotect(reinterpret_cast<void*>(page), getpagesize(), PROT_READ | PROT_WRITE | PROT_EXEC))
        return 2;
    auto target = reinterpret_cast<uintptr_t>(destination);
    memcpy(reinterpret_cast<void*>(address + 16), &target, sizeof(target));
    __builtin___clear_cache(reinterpret_cast<char*>(address),
                            reinterpret_cast<char*>(address + 24));
    if (existing_veneer(34) != 41 || shadowhook_init(SHADOWHOOK_MODE_UNIQUE, true))
        return 3;
    void* stub = shadowhook_hook_func_addr(reinterpret_cast<void*>(address),
                                           reinterpret_cast<void*>(replacement),
                                           reinterpret_cast<void**>(&original));
    if (!stub) {
        printf("hook failed: %d\n", shadowhook_get_errno());
        return 4;
    }
    puts("Veneer installed; calling original trampoline");
    if (existing_veneer(34) != 141)
        return 5;
    if (shadowhook_unhook(stub) || existing_veneer(34) != 41)
        return 6;
    alarm(0);
    puts("PASS: existing LDR X17 / BR X17 veneer, original call and unhook");
}
