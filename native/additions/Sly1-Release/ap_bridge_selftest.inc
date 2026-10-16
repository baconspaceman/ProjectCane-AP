// Included inside namespace slyap in ap_bridge.cpp. This deliberately exercises
// actual bridge transaction methods with synthetic GS, without game/GL startup.
int RunSelfTest(const std::filesystem::path& root) {
 Json results=Json::array();
 GS* originalGs=g_pgsCur;
 try {
  if(std::filesystem::exists(root))throw std::runtime_error("Self-test destination already exists");
  std::filesystem::create_directories(root);
  Json testProfile={{"game","Sly Cooper and the Thievius Raccoonus"},{"save","native-selftest"},
   {"contract","sly1-native-v1"},{"bridge_dir","bridge"}};
  Atomic(root/"profile.json",testProfile);
  Initialize(root/"profile.json");
  Json identity={{"game",testProfile["game"]},{"seed","native-selftest"},{"team",0},{"slot",1},{"save","native-selftest"}};
  handshake={{"version",1},{"contract","sly1-native-v1"},{"challenge","selftest-challenge"},{"identity",identity},
   {"slot_data",{{"UnlockClockwerk",1},{"RequiredBosses",1},{"ItemCluesanityBundleSize",1},{"RequiredPages",1}}}};
  bound=true;
  state["identity"]=identity;
  g_saveData.saveData[0]=GS{};
  g_pgsCur=&g_saveData.saveData[0];
  Persist(state,g_pgsCur);
  int attempt=0;
  auto check=[&](const char* name,bool pass){
   results.push_back({{"name",name},{"passed",pass}});
   if(!pass)throw std::runtime_error(std::string("failed: ")+name);
  };
  auto command=[&](const std::string& key,int64_t item){
   return Json{{"version",1},{"challenge",handshake["challenge"]},{"boot",boot},{"attempt",std::to_string(++attempt)},
    {"identity",identity},{"effect",{{"id",key},{"kind","Repeatable"},{"payload",{{"item_id",item}}}}}};
  };
  auto send=[&](const Json& c,bool ready=true){Atomic(bridgeDir/"command.json",c);Commands(ready);return Read(bridgeDir/"ack.json");};
  auto c=command("received:00000000000000000000",10020015);
  auto a=send(c);
  check("key_applies",a["outcome"]=="Applied"&&g_pgsCur->aws[1].ckey==1&&Count(10020015)==1);
  c["attempt"]=std::to_string(++attempt);a=send(c);
  check("duplicate_receipt_no_extra",a["outcome"]=="AlreadySatisfied"&&g_pgsCur->aws[1].ckey==1&&Count(10020015)==1);
  c=command("received:00000000000000000001",10020015);a=send(c);
  check("second_index_is_second_key",a["outcome"]=="Applied"&&g_pgsCur->aws[1].ckey==2&&Count(10020015)==2);
  c=command("received:00000000000000000001",10020020);a=send(c);
  check("changed_payload_rejected",a["outcome"].is_object()&&a["outcome"].contains("Rejected")&&Count(10020020)==0);
  c=command("received:00000000000000000002",10020020);a=send(c,false);
  check("notready_has_no_effect",a["outcome"].is_object()&&a["outcome"].contains("NotReady")&&Count(10020020)==0);
  Commands(true);
  check("notready_attempt_stays_inert",Count(10020020)==0&&!state["receipts"].contains("received:00000000000000000002"));
  c["attempt"]=std::to_string(++attempt);a=send(c);
  check("new_attempt_after_notready_applies",a["outcome"]=="Applied"&&Count(10020020)==1);
  c=command("received:00000000000000000003",10020020);c["identity"]["seed"]="foreign";send(c);
  check("foreign_identity_no_effect",Count(10020020)==1);
  c=command("received:00000000000000000003",10020020);c["challenge"]="old";send(c);
  check("foreign_challenge_no_effect",Count(10020020)==1);
  c=command("received:00000000000000000003",10020020);c["boot"]="old";send(c);
  check("foreign_boot_no_effect",Count(10020020)==1);
  c=command("received:00000000000000000003",10020019);a=send(c);
  check("charm_applies",a["outcome"]=="Applied"&&g_pgsCur->ccharm==1);
  g_pgsCur->ccharm=0;
  check("spent_charm_saved",SaveSlot(0,*g_pgsCur));
  state=Read(savePath);GS restored{};
  check("native_save_load",LoadSlot(0,&restored)&&restored.ccharm==0&&restored.aws[1].ckey==2);
  *g_pgsCur=restored;c["attempt"]=std::to_string(++attempt);a=send(c);
  check("spent_charm_replay_not_refilled",a["outcome"]=="AlreadySatisfied"&&g_pgsCur->ccharm==0);
  c=command("received:00000000000000000004",10020025);a=send(c);
  check("victory_receipt_does_not_forge_goal",a["outcome"]=="Applied"&&!state.value("goal",false));
  // A failed filesystem commit cannot modify native inventory or in-memory receipts.
  auto originalSavePath=savePath;savePath=root/"bad-target";std::filesystem::create_directory(savePath);
  bool threw=false;c=command("received:00000000000000000005",10020015);
  try {send(c);}catch(const std::exception&){threw=true;}
  savePath=originalSavePath;
  check("failed_commit_has_no_effect",threw&&Count(10020015)==2&&g_pgsCur->aws[1].ckey==2&&!state["receipts"].contains("received:00000000000000000005"));
  // Regression for the first actual GUI launch: CRT ifstream readers prevented
  // Windows atomic replacement and caused the companion publisher to exit.
  const auto sharedDocument=root/"shared-reader.json";
  Atomic(sharedDocument,{{"index",0},{"data",std::string(200000,'a')}});
  std::atomic<bool> finishReader{false},readerFailed{false};
  std::thread reader([&]{while(!finishReader.load()) {
   try {const auto document=Read(sharedDocument);if(!document.contains("index"))readerFailed=true;}
   catch(...){readerFailed=true;}
  }});
  bool writerFailed=false;
  try {for(int i=1;i<=100;++i)Atomic(sharedDocument,{{"index",i},{"data",std::string(200000,'b')}});}
  catch(...){writerFailed=true;}
  finishReader=true;reader.join();
  check("shared_reader_allows_atomic_replacement",!readerFailed&&!writerFailed&&Read(sharedDocument)["index"]==100);
  for(int64_t trap=aptraps::Ice;trap<=aptraps::Invisibility;++trap) {
   const auto delivery="trap:"+std::to_string(trap);
   c=command(delivery,trap);a=send(c);
   check(("trap_queued_"+std::to_string(trap)).c_str(),a["outcome"]=="Applied"&&state["trap_queue"].back()["status"]=="queued");
   ProcessTraps(true);
   check(("trap_started_"+std::to_string(trap)).c_str(),state["trap_queue"].back()["status"]=="started"&&aptraps::State().Active());
   aptraps::State().Advance(11,true);ProcessTraps(true);
   check(("trap_completed_"+std::to_string(trap)).c_str(),state["trap_queue"].back()["status"]=="finished"&&!aptraps::State().Active());
   c["attempt"]=std::to_string(++attempt);a=send(c);
   check(("trap_duplicate_"+std::to_string(trap)).c_str(),a["outcome"]=="AlreadySatisfied"&&!aptraps::State().Active());
  }
  auto interrupted=state;interrupted["trap_queue"].push_back({{"delivery","old-boot-trap"},{"item",aptraps::Ball},{"variant",0},{"status","started"},{"boot","old-boot"}});
  Persist(interrupted);ProcessTraps(false);
  check("old_boot_trap_not_replayed",state["trap_queue"].back()["status"]=="interrupted"&&!aptraps::State().Active());
  const auto preservedState=state;
  const GS preservedGame=*g_pgsCur;
  for(int world=1;world<=4;++world) {
   state["counts"]=Json::object();*g_pgsCur=GS{};
   WM map{};map.gameWorldCur=static_cast<GAMEWORLD>(world);
   GAMEWORLD resolvedWorld{};WORLDLEVEL resolvedLevel{};
   state["counts"][std::to_string(10020014+world)]=1;
   check(("hub_key_without_episode_denied_"+std::to_string(world)).c_str(),!HubTravelAllowed(world));
   state["counts"]=Json::object();state["counts"][std::to_string(10020020+world)]=1;DeriveInventory(*g_pgsCur);
   check(("hub_episode_without_key_denied_"+std::to_string(world)).c_str(),!FResolveWmLevel(&map,WORLDLEVEL_Hub,nullptr,nullptr));
   state["counts"][std::to_string(10020014+world)]=1;DeriveInventory(*g_pgsCur);
   const auto prior=Bytes(*g_pgsCur);
   check(("hub_first_key_enables_unvisited_warp_"+std::to_string(world)).c_str(),FResolveWmLevel(&map,WORLDLEVEL_Hub,&resolvedWorld,&resolvedLevel)&&resolvedWorld==world&&resolvedLevel==WORLDLEVEL_Hub);
   check(("hub_keeps_approach_without_forged_checks_"+std::to_string(world)).c_str(),FResolveWmLevel(&map,WORLDLEVEL_Approach,nullptr,nullptr)&&Bytes(*g_pgsCur)==prior&&g_pgsCur->aws[world].als[1].grfls==0);
  }
  check("hub_shortcut_excludes_intro_and_finale",!HubTravelAllowed(0)&&!HubTravelAllowed(5));
  state=preservedState;*g_pgsCur=preservedGame;
  std::promise<void> started,release;auto released=release.get_future().share();
  std::vector<int> publications;
  {
   StatusPublisher slowWriter([&](const auto&,const Json& value){
    if(publications.empty()){started.set_value();released.wait();}
    publications.push_back(value.at("sequence").get<int>());
   });
   auto batch=[](int sequence){return StatusPublisher::Batch{{{"snapshot",{{"sequence",sequence}}},{"facts",{{"sequence",sequence}}}}};};
   slowWriter.Submit(batch(0));
   const bool workerStarted=started.get_future().wait_for(std::chrono::seconds(5))==std::future_status::ready;
   for(int sequence=1;sequence<=100;++sequence)slowWriter.Submit(batch(sequence));
   release.set_value();slowWriter.Drain();
   check("status_slow_io_does_not_block_and_coalesces",workerStarted&&publications==std::vector<int>({0,0,100,100}));
  }
  Atomic(root/"native-selftest.json",{{"passed",true},{"cases",results},{"scope","compiled native bridge methods with synthetic GS; not gameplay"}});
  g_pgsCur=originalGs;
  return 0;
 }catch(const std::exception& e){
  try {Atomic(root/"native-selftest.json",{{"passed",false},{"cases",results},{"error",e.what()},{"scope","compiled native bridge methods with synthetic GS; not gameplay"}});}catch(...){}
  g_pgsCur=originalGs;
  return 1;
 }
}
