// SPDX-License-Identifier: MIT
// Copyright (c) 2025 Autonomy-Logic
// Adapted from core/src/lib/strucpp_abi.hpp at OpenPLC bf82b1b661fd.

#include <cstddef>
#include <cstdint>

namespace strucpp {

struct RetainVarInfo;

struct ProgramBase {
    virtual ~ProgramBase() = default;
    virtual void run() = 0;
    virtual const RetainVarInfo *getRetainVars() const { return nullptr; }
    virtual std::size_t getRetainCount() const { return 0; }
    virtual void sync_in() {}
    virtual void sync_out() {}
    virtual void located_range(std::uint32_t *offset,
                               std::uint32_t *count) const {
        *offset = 0;
        *count = 0;
    }
};

struct TaskInstance {
    const char *name;
    std::int64_t interval_ns;
    std::int32_t priority;
    ProgramBase **programs;
    std::size_t program_count;
};

struct ResourceInstance {
    const char *name;
    const char *processor;
    TaskInstance *tasks;
    std::size_t task_count;
};

struct ConfigurationInstance {
    virtual ~ConfigurationInstance() = default;
    virtual const char *get_name() const = 0;
    virtual ResourceInstance *get_resources() = 0;
    virtual std::size_t get_resource_count() const = 0;
};

}  // namespace strucpp

namespace {

constexpr std::uint8_t OK = 0;
constexpr std::uint8_t INVALID = 1;
constexpr std::uint8_t EXCEPTION = 2;

}  // namespace

extern "C" std::uint8_t open_rust_plc_program_run(void *raw) noexcept {
    if (raw == nullptr) return INVALID;
    try {
        static_cast<strucpp::ProgramBase *>(raw)->run();
        return OK;
    } catch (...) {
        return EXCEPTION;
    }
}

extern "C" std::uint8_t open_rust_plc_program_located_range(
    void *raw, std::uint32_t *offset, std::uint32_t *count) noexcept {
    if (raw == nullptr || offset == nullptr || count == nullptr) return INVALID;
    try {
        static_cast<strucpp::ProgramBase *>(raw)->located_range(offset, count);
        return OK;
    } catch (...) {
        return EXCEPTION;
    }
}

extern "C" std::uint8_t open_rust_plc_configuration_name(
    void *raw, const char **name) noexcept {
    if (raw == nullptr || name == nullptr) return INVALID;
    try {
        *name = static_cast<strucpp::ConfigurationInstance *>(raw)->get_name();
        return OK;
    } catch (...) {
        return EXCEPTION;
    }
}

extern "C" std::uint8_t open_rust_plc_configuration_resources(
    void *raw, strucpp::ResourceInstance **resources,
    std::size_t *count) noexcept {
    if (raw == nullptr || resources == nullptr || count == nullptr) return INVALID;
    try {
        auto *configuration = static_cast<strucpp::ConfigurationInstance *>(raw);
        *resources = configuration->get_resources();
        *count = configuration->get_resource_count();
        return OK;
    } catch (...) {
        return EXCEPTION;
    }
}
