"""Launch a separate AP run against an existing room. No local server is started."""
import argparse
from datetime import datetime,timezone
import json
import msvcrt
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid
from package_common import ROOT,GAME,CONTRACT,atomic_json,read_json,profile_directory,configured_data

def endpoint(value):
    value=value.strip()
    if not value.startswith(('ws://','wss://')):value='ws://'+value
    if len(value)>512 or any(c.isspace() for c in value) or any(c in value for c in '@?#'):
        raise ValueError('Use a server hostname and port; enter passwords only in the in-game menu.')
    return value

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile',default=None,help='New or existing run name; letters/numbers/dashes/underscores')
    parser.add_argument('--server',help='AP server for a NEW profile, e.g. archipelago.gg:12345')
    parser.add_argument('--slot',help='Exact AP slot name for a NEW profile')
    parser.add_argument('--prepare-only',action='store_true',help='Create/check profile only; do not launch anything')
    args=parser.parse_args()
    name=args.profile or input('AP run profile name [default]: ').strip() or 'default'
    root=profile_directory(name)
    path=root/'profile.json'
    if path.exists():
        profile=read_json(path)
        if profile['game']!=GAME or profile['contract']!=CONTRACT:raise ValueError('Profile game contract differs.')
        if args.server and endpoint(args.server)!=profile['endpoint'] or args.slot and args.slot!=profile['slot_name']:
            raise ValueError('This existing run belongs to another server/slot. Use a new profile name instead of overwriting it.')
    else:
        server=endpoint(args.server or input('AP server (hostname:port): '))
        slot=args.slot or input('Exact AP slot name: ').strip()
        if not slot or len(slot)>128:raise ValueError('A slot name of 1-128 characters is required.')
        catalogue=read_json(ROOT/'world/mappings.json')
        profile={'bridge_dir':'bridge','journal_path':'host-journal.json','endpoint':server,'slot_name':slot,
            'game':GAME,'save':uuid.uuid4().hex,'contract':CONTRACT,
            'items':{str(item['id']):{'kind':'Repeatable','payload':{'item_id':item['id']}} for item in catalogue['items']},
            'locations':[location['id'] for location in catalogue['locations']], 'ack_timeout_ms':2000,'stale_after_ms':1500}
        root.mkdir(parents=True,exist_ok=False)
        atomic_json(path,profile)
    if args.prepare_only:
        print(f'Profile ready: {path}. No server, companion or game started.')
        return
    data=configured_data()
    game=ROOT/'runtime/bin/ProjectCane.exe'
    client=ROOT/'runtime/bin/sly-ap-companion.exe'
    if not game.is_file():raise ValueError('Build the native AP game with scripts/Build-Native.ps1 first; this source-assisted package does not contain an upstream game binary.')
    if not client.is_file():raise ValueError('Companion missing; re-extract the player package.')
    bridge=root/'bridge';bridge.mkdir(exist_ok=True)
    logdir=root/'logs'/datetime.now().strftime('%Y%m%d-%H%M%S');logdir.mkdir(parents=True)
    owned=[];logs=[]
    lock=(root/'launcher.lock').open('a+b')
    try:
        if lock.tell()==0:lock.write(b'\0');lock.flush()
        lock.seek(0);msvcrt.locking(lock.fileno(),msvcrt.LK_NBLCK,1)
        atomic_json(bridge/'launcher-status.json',{'version':1,'save':profile['save'],'fatal':False,'helper':'','message':''})
        env=dict(os.environ,SKIP_REQUIREMENTS_UPDATE='1',PYTHONDONTWRITEBYTECODE='1',SLY_AP_PROFILE=str(path),SLY_AP_MAPPINGS=str(ROOT/'world/mappings.json'),SLY_AP_MAP_PACK=str(ROOT/'tracker/reference'))
        # The portable Python distribution includes the matching MSVC runtime
        # DLLs also needed by the Rust companion; make them visible to child DLL loading.
        env['PATH']=str(ROOT/'runtime/python')+os.pathsep+env.get('PATH','')
        # Do not inherit an unrelated room's password or Python search path.
        for key in ('AP_PASSWORD','PYTHONHOME','PYTHONPATH'):env.pop(key,None)
        def launch(role,command):
            out=(logdir/(role+'.stdout.log')).open('wb');err=(logdir/(role+'.stderr.log')).open('wb');logs.extend([out,err])
            process=subprocess.Popen(command,cwd=root,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
            owned.append((role,process));return process
        companion=launch('companion',[str(client),str(path)])
        logic=launch('logic',[sys.executable,'-B',str(ROOT/'logic/worker.py'),'--bridge-dir',str(bridge)])
        time.sleep(.3)
        if companion.poll() is not None or logic.poll() is not None:raise RuntimeError(f'AP helper startup failed; see {logdir}')
        native=launch('game',[str(game),'--data-dir',str(data)])
        print('Sly AP started. Connect in-game. Close the game normally to stop these helpers.')
        print(f'Your isolated AP run is stored in {root}')
        announced=False
        while native.poll() is None:
            if not announced:
                dead=next(((role,proc) for role,proc in owned if role!='game' and proc.poll() is not None),None)
                if dead:
                    announced=True
                    message=f'{dead[0]} stopped unexpectedly. Gameplay remains open; AP services are degraded. Close normally when ready and inspect {logdir}.'
                    print(message,file=sys.stderr)
                    atomic_json(bridge/'launcher-status.json',{'version':1,'save':profile['save'],'fatal':True,'helper':dead[0],'message':message})
            time.sleep(.25)
        if native.returncode:raise RuntimeError(f'The game exited with code {native.returncode}; see {logdir}')
    finally:
        for _,process in reversed(owned):
            if process.poll() is None:process.terminate()
        for _,process in owned:
            try:process.wait(timeout=5)
            except subprocess.TimeoutExpired:process.kill();process.wait()
        for stream in logs:stream.close()
        lock.close()

if __name__=='__main__':
    try:main()
    except (OSError,ValueError,KeyError,RuntimeError) as error:print(f'Launch: {error}',file=sys.stderr);sys.exit(1)
