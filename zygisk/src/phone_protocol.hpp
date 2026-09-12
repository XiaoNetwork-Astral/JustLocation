#pragma once

#include <string>
#include <string_view>

namespace phone_protocol {

inline std::string quote(std::string_view value) {
    std::string result = "\"";
    constexpr char hex[] = "0123456789abcdef";
    for (unsigned char byte : value) {
        if (byte == '"' || byte == '\\') {
            result += '\\';
            result += static_cast<char>(byte);
        } else if (byte < 0x20) {
            result += "\\u00";
            result += hex[byte >> 4];
            result += hex[byte & 15];
        } else {
            result += static_cast<char>(byte);
        }
    }
    return result + '"';
}

inline std::string operator_fields(std::string_view extra) {
    constexpr const char* names[]{"network_alpha", "sim_alpha", "network_numeric", "sim_numeric"};
    std::string result;
    size_t start = 0;
    for (const char* name : names) {
        if (start > extra.size())
            break;
        size_t end = extra.find('|', start);
        if (end == std::string_view::npos)
            end = extra.size();
        auto value = extra.substr(start, end - start);
        // Absence differs from an explicitly empty original value.
        if (!value.empty())
            result += std::string(",\"") + name + "\":" + quote(value);
        start = end + 1;
    }
    return result;
}

}  // namespace phone_protocol
