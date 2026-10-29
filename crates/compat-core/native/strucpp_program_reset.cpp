#include "generated.hpp"

#include <cstdint>
#include <new>

extern strucpp::Configuration_CONFIG0 g_config;

namespace {
bool config_live = true;
}

extern "C" std::uint8_t strucpp_reset_config() noexcept {
    try {
        if (config_live) {
            g_config.~Configuration_CONFIG0();
            config_live = false;
        }
        new (&g_config) strucpp::Configuration_CONFIG0();
        config_live = true;
        return 0x7e;
    } catch (...) {
        return 0x81;
    }
}
