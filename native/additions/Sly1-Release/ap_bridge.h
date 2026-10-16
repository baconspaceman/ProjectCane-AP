#pragma once
#include <filesystem>
#include <string>
struct GS;
namespace slyap {
void Initialize(const std::filesystem::path& profile);
int RunSelfTest(const std::filesystem::path& root);
bool Enabled();
bool Bound();
bool RemoteBottles();
void Tick();
bool SaveSlot(int slot, const GS& game);
bool LoadSlot(int slot, GS* game);
bool WorldAllowed(int world);
bool HubTravelAllowed(int world);
bool SkipCutscenes();
bool FastClockwerk();
std::string CurrentArea();
const std::string& Error();
}
