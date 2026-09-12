#pragma once
#include <cstddef>
#include <cstdint>
#include <string_view>

class ArtSymbols {
public:
    bool open();
    void* find(std::string_view name, bool prefix) const;

private:
    const std::byte* data_ = nullptr;
    size_t size_ = 0;
    uintptr_t bias_ = 0;
};
