#pragma once
#include <filesystem>
#include <fstream>
#include <stdexcept>
#include <string>
#ifdef _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

namespace slyapio {
#ifdef _WIN32
inline bool ReplacePublished(const std::filesystem::path& temporary,const std::filesystem::path& target) {
    // Same already-flushed bytes, bounded retry only. Never re-run game effects.
    for(int attempt=0;attempt<11;++attempt) {
        if(MoveFileExW(temporary.c_str(),target.c_str(),MOVEFILE_REPLACE_EXISTING|MOVEFILE_WRITE_THROUGH))return true;
        const auto code=GetLastError();
        if(code!=ERROR_ACCESS_DENIED&&code!=ERROR_SHARING_VIOLATION&&code!=ERROR_LOCK_VIOLATION&&code!=ERROR_NO_SYSTEM_RESOURCES)return false;
        if(attempt<10)Sleep(20);
    }
    return false;
}
#endif
// Published bridge documents are replaced atomically. Windows readers MUST
// share deletion as well as reading/writing, or they can kill the publisher.
inline std::string ReadText(const std::filesystem::path& path,size_t limit) {
#ifdef _WIN32
    HANDLE file=CreateFileW(path.c_str(),GENERIC_READ,FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,
                           nullptr,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,nullptr);
    if(file==INVALID_HANDLE_VALUE) {
        const auto code=GetLastError();
        if(code==ERROR_FILE_NOT_FOUND||code==ERROR_PATH_NOT_FOUND)return {};
        throw std::runtime_error("Cannot read AP document");
    }
    std::string text(limit+1,'\0');DWORD read=0;
    const bool ok=ReadFile(file,text.data(),static_cast<DWORD>(text.size()),&read,nullptr);
    CloseHandle(file);
    if(!ok)throw std::runtime_error("Cannot read AP document");
    if(read>limit)throw std::runtime_error("AP document exceeds size bound");
    text.resize(read);return text;
#else
    if(!std::filesystem::exists(path))return {};
    std::ifstream file(path,std::ios::binary);
    if(!file)throw std::runtime_error("Cannot read AP document");
    std::string text(limit+1,'\0');file.read(text.data(),static_cast<std::streamsize>(text.size()));
    if(file.gcount()>static_cast<std::streamsize>(limit))throw std::runtime_error("AP document exceeds size bound");
    text.resize(static_cast<size_t>(file.gcount()));return text;
#endif
}
}
