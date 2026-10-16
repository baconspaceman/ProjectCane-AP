#include "ap_bridge.h"
#include "ap_fileio.h"
#include "ap_status_publisher.h"
#include "ap_traps.h"
#include "jt.h"
#include "po.h"
#include "wm.h"
#include "binoc.h"
#include "game.h"
#include "save.h"
#include "sw.h"
#include "ui.h"
#include "fmv.h"
#include "debug.h"
#include "platform.h"
#include <nlohmann/json.hpp>
#include <windows.h>
#include <algorithm>
#include <array>
#include <chrono>
#include <cstring>
#include <fstream>
#include <map>
#include <set>
#include <stdexcept>
#include <thread>
#include <atomic>
#include <random>
#include <memory>
#include <future>

namespace slyap {
using Json = nlohmann::json;
namespace {
constexpr size_t FrameLimit = 1048576;
bool enabled = false, bound = false;
HANDLE roleLock = INVALID_HANDLE_VALUE;
std::filesystem::path profilePath, bridgeDir, savePath;
Json profile, handshake, state, mappings;
std::string boot, error, lastAttempt;
uint64_t heartbeat = 0;
std::unique_ptr<StatusPublisher> publisher;
std::map<std::string, int64_t> locationIds;
std::map<int64_t, std::string> itemNames;
std::set<int64_t> allowed;
auto lastTick = std::chrono::steady_clock::time_point{};
auto lastTrapTick = std::chrono::steady_clock::now();
const char* levels[4][8] = {
 {"Stealthy Approach","Prowling the Grounds","High Class Heist","Into the Machine","Cunning Disguise","Fire Down Below","Treasure in the Depths","Gunboat Graveyard"},
 {"Rocky Start","Muggshot's Turf","Boneyard Casino","Murray's Big Gamble","At the Dog Track","Two to Tango","Straight to the Top","Back Alley Heist"},
 {"Dread Swamp Path","The Swamp's Dark Center","Lair of the Beast","Grave Undertaking","Piranha Lake","Descent into Danger","Ghastly Voyage","Down Home Cooking"},
 {"Perilous Ascent","Inside the Stronghold","Flaming Temple of Flame","Unseen Foe","King of the Hill","Rapid Fire Assault","Duel by the Dragon","Desperate Race"}
};
const char* bosses[5] = {"Eye of the Storm","Last Call","Deadly Dance","Flame Fu!","Beat Clockwerk"};
const char* events[5] = {"Beat Raleigh","Beat Muggshot","Beat Mz. Ruby","Beat Panda King","Victory"};
const std::vector<std::vector<uint32_t>> moveMasks = {
 {2,16},{4,1024},{8,4096,32768},{32},{64},{128},{256,16384},{512},{2048},{65536,8192},
 {0x20000000},{0x40000000},{0x80000000},{0x10000000}
};
Json Read(const std::filesystem::path& p) {
 auto data=slyapio::ReadText(p,FrameLimit);
 return data.empty()?Json(nullptr):Json::parse(data);
}
void Atomic(const std::filesystem::path& p,const Json& value,bool durable=true) {
 auto data=value.dump(); if(data.size()>FrameLimit) throw std::runtime_error("AP publication exceeds bound");
 std::filesystem::create_directories(p.parent_path());
 auto tmp=p;tmp+=L".tmp";
 HANDLE h=CreateFileW(tmp.c_str(),GENERIC_WRITE,0,nullptr,CREATE_ALWAYS,FILE_ATTRIBUTE_NORMAL,nullptr);
 if(h==INVALID_HANDLE_VALUE) throw std::runtime_error("Cannot create AP transaction");
 DWORD written=0;
 bool ok=WriteFile(h,data.data(),static_cast<DWORD>(data.size()),&written,nullptr)&&written==data.size()&&(!durable||FlushFileBuffers(h));
 CloseHandle(h);
 if(!ok) throw std::runtime_error("Cannot flush AP transaction");
 if(!slyapio::ReplacePublished(tmp,p))
  throw std::runtime_error("Cannot commit AP transaction");
}
std::filesystem::path Resolve(const std::string& s) {
 auto p=std::filesystem::u8path(s);return p.is_absolute()?p:profilePath.parent_path()/p;
}
int Option(const char* key,int fallback=0) {
 if(!bound||!handshake.contains("slot_data")) return fallback;
 const auto& s=handshake["slot_data"];
 return s.contains(key)&&s[key].is_number_integer()?s[key].get<int>():fallback;
}
bool Safe() {
 return bound && g_psw && g_pgsCur && g_saveData.pgsCurrentSave==&g_saveData.saveData[0]
  && !FCutscenePlaying() && (g_ui.uis==UIS_Playing||g_ui.uis==UIS_Hub);
}
bool SafeTraps() {
 if(!Safe()||g_ui.uis!=UIS_Playing||!g_pjt||PpoCur()!=g_pjt||g_binoc.fActive)return false;
 const int w=g_pgsCur->gameWorldCur,l=g_pgsCur->worldLevelCur;
 if(w<1||w>4||l<0||l>8)return false;
 if((w==1&&l==6)||(w==2&&(l==3||l==4))||
    (w==3&&(l==4||l==6||l==7||l==8))||(w==4&&(l==4||l==5||l==7)))return false;
 return static_cast<unsigned int>(g_pjt->jts)<3 || g_pjt->jts==13;
}
std::vector<uint8_t> Bytes(const GS& s) {
 const auto* b=reinterpret_cast<const uint8_t*>(&s);return {b,b+sizeof(GS)};
}
void Persist(Json next,const GS* game=nullptr) {
 if(game) next["game"]=Bytes(*game);
 if(std::filesystem::exists(savePath)){
  auto previous=Read(savePath);
  if(previous.is_null()||previous.at("version")!=1||previous.at("identity")!=state.at("identity"))
   throw std::runtime_error("Existing AP save cannot be safely backed up");
  auto backup=savePath;backup+=L".bak";Atomic(backup,previous);
 }
 Atomic(savePath,next);state=std::move(next);
}
int Count(int64_t item,const Json& source=state) {
 auto k=std::to_string(item);return source.at("counts").value(k,0);
}
void DeriveInventory(GS& s,const Json& source=state) {
 uint32_t moves=0;
 for(size_t i=0;i<moveMasks.size();++i){int n=Count(10020001+i,source);for(int j=0;j<n&&j<static_cast<int>(moveMasks[i].size());++j)moves|=moveMasks[i][j];}
 // Match original Sly client's safeguard against auto-opening/empty vaults.
 if((moves&0xf001fffeU)==0xf001fffeU)moves&=~2048U;
 s.grfvault=static_cast<int>(moves);
 for(int w=1;w<=4;++w){s.aws[w].ckey=Count(10020014+w,source);s.aws[w].fws=(s.aws[w].fws&~1)|(Count(10020020+w,source)>0?1:0);}
 bool final=Option("UnlockClockwerk",1)==1 ? source.at("bosses").size()>=static_cast<size_t>(Option("RequiredBosses",4)) : Count(10020049,source)>=Option("RequiredPages",20);
 s.aws[5].fws=(s.aws[5].fws&~1)|(final?1:0);
 int bundle=Option("ItemCluesanityBundleSize");
 if(bundle>0) {
  const int pos[][2]={{1,0},{1,3},{1,2},{1,5},{1,4},{1,7},{2,0},{2,2},{2,6},{2,5},{2,7},{3,0},{3,2},{3,3},{3,5},{4,0},{4,2},{4,3},{4,6}};
  const int caps[]={20,30,30,30,30,20,40,40,40,30,30,20,30,40,40,30,25,30,40};
  for(int i=0;i<19;++i) s.aws[pos[i][0]].als[pos[i][1]].abitClue.m_cbitSet=std::min(caps[i],Count(10020030+i,source)*bundle);
 }
}
void ProcessTraps(bool ready) {
 auto& runtime=aptraps::State();
 const GS* safeGame=Safe()?g_pgsCur:nullptr;
 Json next=state;bool changed=false,started=false;
 for(auto& entry:next["trap_queue"]) {
  if(entry["status"]=="started" && entry.value("boot","")!=boot) {
   entry["status"]="interrupted";changed=true;
  }
  if(entry["status"]=="started")started=true;
 }
 if(changed)Persist(next,safeGame);
 if(const auto* finished=runtime.Finished()) {
  if(finished->item==aptraps::Ball && g_pjt && PpoCur()==g_pjt &&
     g_pjt->jts==13 && g_pjt->jtbs!=56)SetJtJts(g_pjt,13,56);
  const auto delivery=finished->delivery;next=state;
  for(auto& entry:next["trap_queue"])if(entry["delivery"]==delivery)entry["status"]="finished";
  Persist(next,safeGame);runtime.ClearFinishedCommitted(delivery);started=false;
 }
 if(!ready||started)return;
 next=state;
 for(auto& entry:next["trap_queue"])if(entry["status"]=="queued") {
  aptraps::Record record{entry.at("delivery").get<std::string>(),entry.at("item").get<int64_t>(),entry.value("variant",0)};
  if(!runtime.Supports(record.item))return;
  entry["status"]="started";entry["boot"]=boot;
  Persist(next,safeGame);
  if(!runtime.StartCommitted(record))throw std::runtime_error("Native trap state needs review");
  break;
 }
}
int PhysicalBottles(const LS& s) {
 int count=0;for(auto b:s.abitClue.m_ab)for(int bit=0;bit<8;++bit)count+=(b>>bit)&1;return count;
}
void AddCheck(Json& checks,const std::string& name) {
 auto i=locationIds.find(name);if(i!=locationIds.end()&&allowed.count(i->second))checks[std::to_string(i->second)]=true;
}
void ObserveChecks() {
 if(!g_pgsCur)return;
 Json next=state;auto& checks=next["checks"];
 if(g_pgsCur->aws[0].cvault>0)AddCheck(checks,"Paris Files");
 for(int w=1;w<=4;++w) for(int l=0;l<8;++l) {
  const auto& ls=g_pgsCur->aws[w].als[l];const std::string level=levels[w-1][l];
  if(ls.grfls&2){AddCheck(checks,level+": Key");for(int n=1;n<=10;++n){AddCheck(checks,level+": Minigame Cache #"+std::to_string(n));AddCheck(checks,level+": Key Cache #"+std::to_string(n));}}
  if(ls.grfls&4)AddCheck(checks,level+": Vault");
  if(ls.grfls&8)AddCheck(checks,level+": Hourglass");
  for(int n=1;n<=PhysicalBottles(ls);++n)AddCheck(checks,level+": Bottle #"+std::to_string(n));
 }
 for(int w=1;w<=5;++w) if(g_pgsCur->aws[w].fws&FWS_Complete){
  AddCheck(checks,bosses[w-1]);
  if(w<=4)next["bosses"][events[w-1]]=true;
  else next["goal"]=true;
 }
 // Persist physical observations with the same native state that established them.
 if(next!=state)Persist(next,g_pgsCur);
}
void Publish(bool ready) {
 Json checks=Json::array();for(auto it=state["checks"].begin();it!=state["checks"].end();++it){auto id=std::stoll(it.key());if(allowed.count(id))checks.push_back(id);}
 Json snap={{"version",1},{"contract",profile.at("contract")},{"challenge",handshake.at("challenge")},{"identity",handshake.at("identity")},{"boot",boot},{"heartbeat",++heartbeat},
 {"capabilities",{{"locations",true},{"goal",true},{"permanent_effects",true},{"repeatable_effects",true},{"death_link",false}}},
 {"ready",ready},{"checked_locations",checks},{"goal",state.value("goal",false)},{"receipts",state["receipts"]},{"permanent_states",Json::object()}};
 Json observed=Json::array();for(auto it=state["bosses"].begin();it!=state["bosses"].end();++it)observed.push_back(it.key());if(state.value("goal",false))observed.push_back("Victory");
 Json facts={{"version",1},{"identity",handshake["identity"]},{"challenge",handshake["challenge"]},{"boot",boot},{"observed_events",observed},{"events_complete",true}};
 publisher->Submit({{{bridgeDir/"snapshot.json",std::move(snap)},{bridgeDir/"game-facts.json",std::move(facts)}}});
}
void Acknowledge(const Json& c,const Json& outcome) {
 Json a;for(auto key:{"version","challenge","boot","attempt","identity","effect"})a[key]=c.at(key);a["outcome"]=outcome;Atomic(bridgeDir/"ack.json",a);lastAttempt=c.at("attempt").get<std::string>();
}
void Commands(bool ready) {
 Json c=Read(bridgeDir/"command.json");if(c.is_null())return;
 if(c.at("version")!=1||c.at("identity")!=handshake.at("identity")||c.at("challenge")!=handshake.at("challenge")||c.at("boot")!=boot)return;
 auto attempt=c.at("attempt").get<std::string>();if(attempt==lastAttempt)return;
 const auto& effect=c.at("effect");auto id=effect.at("id").get<std::string>();
 if(state["receipts"].contains(id)){
  if(state["receipts"][id]!=effect){Acknowledge(c,{{"Rejected","delivery key reused with different effect"}});return;}
  Acknowledge(c,"AlreadySatisfied");return;
 }
 if(!ready){Acknowledge(c,{{"NotReady","waiting for safe native game/save state"}});return;}
 int64_t item=effect.at("payload").at("item_id").get<int64_t>();
 if(item<10020001||item>10020049){Acknowledge(c,{{"Rejected","unsupported Sly item identity"}});return;}
 const bool trap=item>=aptraps::Ice&&item<=aptraps::Invisibility;
 if(trap) {
  if(!aptraps::State().Supports(item)){Acknowledge(c,{{"Rejected","native trap hook unavailable"}});return;}
  size_t pending=0;for(const auto& entry:state["trap_queue"])if(entry["status"]=="queued"||entry["status"]=="started")++pending;
  if(pending>=aptraps::MaximumPending){Acknowledge(c,{{"NotReady","native trap queue is full"}});return;}
 }
 if(state["receipts"].size()>=10000){Acknowledge(c,{{"Rejected","receipt capacity exceeded"}});return;}
 Json next=state;
 next["receipts"][id]=effect;
 const auto key=std::to_string(item);next["counts"][key]=next["counts"].value(key,0)+1;
 if(trap)next["trap_queue"].push_back({{"delivery",id},{"item",item},{"variant",item==aptraps::Speed?static_cast<int>(std::random_device{}()&1U):0},{"status","queued"}});
 GS game=*g_pgsCur;DeriveInventory(game,next);
 if(item==10020019){if(game.ccharm<2)++game.ccharm;else game.clife=std::min(99,game.clife+1);}
 if(item==10020020)game.clife=std::min(99,game.clife+1);
 // Commit receipt and changed game state BEFORE exposing the effect or ACK.
 Persist(next,&game);*g_pgsCur=game;g_saveData.saveData[0]=game;
 Acknowledge(c,"Applied");
}
}
void Initialize(const std::filesystem::path& path) {
 if(path.empty())return;
 profilePath=std::filesystem::absolute(path);profile=Read(profilePath);
 if(profile.is_null()||profile.at("game")!="Sly Cooper and the Thievius Raccoonus")throw std::runtime_error("Invalid Sly AP profile");
 enabled=true; cane::g_userOverride=profilePath.parent_path()/"user"; std::filesystem::create_directories(cane::g_userOverride); bridgeDir=Resolve(profile.at("bridge_dir").get<std::string>());
 std::filesystem::create_directories(bridgeDir);
 roleLock=CreateFileW((bridgeDir/"game.lock").c_str(),GENERIC_READ|GENERIC_WRITE,0,nullptr,OPEN_ALWAYS,FILE_ATTRIBUTE_NORMAL,nullptr);
 if(roleLock==INVALID_HANDLE_VALUE)throw std::runtime_error("Another game owns this AP profile");
 savePath=bridgeDir/"native-save.json";
 state=Read(savePath);
 if(state.is_null())state={{"version",1},{"identity",nullptr},{"receipts",Json::object()},{"counts",Json::object()},{"checks",Json::object()},{"bosses",Json::object()},{"goal",false},{"game",nullptr}};
 if(state.at("version")!=1)throw std::runtime_error("Unsupported AP save version");
 if(!state.contains("trap_queue"))state["trap_queue"]=Json::array();
 if(!state["trap_queue"].is_array())throw std::runtime_error("Invalid native trap queue");
 aptraps::State().Configure({true,true,true,true});
 if(!state["identity"].is_null() && state["identity"].at("save")!=profile.at("save"))throw std::runtime_error("AP save token mismatch");
 if(const char* m=std::getenv("SLY_AP_MAPPINGS"))mappings=Read(std::filesystem::u8path(m));
 if(mappings.is_null())throw std::runtime_error("SLY_AP_MAPPINGS is required for AP mode");
 for(const auto& l:mappings.at("locations"))locationIds[l.at("name").get<std::string>()]=l.at("id").get<int64_t>();
 for(const auto& l:mappings.at("items"))itemNames[l.at("id").get<int64_t>()]=l.at("name").get<std::string>();
 boot=std::to_string(GetCurrentProcessId())+"-"+std::to_string(std::chrono::steady_clock::now().time_since_epoch().count());
 publisher=std::make_unique<StatusPublisher>([](const auto& path,const auto& value){Atomic(path,value,false);});
}
bool Enabled(){return enabled;}
bool Bound(){return bound;}
const std::string& Error(){return error;}
bool RemoteBottles(){return enabled&&Option("ItemCluesanityBundleSize")>0;}
bool SkipCutscenes(){return enabled&&Option("CutsceneSkip",1)!=0;}
bool FastClockwerk(){return enabled&&Option("FastClockwerk")!=0;}
bool WorldAllowed(int world){return !enabled||(world==0)||(bound&&world>=1&&world<=5&&(g_pgsCur->aws[world].fws&1));}
bool HubTravelAllowed(int world){return enabled&&bound&&world>=1&&world<=4&&Count(10020020+world)>0&&Count(10020014+world)>0;}
std::string CurrentArea(){int w=g_pgsCur?g_pgsCur->gameWorldCur:0;int l=g_pgsCur?g_pgsCur->worldLevelCur:0;if(w>=1&&w<=4&&l==8)return bosses[w-1];if(w==5)return "Cold Heart of Hate";if(w==0&&l==3)return "Paris";if(w==0&&l==4)return "Hideout";if(w==3&&l==1)return "Swamp's Dark Center";if(w>=1&&w<=4&&l>=0&&l<8)return levels[w-1][l];return g_pgsCur?PchzFriendlyFromLevelId((w<<8)|l):"";}
bool SaveSlot(int slot,const GS& game){if(!enabled)return false;if(slot!=0||!bound)return false;try{Persist(state,&game);return true;}catch(const std::exception& e){error=e.what();return false;}}
bool LoadSlot(int slot,GS* game){if(!enabled||slot!=0||!game)return false;try{if(state["game"].is_null())return false;auto data=state["game"].get<std::vector<uint8_t>>();if(data.size()!=sizeof(GS))throw std::runtime_error("AP save native layout differs");std::memcpy(game,data.data(),sizeof(GS));return true;}catch(const std::exception& e){error=e.what();return false;}}
void Tick(){
 if(!enabled)return;
 auto now=std::chrono::steady_clock::now();
 aptraps::State().Advance(std::chrono::duration<double>(now-lastTrapTick).count(),SafeTraps());lastTrapTick=now;
 if(now-lastTick<std::chrono::milliseconds(250))return;lastTick=now;
 try {
  auto h=Read(bridgeDir/"session.json");
  if(h.is_null())return;
  if(h.at("version")!=1||h.at("contract")!=profile.at("contract")||h.at("identity").at("game")!=profile.at("game")||h.at("identity").at("save")!=profile.at("save"))throw std::runtime_error("AP session identity mismatch");
  if(!state["identity"].is_null()&&state["identity"]!=h["identity"])throw std::runtime_error("AP save belongs to another seed/team/slot");
  if(state["identity"].is_null()){auto next=state;next["identity"]=h["identity"];Persist(next);}
  handshake=h;bound=true;
  auto active=Read(bridgeDir/"active-locations.json");
  if(active.is_null()||active.at("identity")!=h.at("identity")||active.at("challenge")!=h.at("challenge")){Publish(false);return;}
  allowed.clear();for(auto id:active.at("locations"))allowed.insert(id.get<int64_t>());
  bool ready=Safe();
  if(ready){ObserveChecks();DeriveInventory(*g_pgsCur);}
  Commands(ready);
  ProcessTraps(SafeTraps());
  if (ready) {
    g_fSkipBinocDialogs = SkipCutscenes();
    bool saveResources = state["game"].is_null();
    if (!saveResources) {
      auto bytes=state["game"].get<std::vector<uint8_t>>();
      if(bytes.size()!=sizeof(GS))throw std::runtime_error("AP save layout mismatch");
      GS saved; std::memcpy(&saved,bytes.data(),sizeof(GS));
      saveResources=saved.clife!=g_pgsCur->clife||saved.ccharm!=g_pgsCur->ccharm||saved.ccoin!=g_pgsCur->ccoin||saved.gameWorldCur!=g_pgsCur->gameWorldCur||saved.worldLevelCur!=g_pgsCur->worldLevelCur;
    }
    if(saveResources)Persist(state,g_pgsCur);
  }
  Publish(ready);error=publisher->Error();
 }catch(const std::exception& e){error=e.what();bound=false;}
}
#include "ap_bridge_selftest.inc"
}
