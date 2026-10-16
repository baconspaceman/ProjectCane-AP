#pragma once
#include <array>
#include <condition_variable>
#include <filesystem>
#include <functional>
#include <mutex>
#include <optional>
#include <string>
#include <thread>
#include <nlohmann/json.hpp>

namespace slyap {
// Only disposable status snapshots belong here. Saves, item receipts and ACKs
// must stay on their synchronous transaction path. No game pointers cross threads.
class StatusPublisher {
public:
    using Json = nlohmann::json;
    using Batch = std::array<std::pair<std::filesystem::path, Json>, 2>;
    using Writer = std::function<void(const std::filesystem::path&, const Json&)>;
    explicit StatusPublisher(Writer writer) : writer_(std::move(writer)), thread_([this]{ Run(); }) {}
    ~StatusPublisher() {
        { std::lock_guard lock(mutex_); stopping_ = true; }
        changed_.notify_one();
        thread_.join();
    }
    void Submit(Batch batch) {
        { std::lock_guard lock(mutex_); pending_ = std::move(batch); }
        changed_.notify_one();
    }
    std::string Error() { std::lock_guard lock(mutex_); return error_; }
    void Drain() { std::unique_lock lock(mutex_); drained_.wait(lock,[this]{return !pending_ && !busy_;}); }
private:
    void Run() {
        for (;;) {
            Batch batch;
            {
                std::unique_lock lock(mutex_);
                changed_.wait(lock,[this]{return stopping_ || pending_.has_value();});
                if (!pending_) return;
                batch = std::move(*pending_); pending_.reset(); busy_ = true;
            }
            std::string failure;
            try { for (const auto& [path, value] : batch) writer_(path,value); }
            catch (const std::exception& e) { failure = e.what(); }
            catch (...) { failure = "AP status publisher failed"; }
            {
                std::lock_guard lock(mutex_); error_ = std::move(failure); busy_ = false;
                if (!pending_) drained_.notify_all();
            }
        }
    }
    Writer writer_;
    std::mutex mutex_;
    std::condition_variable changed_, drained_;
    std::optional<Batch> pending_;
    bool stopping_ = false, busy_ = false;
    std::string error_;
    std::thread thread_;
};
}
