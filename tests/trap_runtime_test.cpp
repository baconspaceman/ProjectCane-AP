// Standalone contract test; no game, SDK or graphics context. Parent runs build.
#include "../native/ProjectCane/Sly1-Release/ap_traps.h"
#include <cassert>
#include <limits>

int main() {
    aptraps::Runtime r;
    assert(!r.Supports(aptraps::Ice));
    assert(!r.StartCommitted({"uncommitted", aptraps::Speed, 0}));
    r.Configure({false, true, true, true});
    assert(!r.Supports(aptraps::Ice)); // not enabled before its native hook is integrated
    assert(!r.StartCommitted({"bad", aptraps::Speed, 2}));
    assert(r.SpeedScale() == 1.0f && !r.ForceRoll() && !r.HideSly());
    assert(r.StartCommitted({"first", aptraps::Speed, 0}));
    assert(r.SpeedScale() == 0.5f);
    r.Advance(3.0, true);
    assert(r.RemainingSeconds() == 7.0);
    assert(r.StartCommitted({"first", aptraps::Speed, 0}));
    assert(r.RemainingSeconds() == 7.0); // duplicate is not a new duration
    assert(!r.StartCommitted({"first", aptraps::Speed, 1}));
    assert(!r.StartCommitted({"second", aptraps::Ball, 0}));
    r.Advance(2.0, false);
    assert(r.SpeedScale() == 1.0f && r.RemainingSeconds() == 5.0);
    r.Advance(5.0, false);
    assert(r.Finished() && r.Finished()->delivery == "first");
    assert(!r.ClearFinishedCommitted("wrong"));
    assert(r.ClearFinishedCommitted("first"));
    assert(r.StartCommitted({"second", aptraps::Ball, 0}));
    assert(r.ForceRoll());
    r.Suspend();
    assert(!r.ForceRoll());
    r.Advance(0.0, true);
    assert(r.ForceRoll());
    r.Advance(std::numeric_limits<double>::quiet_NaN(), true);
    assert(!r.ForceRoll());
    r.Advance(10.0, true);
    assert(!r.ForceRoll());
    assert(r.ClearFinishedCommitted("second"));
    assert(r.StartCommitted({"third", aptraps::Invisibility, 0}));
    assert(r.HideSly());
    r.Configure({false, true, true, false});
    assert(!r.HideSly());
    r.Advance(10.0, true);
    assert(r.ClearFinishedCommitted("third"));
    r.Configure({true, true, true, true});
    assert(r.Supports(aptraps::Ice));
    assert(!r.IceActive());
    assert(r.StartCommitted({"fourth", aptraps::Ice, 0}));
    assert(r.IceActive());
    r.Advance(5.0, false);
    assert(!r.IceActive());
    r.Advance(0.0, true);
    assert(r.IceActive());
    r.Advance(5.0, true);
    assert(!r.IceActive());
    return 0;
}
