#pragma once
#include <filesystem>
#include <string>

// Native presentation only. The SDK companion remains the AP network authority.
// Call Frame after RenderMenuGui has begun the ImGui frame, before game updates.
namespace apui {
bool Initialize(const std::filesystem::path& absoluteProfilePath);
// Pinned PopTracker source directory. Optional; assets/coordinates are read-only.
bool SetMapPackPath(const std::filesystem::path& absolutePackDirectory);
void Frame();
bool WantsGameInputCapture();
void SetCurrentArea(const std::string& area);
// Parent supplies its current adapter error, not credentials or arbitrary packet text.
void SetNativeDiagnostic(const std::string& message);
// Comic/transition cinematics defer notifications; ordinary in-engine scenes do not.
void SetCinematic(bool deferNotifications);
void Shutdown();
}
