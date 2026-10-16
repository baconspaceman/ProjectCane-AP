#pragma once

#include <cstdint>
#include <string>

// Transient presentation/physics state only. ap_bridge owns the durable queue,
// receipts and transitions. This module never writes a save or grants inventory.
namespace aptraps {

constexpr int64_t Ice = 10020026;
constexpr int64_t Speed = 10020027;
constexpr int64_t Ball = 10020028;
constexpr int64_t Invisibility = 10020029;
constexpr double DurationSeconds = 10.0;
constexpr std::size_t MaximumPending = 128;

struct Capabilities {
    bool ice = false;
    bool speed = false;
    bool ball = false;
    bool invisibility = false;
};

struct Record {
    std::string delivery;
    int64_t item = 0;
    // Speed only: 0=slow (0.5), 1=fast (1.5). Choose once and persist before start.
    int variant = 0;
};

class Runtime {
public:
    // Configure only effects whose actual native hooks have been integrated.
    // Ice uses the native low-friction cheat path, not an emulator address.
    void Configure(Capabilities capabilities);
    bool Supports(int64_t item) const;
    // Call AFTER committing this record's queued->started transition to the
    // native save envelope, while gameplay is safe. Calling twice never resets time.
    bool StartCommitted(const Record& record);
    // Supply monotonic REAL elapsed seconds, never scaled simulation delta.
    // Loading/menu/death suspends modifiers, but an already started ten-second
    // effect continues expiring, matching the original client's wall-clock timer.
    void Advance(double elapsedSeconds, bool safeGameplay);
    const Record* Finished() const;
    // Call only AFTER persisting started->finished. Exact delivery identity required.
    bool ClearFinishedCommitted(const std::string& delivery);
    // Stops transient modifiers immediately; durable state remains parent's job.
    void Suspend();
    bool Active() const;
    double RemainingSeconds() const;
    float SpeedScale() const;
    bool IceActive() const;
    bool ForceRoll() const;
    bool HideSly() const;
private:
    Capabilities capabilities_;
    Record record_;
    bool occupied_ = false;
    bool safe_ = false;
    double remaining_ = 0.0;
};

Runtime& State();
// Read-only hook conveniences; default to inert before Configure/StartCommitted.
float SpeedScale();
bool IceActive();
bool ForceRoll();
bool HideSly();

} // namespace aptraps
