#include "ap_traps.h"

#include <algorithm>
#include <cmath>

namespace aptraps {

void Runtime::Configure(Capabilities capabilities) {
    capabilities_ = capabilities;
    if (occupied_ && !Supports(record_.item)) safe_ = false;
}

bool Runtime::Supports(int64_t item) const {
    switch (item) {
    case Ice: return capabilities_.ice;
    case Speed: return capabilities_.speed;
    case Ball: return capabilities_.ball;
    case Invisibility: return capabilities_.invisibility;
    default: return false;
    }
}

bool Runtime::StartCommitted(const Record& record) {
    if (record.delivery.empty() || record.delivery.size() > 256 || !Supports(record.item)) return false;
    if ((record.item == Speed && record.variant != 0 && record.variant != 1) ||
        (record.item != Speed && record.variant != 0)) return false;
    if (occupied_) {
        // Idempotent in-process retry, including an already expired effect.
        return record.delivery == record_.delivery && record.item == record_.item && record.variant == record_.variant;
    }
    record_ = record;
    occupied_ = true;
    safe_ = true;
    remaining_ = DurationSeconds;
    return true;
}

void Runtime::Advance(double elapsedSeconds, bool safeGameplay) {
    // Invalid timing makes effects inert rather than extending them indefinitely.
    if (!std::isfinite(elapsedSeconds) || elapsedSeconds < 0.0) {
        safe_ = false;
        return;
    }
    safe_ = safeGameplay;
    if (occupied_) remaining_ = std::max(0.0, remaining_ - elapsedSeconds);
}

const Record* Runtime::Finished() const {
    return occupied_ && remaining_ <= 0.0 ? &record_ : nullptr;
}

bool Runtime::ClearFinishedCommitted(const std::string& delivery) {
    if (Finished() == nullptr || delivery != record_.delivery) return false;
    occupied_ = false;
    safe_ = false;
    remaining_ = 0.0;
    record_ = {};
    return true;
}

void Runtime::Suspend() { safe_ = false; }
bool Runtime::Active() const { return occupied_ && safe_ && remaining_ > 0.0 && Supports(record_.item); }
double Runtime::RemainingSeconds() const { return occupied_ ? remaining_ : 0.0; }
float Runtime::SpeedScale() const { return Active() && record_.item == Speed ? (record_.variant == 0 ? 0.5f : 1.5f) : 1.0f; }
bool Runtime::IceActive() const { return Active() && record_.item == Ice; }
bool Runtime::ForceRoll() const { return Active() && record_.item == Ball; }
bool Runtime::HideSly() const { return Active() && record_.item == Invisibility; }

Runtime& State() { static Runtime runtime; return runtime; }
float SpeedScale() { return State().SpeedScale(); }
bool IceActive() { return State().IceActive(); }
bool ForceRoll() { return State().ForceRoll(); }
bool HideSly() { return State().HideSly(); }

} // namespace aptraps
