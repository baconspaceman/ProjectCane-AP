"""Portable local paths and bounded profile I/O. No model/MCP dependency."""
from pathlib import Path
import json
import os
import re
import struct
import time

ROOT=Path(__file__).resolve().parents[1]
GAME='Sly Cooper and the Thievius Raccoonus'
CONTRACT='sly1-native-v1'

def atomic_json(path,value):
    path=Path(path)
    path.parent.mkdir(parents=True,exist_ok=True)
    payload=json.dumps(value,ensure_ascii=False,indent=2).encode('utf-8')
    temp=path.with_suffix(path.suffix+'.tmp')
    with temp.open('wb') as stream:
        stream.write(payload);stream.flush();os.fsync(stream.fileno())
    for attempt in range(11):
        try:os.replace(temp,path);return
        except OSError as error:
            if attempt==10 or getattr(error,'winerror',None) not in (5,32,33,1450):raise
            time.sleep(.02)

def read_json(path):
    path=Path(path)
    with path.open('rb') as stream:
        raw=stream.read(1048577)
    if len(raw)>1048576:raise ValueError('JSON file exceeds limit')
    return json.loads(raw)

def profile_directory(name):
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]{0,63}',name):
        raise ValueError('Profile names use 1-64 letters, numbers, dashes or underscores.')
    if name.upper().split('.')[0] in {'CON','PRN','AUX','NUL',*[f'COM{i}' for i in range(1,10)],*[f'LPT{i}' for i in range(1,10)]}:
        raise ValueError('That profile name is reserved by Windows.')
    return ROOT/'profiles'/name

def validate_data(directory):
    directory=Path(directory).expanduser().resolve()
    worlds=directory/'Worlds'
    levels=list(worlds.rglob('*.brx')) if worlds.is_dir() else []
    if len(levels)<46:raise ValueError('Choose the extracted data folder containing Worlds and all 46 PCBR levels. Use the official ProjectCane 0.4.1 extractor first.')
    for path in levels:
        with path.open('rb') as stream:header=stream.read(8)
        if len(header)!=8 or header[:4]!=b'PCBR' or struct.unpack('<I',header[4:])[0]!=17:
            raise ValueError(f'Incompatible level format: {path.name}. Re-extract using the matching ProjectCane 0.4.1 extractor.')
    required=worlds/'Paris'/'Dialog'/'VAG_006B_0FF06C.wav'
    if not required.is_file():raise ValueError('Current PCM dialogue is missing. Old VAG data is incompatible; re-extract with the official 0.4.1 extractor.')
    with required.open('rb') as stream:header=stream.read(44)
    if len(header)<44 or header[:4]!=b'RIFF' or header[8:12]!=b'WAVE' or struct.unpack('<H',header[20:22])[0]!=1:
        raise ValueError('Required dialogue is not a valid PCM WAV.')
    for index in range(18):
        for extension in ('m2v','wav'):
            if not (directory/'Cutscenes'/f'Cutscene_{index:02d}.{extension}').is_file():
                raise ValueError(f'Missing extracted cutscene {index:02d}.{extension}.')
    return directory

def configured_data():
    if not (ROOT/'local-settings.json').is_file():
        raise ValueError('Game data has not been configured. Run Setup.cmd --data "your extracted data folder" first.')
    config=read_json(ROOT/'local-settings.json')
    directory=Path(config['data_directory'])
    if not directory.is_absolute():directory=ROOT/directory
    return validate_data(directory)
