#include "ap_ui.h"
#include "ap_fileio.h"
#include <glad/glad.h>
#include <imgui/imgui.h>
#include <nlohmann/json.hpp>
#define STB_IMAGE_STATIC
#define STB_IMAGE_IMPLEMENTATION
#define STBI_ONLY_PNG
#include <stb/stb_image.h>
#include <algorithm>
#include <array>
#include <chrono>
#include <cstring>
#include <deque>
#include <fstream>
#include <map>
#include <set>
#include <sstream>
#include <vector>
#ifdef _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <sddl.h>
#pragma comment(lib, "Advapi32.lib")
#else
#include <fcntl.h>
#include <unistd.h>
#endif

namespace apui {
namespace {
using Json = nlohmann::json;
namespace fs = std::filesystem;
struct Notice { std::string text, detail, kind; int flags = 0; double until = 0; };
struct Marker { std::string area; float x = 0, y = 0; };
struct Map {
    std::string name;
    fs::path image;
    std::vector<Marker> markers;
    GLuint texture = 0;
    int width = 0, height = 0;
    bool attempted = false;
};
struct State {
    fs::path profile, bridge;
    std::string save, game, error, nativeDiagnostic, actionMessage, lastRequest, identity, currentArea, selectedArea;
    std::array<char, 512> endpoint{};
    std::array<char, 128> slot{}, password{}, search{};
    Json status, tracker;
    bool enabled = false, panel = true, full = false, interact = false, capture = false;
    bool highContrast = false, reducedMotion = false, detailed = false, hudBk = false;
    bool mapView = false, follow = true, moveNotice = false, cinematic = false;
    bool eventsPrimed = false, previousBkKnown = false, previousBk = false;
    bool launcherFatal = false;
    bool showPerformance = false, showFrameGraph = false;
    std::array<float, 360> frameTimes{};
    int frameNext = 0, frameCount = 0, frameWindowCount = 0;
    double frameWindowMs = 0;
    float measuredFps = 0, measuredFrameMs = 0;
    std::chrono::steady_clock::time_point previousFrame{};
    std::string failedHelper;
    int anchor = 0, palette = 0, deferred = 0;
    float textScale = 1.0f, uiScale = 1.0f, compactSeconds = 4.0f, detailSeconds = 7.0f;
    float floatingX = 20.0f, floatingY = 100.0f;
    double nextPoll = 0, lastCommand = -10;
    uint64_t serial = 0;
    std::set<std::string> seen;
    std::deque<Notice> recent, notices;
    std::deque<std::pair<fs::path, uint64_t>> pendingRequests;
    std::vector<Map> maps;
    int selectedMap = 0;
} s;

uint64_t NowMs() {
    return static_cast<uint64_t>(std::chrono::duration_cast<std::chrono::milliseconds>(
        std::chrono::system_clock::now().time_since_epoch()).count());
}
std::string Str(const Json& obj, const char* name, const std::string& fallback = "") {
    auto it = obj.find(name);
    return it != obj.end() && it->is_string() ? it->get<std::string>() : fallback;
}
bool Bool(const Json& obj, const char* name, bool fallback = false) {
    auto it = obj.find(name);
    return it != obj.end() && it->is_boolean() ? it->get<bool>() : fallback;
}
bool Count(const Json& obj, const char* name, uint64_t& value) {
    auto field = obj.find(name);
    if (field == obj.end()) return false;
    if (field->is_number_unsigned()) { value = field->get<uint64_t>(); return true; }
    if (field->is_number_integer()) {
        auto n = field->get<int64_t>();
        if (n >= 0) { value = static_cast<uint64_t>(n); return true; }
    }
    return false;
}
Json Read(const fs::path& path) {
    // Atomic companion writers replace these files on Windows. A normal ifstream
    // omits FILE_SHARE_DELETE and can make that replace fail with Access denied.
    const auto bytes = slyapio::ReadText(path, 1024 * 1024);
    if (bytes.empty()) return Json();
    auto result = Json::parse(bytes, nullptr, false);
    return result.is_object() ? result : Json();
}
Json ReadPackJson(const fs::path& path) {
    std::error_code ec;
    if (fs::file_size(path, ec) > 2 * 1024 * 1024 || ec) return Json();
    std::ifstream input(path, std::ios::binary);
    return Json::parse(input, nullptr, false, true); // Pack is JSON with comments.
}
void ReadMarkers(const Json& nodes) {
    if (!nodes.is_array()) return;
    for (const auto& node : nodes) {
        if (!node.is_object()) continue;
        auto locations = node.find("map_locations");
        if (locations != node.end() && locations->is_array()) for (const auto& location : *locations) {
            if (!location.is_object() || !location.contains("x") || !location["x"].is_number() ||
                !location.contains("y") || !location["y"].is_number()) continue;
            auto target = Str(location, "map");
            for (auto& map : s.maps) if (map.name == target)
                map.markers.push_back({Str(node, "name"), location["x"].get<float>(), location["y"].get<float>()});
        }
        auto children = node.find("children");
        if (children != node.end()) ReadMarkers(*children);
    }
}
bool LoadMapTexture(Map& map) {
    if (map.attempted) return map.texture != 0;
    map.attempted = true;
    std::error_code ec;
    auto count = fs::file_size(map.image, ec);
    if (ec || count > 16 * 1024 * 1024) return false;
    std::vector<unsigned char> bytes(static_cast<size_t>(count));
    std::ifstream input(map.image, std::ios::binary);
    if (!input.read(reinterpret_cast<char*>(bytes.data()), static_cast<std::streamsize>(count))) return false;
    int channels = 0;
    if (!stbi_info_from_memory(bytes.data(), static_cast<int>(bytes.size()), &map.width, &map.height, &channels)
        || map.width <= 0 || map.height <= 0 || map.width > 4096 || map.height > 4096) return false;
    unsigned char* pixels = stbi_load_from_memory(bytes.data(), static_cast<int>(bytes.size()), &map.width, &map.height, &channels, 4);
    if (!pixels) return false;
    GLint oldBinding = 0, oldAlignment = 0, oldRowLength = 0, oldPbo = 0;
    glGetIntegerv(GL_TEXTURE_BINDING_2D, &oldBinding);
    glGetIntegerv(GL_UNPACK_ALIGNMENT, &oldAlignment);
    glGetIntegerv(GL_UNPACK_ROW_LENGTH, &oldRowLength);
    glGetIntegerv(GL_PIXEL_UNPACK_BUFFER_BINDING, &oldPbo);
    glBindBuffer(GL_PIXEL_UNPACK_BUFFER, 0);
    glPixelStorei(GL_UNPACK_ALIGNMENT, 1);
    glPixelStorei(GL_UNPACK_ROW_LENGTH, 0);
    glGenTextures(1, &map.texture);
    glBindTexture(GL_TEXTURE_2D, map.texture);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA, map.width, map.height, 0, GL_RGBA, GL_UNSIGNED_BYTE, pixels);
    glBindTexture(GL_TEXTURE_2D, static_cast<GLuint>(oldBinding));
    glPixelStorei(GL_UNPACK_ALIGNMENT, oldAlignment);
    glPixelStorei(GL_UNPACK_ROW_LENGTH, oldRowLength);
    glBindBuffer(GL_PIXEL_UNPACK_BUFFER, static_cast<GLuint>(oldPbo));
    stbi_image_free(pixels);
    return map.texture != 0;
}
// The control protocol may carry a password exactly once. Never create a world-readable
// temporary file. Consumers remove the command before acting; no password goes to settings.
bool WritePrivate(const fs::path& path, const Json& value) {
    std::error_code ec;
    fs::create_directories(path.parent_path(), ec);
    if (ec) return false;
    fs::path temporary = path;
    temporary += ".tmp";
    const auto bytes = value.dump();
#ifdef _WIN32
    PSECURITY_DESCRIPTOR sd = nullptr;
    if (!ConvertStringSecurityDescriptorToSecurityDescriptorW(
            L"D:P(A;;FA;;;SY)(A;;FA;;;OW)", SDDL_REVISION_1, &sd, nullptr)) return false;
    SECURITY_ATTRIBUTES sa{sizeof(SECURITY_ATTRIBUTES), sd, FALSE};
    HANDLE h = CreateFileW(temporary.c_str(), GENERIC_WRITE, 0, &sa, CREATE_NEW,
        FILE_ATTRIBUTE_NORMAL, nullptr);
    LocalFree(sd);
    if (h == INVALID_HANDLE_VALUE) return false;
    DWORD written = 0;
    bool ok = WriteFile(h, bytes.data(), static_cast<DWORD>(bytes.size()), &written, nullptr) &&
        written == bytes.size() && FlushFileBuffers(h);
    CloseHandle(h);
    if (ok) ok = slyapio::ReplacePublished(temporary, path);
    if (!ok) DeleteFileW(temporary.c_str());
    return ok;
#else
    int fd = open(temporary.c_str(), O_WRONLY | O_CREAT | O_EXCL, 0600);
    if (fd < 0) return false;
    size_t done = 0;
    while (done < bytes.size()) {
        auto n = write(fd, bytes.data() + done, bytes.size() - done);
        if (n <= 0) break;
        done += static_cast<size_t>(n);
    }
    bool ok = done == bytes.size() && fsync(fd) == 0;
    close(fd);
    if (ok) { fs::rename(temporary, path, ec); ok = !ec; }
    if (!ok) fs::remove(temporary, ec);
    return ok;
#endif
}
bool Fresh() {
    if (!s.status.is_object()) return false;
    auto it = s.status.find("updated_unix_ms");
    if (it == s.status.end() || !it->is_number_unsigned()) return false;
    uint64_t now = NowMs(), stamp = it->get<uint64_t>();
    return stamp <= now + 2000 && now <= stamp + 5000;
}
bool LogicKnown() { return !s.launcherFatal && Fresh() && Bool(s.status, "server_connected") &&
    s.tracker.is_object() && Str(s.tracker, "status") == "known"; }
void Notify(const std::string& text, const std::string& detail = "", int flags = 0,
            const std::string& kind = "status", bool popup = true) {
    Notice n{text, detail, kind, flags, ImGui::GetTime() + (s.detailed ? s.detailSeconds : s.compactSeconds)};
    s.recent.push_front(n);
    while (s.recent.size() > 60) s.recent.pop_back();
    if (!popup) return;
    if (s.cinematic) { ++s.deferred; return; }
    s.notices.push_back(n);
    while (s.notices.size() > 4) s.notices.pop_front();
}
ImVec4 ItemColor(int flags) {
    if (flags & 4) return s.palette == 0 ? ImVec4(1, .38f, .38f, 1) : ImVec4(1, .65f, .15f, 1);
    if (flags & 1) return s.palette == 0 ? ImVec4(.38f, 1, .55f, 1) : ImVec4(.35f, .75f, 1, 1);
    return ImVec4(1, 1, 1, 1);
}
void Request(const char* action) {
    if (ImGui::GetTime() - s.lastCommand < 1) return;
    if (!Fresh()) { s.actionMessage = "Companion unavailable. Start this profile's companion first."; return; }
    auto id = std::to_string(NowMs()) + "-" + std::to_string(++s.serial);
    Json command{{"version", 1}, {"id", id}, {"save", s.save}, {"action", action}};
    if (std::string(action) != "disconnect") {
        command["endpoint"] = s.endpoint.data();
        command["slot_name"] = s.slot.data();
        if (s.password[0]) command["password"] = s.password.data();
    }
    auto path = s.bridge / "controls" / (id + ".json");
    bool written = WritePrivate(path, command);
    if (written) { s.pendingRequests.emplace_back(path, NowMs() + 15000); s.lastRequest = id; }
    std::fill(s.password.begin(), s.password.end(), '\0');
    s.lastCommand = ImGui::GetTime();
    s.actionMessage = written ? "Request submitted; waiting for companion status." : "Could not securely write the control request.";
}
void SaveSettings() {
    Json prefs{{"version", 1}, {"high_contrast", s.highContrast}, {"reduced_motion", s.reducedMotion},
        {"detailed", s.detailed}, {"hud_bk", s.hudBk}, {"anchor", s.anchor}, {"palette", s.palette},
        {"text_scale", s.textScale}, {"ui_scale", s.uiScale}, {"compact_seconds", s.compactSeconds},
        {"detail_seconds", s.detailSeconds}, {"floating_x", s.floatingX}, {"floating_y", s.floatingY},
        {"show_performance", s.showPerformance}, {"show_frame_graph", s.showFrameGraph}};
    s.actionMessage = WritePrivate(s.bridge / "ui-settings.json", prefs) ? "Presentation settings saved." : "Could not save settings.";
}
void Poll() {
    // A helper failure survives companion heartbeat loss. Clear it only on an
    // explicit nonfatal launcher record for this same save, not a missing file.
    auto launcher = Read(s.bridge / "launcher-status.json");
    if (launcher.is_object() && launcher.value("version", 0) == 1 && Str(launcher, "save") == s.save
        && launcher.contains("fatal") && launcher["fatal"].is_boolean()) {
        s.launcherFatal = Bool(launcher, "fatal");
        auto helper = Str(launcher, "helper");
        s.failedHelper = helper == "companion" || helper == "logic" || helper == "server" ? helper : "AP service";
    }
    // Remove only requests created by this UI if the consumer died before taking them.
    while (!s.pendingRequests.empty() && s.pendingRequests.front().second < NowMs()) {
        std::error_code ec;
        fs::remove(s.pendingRequests.front().first, ec);
        s.pendingRequests.pop_front();
    }
    auto status = Read(s.bridge / "ui-status.json");
    s.status = Json();
    s.tracker = Json();
    if (!status.is_object() || status.value("version", 0) != 1) return;
    if (Str(status, "save") != s.save) { s.error = "Companion belongs to another save profile."; return; }
    auto identity = status.find("identity");
    if (identity != status.end() && identity->is_object()) {
        if (Str(*identity, "save") != s.save || Str(*identity, "game") != s.game) {
            s.error = "Companion belongs to another game/save. Status ignored."; return;
        }
        auto current = identity->dump();
        if (current != s.identity) {
            s.identity = current; s.recent.clear(); s.notices.clear(); s.seen.clear();
            s.eventsPrimed = false; s.previousBkKnown = false; s.selectedArea.clear(); s.follow = true;
        }
    }
    s.status = status;
    auto results = status.find("control_results");
    if (!s.lastRequest.empty() && results != status.end() && results->is_array()) {
        for (const auto& result : *results) if (Str(result, "id") == s.lastRequest) {
            s.actionMessage = Bool(result, "ok") ? "Request accepted. Check the current connection status below." :
                "Request rejected: " + Str(result, "error", "see companion diagnostic");
            s.lastRequest.clear();
            break;
        }
    }
    auto tracker = status.find("tracker");
    if (tracker != status.end() && tracker->is_object() && identity != status.end() && identity->is_object()
        && tracker->contains("identity") && (*tracker)["identity"] == *identity
        && tracker->contains("revision") && status.contains("revision") && (*tracker)["revision"] == status["revision"])
        s.tracker = *tracker;
    if (!Fresh()) return;
    s.error.clear();
    bool known = LogicKnown() && s.tracker.contains("bk") && s.tracker["bk"].is_boolean();
    if (known) {
        bool bk = s.tracker["bk"].get<bool>();
        if ((!s.previousBkKnown && bk) || (s.previousBkKnown && bk != s.previousBk))
            Notify(bk ? "You are now BK'd." : "You are no longer BK'd.",
                bk ? "No unchecked locations are available in the current logic." : "New checks are available.");
        s.previousBk = bk;
        s.previousBkKnown = true;
    }
    auto events = Read(s.bridge / "ui-events.json");
    if (!events.is_object() || !events.contains("events") || !events["events"].is_array()) return;
    if (!events.contains("identity") || identity == status.end() || !identity->is_object() || events["identity"] != *identity) return;
    // No room-chat events are rendered. Keep only known client event categories.
    for (const auto& event : events["events"]) {
        if (!event.is_object()) continue;
        auto kind = Str(event, "kind"), id = Str(event, "id");
        if (id.empty() || (kind != "received" && kind != "sent" && kind != "check" && kind != "connection" && kind != "goal")) continue;
        if (!s.seen.insert(id).second) continue;
        int flags = event.contains("flags") && event["flags"].is_number_integer() ? event["flags"].get<int>() : 0;
        auto text = Str(event, "text");
        if (text.empty()) text = Str(event, "item_name", "AP activity");
        auto detail = Str(event, "location");
        auto sender = Str(event, "sender");
        if (!sender.empty()) detail += (detail.empty() ? "" : " | ") + std::string("From: ") + sender;
        Notify(text, detail, flags, kind, s.eventsPrimed);
    }
    s.eventsPrimed = true;
}
void BkText() {
    if (!LogicKnown() || !s.tracker.contains("bk") || !s.tracker["bk"].is_boolean()) {
        ImGui::TextUnformatted("Logic status: Unknown");
        return;
    }
    ImGui::TextUnformatted(s.tracker["bk"].get<bool>() ? "BK'd - no checks available in logic" : "Checks available");
}
void Settings() {
    ImGui::Checkbox("Show FPS and frame time", &s.showPerformance);
    if (s.showPerformance) {
        ImGui::Checkbox("Show frame-time graph", &s.showFrameGraph);
        ImGui::TextWrapped("60 FPS is about 16.7 ms per frame. Graph spikes indicate longer frames.");
    }
    ImGui::Separator();
    ImGui::Checkbox("High contrast", &s.highContrast);
    ImGui::Checkbox("Reduced animation", &s.reducedMotion);
    ImGui::SliderFloat("Text size", &s.textScale, .8f, 2.0f, "%.2fx");
    ImGui::SliderFloat("Panel size", &s.uiScale, .8f, 1.6f, "%.2fx");
    ImGui::Combo("Colors", &s.palette, "Standard\0Blue / orange\0");
    ImGui::TextWrapped("Status labels accompany colors. Notifications are silent.");
    ImGui::Checkbox("Detailed notifications", &s.detailed);
    ImGui::SliderFloat("Compact duration", &s.compactSeconds, 1, 15, "%.0f seconds");
    ImGui::SliderFloat("Detailed duration", &s.detailSeconds, 1, 20, "%.0f seconds");
    ImGui::Combo("Notification position", &s.anchor,
        "Bottom left\0Top left\0Top right\0Bottom right\0Center right\0Center left\0Floating\0");
    if (s.anchor == 6) ImGui::Checkbox("Move notification panel", &s.moveNotice);
    else s.moveNotice = false;
    if (ImGui::Button("Reset position")) { s.anchor = 0; s.moveNotice = false; }
    ImGui::Checkbox("Persistent BK notice", &s.hudBk);
    if (ImGui::Button("Save presentation settings")) SaveSettings();
}
void Panel() {
    ImGui::SetNextWindowSize(ImVec2(540 * s.uiScale, 570 * s.uiScale), ImGuiCond_FirstUseEver);
    if (ImGui::Begin("Sly Archipelago - F8", &s.panel)) {
        ImGui::SetWindowFontScale(s.textScale);
        ImGui::TextWrapped("Engineering build - baseline integration is under verification.");
        ImGui::TextWrapped("AP links and ghost players are not available in this build.");
        if (ImGui::BeginTabBar("AP tabs")) {
            if (ImGui::BeginTabItem("Connect")) {
                ImGui::TextWrapped("AP save profile: %s", s.save.c_str());
                ImGui::InputText("Server", s.endpoint.data(), s.endpoint.size());
                ImGui::InputText("Slot", s.slot.data(), s.slot.size());
                ImGui::InputText("Password", s.password.data(), s.password.size(), ImGuiInputTextFlags_Password);
                if (ImGui::Button("Connect")) Request("connect"); ImGui::SameLine();
                if (ImGui::Button("Reconnect")) Request("reconnect"); ImGui::SameLine();
                if (ImGui::Button("Disconnect")) Request("disconnect");
                ImGui::Separator();
                ImGui::Text("Server: %s", !Fresh() ? "Unknown - companion unavailable" : Bool(s.status, "server_connected") ? "Connected" : "Disconnected");
                ImGui::Text("Game: %s", !s.nativeDiagnostic.empty() ? "Adapter needs attention" : !Fresh() ? "Unknown" : Bool(s.status, "game_ready") ? "Ready" : "Not ready");
                if (Fresh() && Bool(s.status, "server_connected") && !Bool(s.status, "game_ready") && s.nativeDiagnostic.empty())
                    ImGui::TextWrapped("To begin: close this panel with F8, then choose Start AP profile or Resume AP profile in the first save slot. Game readiness waits for that selection.");
                if (!s.actionMessage.empty()) ImGui::TextWrapped("%s", s.actionMessage.c_str());
                if (!s.error.empty()) ImGui::TextWrapped("%s", s.error.c_str());
                if (!s.nativeDiagnostic.empty()) ImGui::TextWrapped("Native adapter: %s", s.nativeDiagnostic.c_str());
                if (ImGui::CollapsingHeader("Troubleshooting")) {
                    auto heartbeat = s.status.find("updated_unix_ms");
                    if (heartbeat != s.status.end() && heartbeat->is_number_unsigned()) {
                        auto stamp = heartbeat->get<uint64_t>(), now = NowMs();
                        if (stamp <= now) ImGui::Text("Last companion update: %.1f seconds ago", static_cast<double>(now - stamp) / 1000.0);
                        else ImGui::TextUnformatted("Companion timestamp is ahead of the system clock.");
                    } else ImGui::TextUnformatted("No readable companion heartbeat for this profile.");
                    auto d = Str(s.status, "diagnostic", "No current companion diagnostic.");
                    ImGui::TextWrapped("%s", d.c_str());
                    ImGui::TextWrapped("F8: connection/settings. Hold F9: quick view. F10: full tracker. F11: switch tracker interaction. Game continues running.");
                    if (ImGui::Button("Export diagnostic report")) {
                        // Explicit allowlist: no arbitrary companion errors, server, slot,
                        // identity, paths, password or recent-event text in the export.
                        Json report{{"version", 1}, {"component", "sly-native-ui"},
                            {"created_unix_ms", NowMs()}, {"companion_fresh", Fresh()},
                            {"server_connected", Fresh() && Bool(s.status, "server_connected")},
                            {"game_ready", Fresh() && Bool(s.status, "game_ready")},
                            {"logic_known", LogicKnown()}, {"native_error_present", !s.nativeDiagnostic.empty()}};
                        if (s.tracker.contains("checks") && s.tracker["checks"].is_array())
                            report["check_count"] = s.tracker["checks"].size();
                        auto destination = s.bridge / "diagnostics" / ("ui-" + std::to_string(NowMs()) + ".json");
                        s.actionMessage = WritePrivate(destination, report) ?
                            "Diagnostic saved locally: " + destination.string() : "Could not save diagnostic.";
                    }
                }
                ImGui::EndTabItem();
            }
            if (ImGui::BeginTabItem("Accessibility & display")) { Settings(); ImGui::EndTabItem(); }
            if (ImGui::BeginTabItem("Recent activity")) {
                ImGui::TextUnformatted("Client events from this game session. No room chat.");
                for (auto& n : s.recent) {
                    ImGui::PushStyleColor(ImGuiCol_Text, ItemColor(n.flags));
                    ImGui::TextWrapped("%s", n.text.c_str()); ImGui::PopStyleColor();
                    if (!n.detail.empty()) ImGui::TextWrapped("%s", n.detail.c_str());
                    ImGui::Separator();
                }
                ImGui::EndTabItem();
            }
            if (ImGui::BeginTabItem("Help")) {
                ImGui::TextWrapped("Accessibility is available in the adjacent display tab at any time. This is Sly-specific help; there is no required walkthrough.");
                ImGui::Separator();
                ImGui::TextWrapped("F8: open or close connection, settings and recent activity.");
                ImGui::TextWrapped("Hold F9: show quick view. Release it to hide the view.");
                ImGui::TextWrapped("F10: open or close the full tracker. It opens in view-only mode so you can keep moving Sly.");
                ImGui::TextWrapped("F11: switch between controlling Sly and interacting with the full tracker. The world keeps running while you read or type.");
                ImGui::TextWrapped("Hotkeys are inactive while typing. Closing this panel returns gameplay control unless tracker interaction remains enabled.");
                ImGui::Separator();
                ImGui::TextWrapped("Connect and Reconnect request a connection through the companion. Request acceptance is not proof of server login or game readiness; both statuses are shown separately.");
                ImGui::TextWrapped("This engineering build uses one configured, isolated AP save profile per launch. It does not load the ordinary game save. Profile chooser and persistent password storage are not implemented yet.");
                ImGui::TextWrapped("After connecting, press F8 to return to the game menu. Select Start AP profile or Resume AP profile in the FIRST save slot. Other slots are disabled for this configured run. Game status remains Not ready until the AP profile is selected.");
                ImGui::TextWrapped("Default game keyboard controls: Enter = Start/pause; K = confirm/jump; W/S = menu up/down. If you changed the game's bindings, use your configured keys instead.");
                ImGui::TextWrapped("The tracker uses the current world's logic. BK means no unchecked location is available in logic. Unknown means the current data is incomplete or disconnected; it is not a BK result.");
                ImGui::TextWrapped("Expand Inventory in the full tracker for total received item quantities. Delivered means acknowledged by the native adapter, not the number of unspent lives or charms you currently hold. Pending includes deliveries still waiting or requiring attention.");
                ImGui::TextWrapped("Enable Map in the full tracker to use Slinq17's level map. Number = available checks, ! = blocked, x = completed, > = episode link. Hover a marker for counts; click it to open that level's check list. These are level markers, not individual bottle positions.");
                ImGui::TextWrapped("Notifications are silent. Progression, ordinary items and traps have labels as well as colors. Only client events appear here; this is not room chat.");
                ImGui::Separator();
                ImGui::TextWrapped("Not ready in this build: AP links, ghost players, controller tracker shortcuts, rebinding, full profile management, updates, and the complete RC1 customization set. Native gameplay parity and runtime acceptance are still being verified.");
                ImGui::EndTabItem();
            }
            ImGui::EndTabBar();
        }
    }
    ImGui::End();
}
void DrawMap(const Json& checks) {
    if (s.maps.empty()) {
        ImGui::TextWrapped("Map pack not configured. The check list remains available.");
        return;
    }
    if (s.follow && !s.currentArea.empty()) {
        bool mapped = false;
        for (size_t i = 0; i < s.maps.size(); ++i)
            for (const auto& marker : s.maps[i].markers)
                if (marker.area == s.currentArea) { s.selectedMap = static_cast<int>(i); mapped = true; }
        static const std::map<std::string, std::string> hubs{
            {"Hideout", "Hideout"}, {"Prowling the Grounds", "ToT"}, {"Muggshot's Turf", "SSE"},
            {"Swamp's Dark Center", "VV"}, {"Inside the Stronghold", "FitS"}};
        auto hub = hubs.find(s.currentArea);
        if (!mapped && hub != hubs.end()) for (size_t i = 0; i < s.maps.size(); ++i) if (s.maps[i].name == hub->second) {
            s.selectedMap = static_cast<int>(i); mapped = true;
        }
        if (!mapped) {
            ImGui::TextWrapped("This source pack has no map for the current area.");
            if (ImGui::Button("Browse other maps")) s.follow = false;
            return;
        }
    }
    s.selectedMap = std::clamp(s.selectedMap, 0, static_cast<int>(s.maps.size()) - 1);
    if (ImGui::BeginCombo("Map", s.maps[s.selectedMap].name.c_str())) {
        for (size_t i = 0; i < s.maps.size(); ++i) if (ImGui::Selectable(s.maps[i].name.c_str(), i == s.selectedMap)) {
            s.selectedMap = static_cast<int>(i); s.follow = false;
        }
        ImGui::EndCombo();
    }
    ImGui::TextWrapped("Slinq17 map: markers group checks by level, not bottle positions.");
    ImGui::TextUnformatted("Number: available checks | !: blocked | x: completed | >: episode");
    Map& map = s.maps[s.selectedMap];
    if (!LoadMapTexture(map)) { ImGui::TextUnformatted("Map image could not be loaded."); return; }
    auto avail = ImGui::GetContentRegionAvail();
    float scale = (std::min)((std::max)(100.f, avail.x) / map.width, (std::max)(100.f, avail.y - 18) / map.height);
    ImVec2 origin = ImGui::GetCursorScreenPos(), extent(map.width * scale, map.height * scale);
    ImGui::Image(reinterpret_cast<ImTextureID>(static_cast<intptr_t>(map.texture)), extent);
    auto* draw = ImGui::GetWindowDrawList();
    for (const auto& marker : map.markers) {
        if (marker.x < 0 || marker.y < 0 || marker.x > map.width || marker.y > map.height) continue;
        int available = 0, remaining = 0, total = 0;
        for (const auto& check : checks) if (Str(check, "area") == marker.area) {
            ++total;
            auto state = Str(check, "status");
            if (state == "available") ++available;
            if (state != "completed") ++remaining;
        }
        // Areas with no generated checks do not get misleading availability markers.
        // Hideout's episode links are navigation only, derived from the pack's own names.
        int destination = -1;
        static const char* episodeNames[] = {"Tide of Terror", "Sunset Snake Eyes", "Vicious Voodoo", "Fire in the Sky"};
        static const char* mapNames[] = {"ToT", "SSE", "VV", "FitS"};
        if (map.name == "Hideout") for (int i = 0; i < 4; ++i) if (marker.area == episodeNames[i])
            for (size_t j = 0; j < s.maps.size(); ++j) if (s.maps[j].name == mapNames[i]) destination = static_cast<int>(j);
        if (!total && destination < 0) continue;
        ImVec2 center(origin.x + marker.x * scale, origin.y + marker.y * scale);
        float radius = (std::max)(8.f, 10.f * s.textScale);
        ImVec4 col = available ? ItemColor(1) : remaining ? ImVec4(1, .8f, .3f, 1) : total ? ImVec4(.6f, .6f, .6f, 1) : ImVec4(.7f, .8f, 1, 1);
        draw->AddCircleFilled(center, radius + 2, IM_COL32(0, 0, 0, 255));
        draw->AddCircleFilled(center, radius, ImGui::ColorConvertFloat4ToU32(col));
        std::string label = destination >= 0 ? ">" : available ? std::to_string(available) : remaining ? "!" : "x";
        auto textSize = ImGui::CalcTextSize(label.c_str());
        draw->AddText(ImVec2(center.x - textSize.x / 2, center.y - textSize.y / 2), IM_COL32(0, 0, 0, 255), label.c_str());
        if (s.interact && ImGui::IsMouseHoveringRect(ImVec2(center.x - radius, center.y - radius), ImVec2(center.x + radius, center.y + radius))) {
            ImGui::BeginTooltip();
            ImGui::TextUnformatted(marker.area.c_str());
            if (destination >= 0) ImGui::TextUnformatted("Open episode map");
            else ImGui::Text("%d available / %d remaining / %d total", available, remaining, total);
            ImGui::EndTooltip();
            if (ImGui::IsMouseClicked(ImGuiMouseButton_Left)) {
                s.follow = false;
                if (destination >= 0) s.selectedMap = destination;
                else { s.selectedArea = marker.area; s.mapView = false; }
            }
        }
    }
}
void Inventory() {
    if (!ImGui::CollapsingHeader("Inventory (received items)")) return;
    auto inventory = s.status.find("inventory");
    if (!Fresh() || !Bool(s.status, "server_connected") || Str(s.status, "inventory_status") != "known"
        || inventory == s.status.end() || !inventory->is_array()) {
        ImGui::TextWrapped("Inventory unknown: waiting for complete, current received-item history.");
        return;
    }
    ImGui::TextWrapped("Received totals / acknowledged deliveries. These are not current unspent resources.");
    if (inventory->empty()) { ImGui::TextUnformatted("No items received in this session."); return; }
    float height = (std::min)(180.f * s.uiScale, (std::max)(80.f, ImGui::GetContentRegionAvail().y * .4f));
    ImGui::BeginChild("received inventory", ImVec2(0, height), true);
    for (const auto& item : *inventory) {
        uint64_t received = 0, applied = 0, pending = 0;
        auto name = Str(item, "name", "Unknown item");
        if (!Count(item, "received_count", received)) {
            ImGui::TextWrapped("%s - received quantity unknown", name.c_str());
            continue;
        }
        auto classification = item.find("flags");
        bool classified = classification != item.end() && classification->is_number_integer() &&
            classification->get<double>() >= 0 && classification->get<double>() <= 7;
        int flags = classified ? classification->get<int>() : 0;
        const char* label = !classified ? "Unknown type" : (flags & 4) ? "Trap" :
            (flags & 1) ? "Progression" : (flags & 2) ? "Useful" : "Item";
        ImGui::PushStyleColor(ImGuiCol_Text, ItemColor(flags));
        ImGui::TextWrapped("[%s] %s - received %llu", label, name.c_str(), static_cast<unsigned long long>(received));
        ImGui::PopStyleColor();
        bool deliveryKnown = Count(item, "applied_count", applied) && Count(item, "pending_count", pending)
            && applied <= received && pending == received - applied;
        if (deliveryKnown) ImGui::Text("Delivered %llu | Pending %llu", static_cast<unsigned long long>(applied), static_cast<unsigned long long>(pending));
        else ImGui::TextUnformatted("Delivery status unknown");
        if (Str(item, "classification_source") == "local_precollected_catalog")
            ImGui::TextUnformatted("Starting item classification: matching Sly world catalog");
        ImGui::Separator();
    }
    ImGui::EndChild();
}
void Tracker(bool quick) {
    ImGuiWindowFlags flags = quick || !s.interact ? ImGuiWindowFlags_NoInputs | ImGuiWindowFlags_NoFocusOnAppearing : 0;
    if (quick) flags |= ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_AlwaysAutoResize;
    ImGui::SetNextWindowSize(ImVec2((quick ? 350 : 650) * s.uiScale, (quick ? 240 : 500) * s.uiScale), ImGuiCond_FirstUseEver);
    if (ImGui::Begin(quick ? "AP Quick View" : "AP Tracker - F10 / F11 to interact", quick ? nullptr : &s.full, flags)) {
        ImGui::SetWindowFontScale(s.textScale);
        if (!quick) {
            ImGui::TextUnformatted(s.interact ? "Focus: tracker (world keeps running)" : "Focus: Sly (F11 to interact)");
            Inventory();
        }
        BkText();
        if (!LogicKnown()) {
            auto reason = Str(s.tracker, "reason", "Waiting for a current, complete logic snapshot.");
            ImGui::TextWrapped("%s", reason.c_str());
            ImGui::End(); return;
        }
        const auto& checks = s.tracker["checks"];
        if (!checks.is_array()) { ImGui::TextUnformatted("Check data unavailable."); ImGui::End(); return; }
        std::map<std::string, std::pair<int, int>> areas;
        for (const auto& c : checks) {
            auto& count = areas[Str(c, "area", "Unknown area")];
            if (Str(c, "status") == "available") ++count.first;
            if (Str(c, "status") != "completed") ++count.second;
        }
        if (!quick) {
            if (ImGui::Button("Follow Sly")) { s.follow = true; s.selectedArea.clear(); }
            ImGui::SameLine(); ImGui::Checkbox("Map", &s.mapView);
            const char* selected = s.selectedArea.empty() ? "All areas" : s.selectedArea.c_str();
            if (ImGui::BeginCombo("Area", selected)) {
                if (ImGui::Selectable("All areas", s.selectedArea.empty())) { s.follow = false; s.selectedArea.clear(); }
                for (const auto& area : areas) if (ImGui::Selectable(area.first.c_str(), area.first == s.selectedArea)) {
                    s.selectedArea = area.first; s.follow = false;
                }
                ImGui::EndCombo();
            }
            ImGui::InputText("Find check", s.search.data(), s.search.size());
        }
        std::string areaFilter = (quick || s.follow) ? s.currentArea : s.selectedArea;
        if (quick && areaFilter.empty()) ImGui::TextUnformatted("Current area unknown - showing all areas.");
        else if (!areaFilter.empty()) ImGui::TextWrapped("Area: %s", areaFilter.c_str());
        if (!quick && s.mapView) {
            DrawMap(checks);
        } else {
            ImGui::BeginChild(quick ? "quick checks" : "full checks", ImVec2(0, quick ? 140 * s.uiScale : 0), false);
            unsigned shown = 0;
            for (const auto& c : checks) {
                if (!areaFilter.empty() && Str(c, "area") != areaFilter) continue;
                auto name = Str(c, "name", "Unnamed check"), state = Str(c, "status", "unknown");
                if (quick && state != "available") continue;
                if (!quick && s.search[0] && name.find(s.search.data()) == std::string::npos) continue;
                ImVec4 color = state == "available" ? ItemColor(1) : state == "completed" ? ImVec4(.65f, .65f, .65f, 1) : ImVec4(1, 1, 1, 1);
                ImGui::PushStyleColor(ImGuiCol_Text, color);
                ImGui::TextWrapped("[%s] %s", state.c_str(), name.c_str()); ImGui::PopStyleColor();
                if (!quick && state == "unavailable") {
                    auto why = Str(c, "requirements", "Detailed requirements unavailable; no guess made.");
                    ImGui::TextWrapped("  %s", why.c_str());
                }
                if (quick && ++shown >= 6) break;
            }
            ImGui::EndChild();
        }
    }
    ImGui::End();
}
void Notifications() {
    const auto now = ImGui::GetTime();
    while (!s.notices.empty() && s.notices.front().until < now) s.notices.pop_front();
    bool bk = s.hudBk && LogicKnown() && Bool(s.tracker, "bk");
    if ((s.notices.empty() && !bk && !s.moveNotice) || s.cinematic) return;
    auto size = ImGui::GetIO().DisplaySize;
    float width = (std::min)(430.0f * s.uiScale, size.x - 32.0f);
    ImVec2 pivot(0, 1), pos(16, size.y - 16);
    if (s.anchor == 1) { pivot = ImVec2(0, 0); pos = ImVec2(16, 16); }
    if (s.anchor == 2) { pivot = ImVec2(1, 0); pos = ImVec2(size.x - 16, 16); }
    if (s.anchor == 3) { pivot = ImVec2(1, 1); pos = ImVec2(size.x - 16, size.y - 16); }
    if (s.anchor == 4) { pivot = ImVec2(1, .5f); pos = ImVec2(size.x - 16, size.y / 2); }
    if (s.anchor == 5) { pivot = ImVec2(0, .5f); pos = ImVec2(16, size.y / 2); }
    if (s.anchor == 6) { pivot = ImVec2(0, 0); pos = ImVec2(std::clamp(s.floatingX, 0.f, (std::max)(0.f, size.x - width)), std::clamp(s.floatingY, 0.f, (std::max)(0.f, size.y - 100))); }
    ImGui::SetNextWindowPos(pos, s.moveNotice ? ImGuiCond_Once : ImGuiCond_Always, pivot);
    ImGui::SetNextWindowSize(ImVec2(width, 0));
    ImGuiWindowFlags flags = ImGuiWindowFlags_AlwaysAutoResize | ImGuiWindowFlags_NoFocusOnAppearing | ImGuiWindowFlags_NoSavedSettings;
    if (!s.moveNotice) flags |= ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_NoInputs;
    if (ImGui::Begin("AP notifications - drag to position", nullptr, flags)) {
        ImGui::SetWindowFontScale(s.textScale);
        if (s.moveNotice) { auto p = ImGui::GetWindowPos(); s.floatingX = p.x; s.floatingY = p.y; ImGui::TextUnformatted("Turn off Move panel to lock."); }
        if (bk) ImGui::TextWrapped("You are BK'd. No checks are available in logic.");
        for (const auto& n : s.notices) {
            ImGui::PushStyleColor(ImGuiCol_Text, ItemColor(n.flags));
            const char* category = (n.flags & 4) ? "Trap" : (n.flags & 1) ? "Progression" : "Item";
            if (n.kind == "received" || n.kind == "sent") ImGui::TextWrapped("[%s] %s", category, n.text.c_str());
            else ImGui::TextWrapped("%s", n.text.c_str());
            ImGui::PopStyleColor();
            if (s.detailed && !n.detail.empty()) ImGui::TextWrapped("%s", n.detail.c_str());
        }
    }
    ImGui::End();
}
void SamplePerformance() {
    const auto now = std::chrono::steady_clock::now();
    if (s.previousFrame != std::chrono::steady_clock::time_point{}) {
        const double ms = std::chrono::duration<double, std::milli>(now - s.previousFrame).count();
        if (ms > 0) {
            s.frameTimes[s.frameNext] = static_cast<float>(ms);
            s.frameNext = (s.frameNext + 1) % static_cast<int>(s.frameTimes.size());
            s.frameCount = (std::min)(s.frameCount + 1, static_cast<int>(s.frameTimes.size()));
            s.frameWindowMs += ms;
            ++s.frameWindowCount;
            if (s.frameWindowMs >= 250) {
                s.measuredFrameMs = static_cast<float>(s.frameWindowMs / s.frameWindowCount);
                s.measuredFps = static_cast<float>(1000.0 * s.frameWindowCount / s.frameWindowMs);
                s.frameWindowMs = 0;
                s.frameWindowCount = 0;
            }
        }
    }
    s.previousFrame = now;
}
void PerformanceOverlay() {
    if (!s.showPerformance) return;
    const auto* viewport = ImGui::GetMainViewport();
    ImGui::SetNextWindowPos(ImVec2(viewport->WorkPos.x + viewport->WorkSize.x - 12,
        viewport->WorkPos.y + 12), ImGuiCond_Always, ImVec2(1, 0));
    ImGui::SetNextWindowBgAlpha(s.highContrast ? 1.f : .85f);
    const auto flags = ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_AlwaysAutoResize |
        ImGuiWindowFlags_NoInputs | ImGuiWindowFlags_NoFocusOnAppearing | ImGuiWindowFlags_NoSavedSettings;
    if (ImGui::Begin("AP performance", nullptr, flags)) {
        ImGui::SetWindowFontScale(s.textScale);
        if (s.measuredFps > 0)
            ImGui::Text("%.1f FPS | %.2f ms/frame", s.measuredFps, s.measuredFrameMs);
        else ImGui::TextUnformatted("Measuring frame time...");
        if (s.showFrameGraph && s.frameCount) {
            float peak = 0;
            for (int i = 0; i < s.frameCount; ++i) peak = (std::max)(peak, s.frameTimes[i]);
            const int offset = s.frameCount == static_cast<int>(s.frameTimes.size()) ? s.frameNext : 0;
            ImGui::Text("Recent peak: %.2f ms | 60 FPS: 16.67 ms", peak);
            ImGui::PlotLines("##frame-times", s.frameTimes.data(), s.frameCount, offset,
                "Frame time (ms)", 0.f, (std::max)(33.34f, peak * 1.1f), ImVec2(310 * s.uiScale, 85 * s.uiScale));
        }
    }
    ImGui::End();
}
void LauncherWarning() {
    if (!s.launcherFatal) return;
    auto viewport = ImGui::GetIO().DisplaySize;
    ImGui::SetNextWindowPos(ImVec2(viewport.x * .5f, 12), ImGuiCond_Always, ImVec2(.5f, 0));
    ImGui::SetNextWindowSize(ImVec2((std::min)(600.f * s.uiScale, (std::max)(180.f, viewport.x - 24)), 0));
    ImGui::SetNextWindowBgAlpha(.96f);
    constexpr ImGuiWindowFlags flags = ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_NoInputs |
        ImGuiWindowFlags_NoFocusOnAppearing | ImGuiWindowFlags_NoSavedSettings | ImGuiWindowFlags_AlwaysAutoResize;
    if (ImGui::Begin("AP service failure", nullptr, flags)) {
        ImGui::SetWindowFontScale(s.textScale);
        ImGui::PushStyleColor(ImGuiCol_Text, ItemColor(4));
        ImGui::TextWrapped("AP service stopped: %s", s.failedHelper.c_str());
        ImGui::PopStyleColor();
        // Use fixed user-facing wording, never arbitrary launcher message/log content.
        ImGui::TextWrapped("Gameplay remains open. AP services are degraded and tracker logic is unknown. Open F8 for status; the stopped helper needs to be restarted.");
    }
    ImGui::End();
}
} // namespace

bool Initialize(const fs::path& absoluteProfilePath) {
    Shutdown();
    if (!absoluteProfilePath.is_absolute()) return false;
    auto profile = Read(absoluteProfilePath);
    if (!profile.is_object()) return false;
    s.profile = absoluteProfilePath;
    s.save = Str(profile, "save"); s.game = Str(profile, "game");
    auto dir = Str(profile, "bridge_dir");
    if (s.save.empty() || s.game.empty() || dir.empty()) return false;
    s.bridge = fs::path(dir);
    if (s.bridge.is_relative()) s.bridge = absoluteProfilePath.parent_path() / s.bridge;
    auto endpoint = Str(profile, "endpoint"), slot = Str(profile, "slot_name");
    std::copy_n(endpoint.data(), (std::min)(endpoint.size(), s.endpoint.size() - 1), s.endpoint.data());
    std::copy_n(slot.data(), (std::min)(slot.size(), s.slot.size() - 1), s.slot.data());
    auto p = Read(s.bridge / "ui-settings.json");
    if (p.is_object()) {
        try {
            s.highContrast = Bool(p, "high_contrast"); s.reducedMotion = Bool(p, "reduced_motion");
            s.detailed = Bool(p, "detailed"); s.hudBk = Bool(p, "hud_bk");
              s.showPerformance = Bool(p, "show_performance"); s.showFrameGraph = Bool(p, "show_frame_graph");
            s.anchor = std::clamp(p.value("anchor", 0), 0, 6); s.palette = std::clamp(p.value("palette", 0), 0, 1);
            s.textScale = std::clamp(p.value("text_scale", 1.f), .8f, 2.f); s.uiScale = std::clamp(p.value("ui_scale", 1.f), .8f, 1.6f);
            s.compactSeconds = std::clamp(p.value("compact_seconds", 4.f), 1.f, 15.f);
            s.detailSeconds = std::clamp(p.value("detail_seconds", 7.f), 1.f, 20.f);
            s.floatingX = p.value("floating_x", 20.f); s.floatingY = p.value("floating_y", 100.f);
        } catch (const Json::exception&) { s.error = "Some saved presentation settings were invalid."; }
    }
    s.enabled = true;
    return true;
}
bool SetMapPackPath(const fs::path& directory) {
    if (!directory.is_absolute() || !s.maps.empty()) return false;
    try {
        auto maps = ReadPackJson(directory / "maps" / "maps.jsonc");
        if (!maps.is_array()) return false;
        for (const auto& entry : maps) {
            auto image = fs::path(Str(entry, "img"));
            if (image.empty() || image.is_absolute()) continue;
            bool traversal = false;
            for (const auto& part : image) if (part == "..") traversal = true;
            if (traversal) continue;
            Map map; map.name = Str(entry, "name"); map.image = directory / image;
            if (!map.name.empty()) s.maps.push_back(std::move(map));
        }
        for (const char* name : {"hideout", "tide_of_terror", "sunset_snake_eyes", "vicious_voodoo", "fire_in_the_sky"})
            ReadMarkers(ReadPackJson(directory / "locations" / (std::string(name) + ".jsonc")));
        return !s.maps.empty();
    } catch (const std::exception&) { s.maps.clear(); return false; }
}
void Frame() {
    s.capture = false;
    if (!s.enabled) return;
    SamplePerformance();
    s.capture = s.panel || (s.full && s.interact) || s.moveNotice;
    try {
        if (ImGui::GetTime() >= s.nextPoll) {
            s.nextPoll = ImGui::GetTime() + .25;
            try { Poll(); } catch (const std::exception&) {
                s.error = "Invalid companion data. Connection information is unknown.";
                s.status = Json(); s.tracker = Json();
            }
        }
        auto& io = ImGui::GetIO();
        if (!io.WantTextInput) {
            if (ImGui::IsKeyPressed(ImGuiKey_F8, false)) s.panel = !s.panel;
            if (ImGui::IsKeyPressed(ImGuiKey_F10, false)) { s.full = !s.full; if (!s.full) s.interact = false; }
            if (s.full && ImGui::IsKeyPressed(ImGuiKey_F11, false)) s.interact = !s.interact;
        }
        const bool pushedContrast = s.highContrast;
        if (pushedContrast) {
            ImGui::PushStyleColor(ImGuiCol_WindowBg, ImVec4(.015f, .015f, .015f, 1));
            ImGui::PushStyleColor(ImGuiCol_Text, ImVec4(1, 1, 1, 1));
        }
        if (s.panel) Panel();
        if (s.full) Tracker(false);
        if (!io.WantTextInput && ImGui::IsKeyDown(ImGuiKey_F9)) Tracker(true);
        Notifications();
        PerformanceOverlay();
        LauncherWarning();
        if (pushedContrast) ImGui::PopStyleColor(2);
        s.capture = s.panel || (s.full && s.interact) || s.moveNotice;
    } catch (const std::exception&) {
        // Malformed external metadata must never unwind into gameplay. Do not echo input,
        // since diagnostics may otherwise inadvertently expose credentials.
        s.error = "Invalid companion UI data. Waiting for a valid update.";
        s.status = Json(); s.tracker = Json();
    }
}
bool WantsGameInputCapture() { return s.enabled && s.capture; }
void SetCurrentArea(const std::string& area) { s.currentArea = area; }
void SetNativeDiagnostic(const std::string& message) { s.nativeDiagnostic = message.substr(0, 4096); }
void SetCinematic(bool deferNotifications) {
    if (s.enabled && s.cinematic && !deferNotifications && ImGui::GetCurrentContext()) {
        // FMV playback blocks normal game frames. Ingest its accumulated events before
        // dropping the deferral flag; the caller invokes this after ImGui NewFrame.
        try { Poll(); } catch (const std::exception&) { s.error = "Companion data unavailable after cinematic."; }
        int count = s.deferred; s.deferred = 0; s.cinematic = false;
        if (count) Notify(std::to_string(count) + " AP updates received.", "See Recent activity for individual events.");
    }
    s.cinematic = deferNotifications;
}
void Shutdown() {
    std::fill(s.password.begin(), s.password.end(), '\0');
    for (const auto& request : s.pendingRequests) {
        std::error_code ec; fs::remove(request.first, ec);
    }
    for (const auto& map : s.maps) if (map.texture) glDeleteTextures(1, &map.texture);
    s = State{};
}
} // namespace apui
