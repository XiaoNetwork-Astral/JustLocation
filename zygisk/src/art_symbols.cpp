#include "art_symbols.hpp"

#include <elf.h>
#include <fcntl.h>
#include <link.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

#include <cstring>
#include <string>

bool ArtSymbols::open() {
    struct Loaded {
        uintptr_t bias = 0;
        std::string path;
    } loaded;
    dl_iterate_phdr(
            [](dl_phdr_info* info, size_t, void* context) {
                std::string_view path(info->dlpi_name);
                if (!path.ends_with("/libart.so"))
                    return 0;
                auto& output = *static_cast<Loaded*>(context);
                output.bias = info->dlpi_addr;
                output.path = path;
                return 1;
            },
            &loaded);
    if (loaded.path.empty())
        return false;
    int fd = ::open(loaded.path.c_str(), O_RDONLY | O_CLOEXEC);
    if (fd < 0)
        return false;
    struct stat st{};
    if (fstat(fd, &st) || st.st_size < static_cast<off_t>(sizeof(Elf64_Ehdr))) {
        close(fd);
        return false;
    }
    void* memory = mmap(nullptr, st.st_size, PROT_READ, MAP_PRIVATE, fd, 0);
    close(fd);
    if (memory == MAP_FAILED)
        return false;
    // Retained for the lifetime of ART hooks, including later lazy symbol lookups.
    data_ = static_cast<const std::byte*>(memory);
    size_ = st.st_size;
    bias_ = loaded.bias;
    const auto* header = reinterpret_cast<const Elf64_Ehdr*>(data_);
    return !memcmp(header->e_ident, ELFMAG, SELFMAG) && header->e_ident[EI_CLASS] == ELFCLASS64;
}

void* ArtSymbols::find(std::string_view name, bool prefix) const {
    if (!data_)
        return nullptr;
    const auto& header = *reinterpret_cast<const Elf64_Ehdr*>(data_);
    auto fits = [this](size_t offset, size_t length) {
        return offset <= size_ && length <= size_ - offset;
    };
    if (header.e_shentsize != sizeof(Elf64_Shdr) ||
        !fits(header.e_shoff, static_cast<size_t>(header.e_shnum) * sizeof(Elf64_Shdr)))
        return nullptr;
    const auto* sections = reinterpret_cast<const Elf64_Shdr*>(data_ + header.e_shoff);
    for (size_t i = 0; i < header.e_shnum; ++i) {
        const auto& section = sections[i];
        if ((section.sh_type != SHT_SYMTAB && section.sh_type != SHT_DYNSYM) ||
            section.sh_link >= header.e_shnum || section.sh_entsize != sizeof(Elf64_Sym) ||
            !fits(section.sh_offset, section.sh_size))
            continue;
        const auto& strings = sections[section.sh_link];
        if (!fits(strings.sh_offset, strings.sh_size))
            continue;
        const auto* symbols = reinterpret_cast<const Elf64_Sym*>(data_ + section.sh_offset);
        const auto* names = reinterpret_cast<const char*>(data_ + strings.sh_offset);
        for (size_t n = 0; n < section.sh_size / sizeof(Elf64_Sym); ++n) {
            const auto& symbol = symbols[n];
            if (!symbol.st_value || symbol.st_shndx == SHN_UNDEF ||
                symbol.st_name >= strings.sh_size)
                continue;
            const char* start = names + symbol.st_name;
            const auto* end =
                    static_cast<const char*>(memchr(start, 0, strings.sh_size - symbol.st_name));
            if (!end)
                continue;
            std::string_view candidate(start, end - start);
            if (prefix ? candidate.starts_with(name) : candidate == name) {
                return reinterpret_cast<void*>(bias_ + symbol.st_value);
            }
        }
    }
    return nullptr;
}
