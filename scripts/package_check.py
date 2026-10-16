"""Verify a clean relocated player runtime without launching a game or AP server."""
import importlib.util
import json
import sys
from package_common import ROOT,read_json

def main():
    import websockets,yaml,schema,jellyfish,colorama,platformdirs,orjson,bsdiff4
    spec=importlib.util.spec_from_file_location('sly_logic_check',ROOT/'logic/worker.py')
    worker=importlib.util.module_from_spec(spec);spec.loader.exec_module(worker)
    assert worker.GAME=='Sly Cooper and the Thievius Raccoonus'
    assert read_json(ROOT/'world/mappings.json')['game']==worker.GAME
    if not (ROOT/'runtime/bin/sly-ap-companion.exe').is_file():raise RuntimeError('Missing companion')
    roots=[sys.prefix,sys.base_prefix,sys.executable]
    assert all(str(ROOT).lower() in value.lower() for value in roots),roots
    print(json.dumps({'passed':True,'python':sys.version.split()[0],'root':str(ROOT),
        'python_roots':roots,'native_built':(ROOT/'runtime/bin/ProjectCane.exe').is_file(),
        'scope':'relocated portable Python/import/path check; no game/server/network launch'},indent=2))

if __name__=='__main__':main()
