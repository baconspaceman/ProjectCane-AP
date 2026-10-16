"""Configure owned extracted data; never installs over stock ProjectCane or AP."""
import argparse
import sys
from package_common import ROOT,atomic_json,validate_data

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data',help='Existing compatible data folder containing Worlds and Cutscenes')
    parser.add_argument('--check-only',action='store_true',help='Validate without saving configuration')
    args=parser.parse_args()
    directory=validate_data(args.data or input('Extracted ProjectCane 0.4.1 data folder: ').strip().strip('"'))
    if not args.check_only:
        try:stored=str(directory.relative_to(ROOT))
        except ValueError:stored=str(directory)
        atomic_json(ROOT/'local-settings.json',{'version':1,'data_directory':stored})
    print('Compatible PCBR17 level data, PCM dialogue and cutscene files found.')
    print('Existing game data is reused read-only. AP saves remain inside this package\'s profiles folder.')
    if not (ROOT/'runtime/bin/ProjectCane.exe').is_file():
        print('Native AP game has not been built. Run scripts/Build-Native.ps1 as described in README before Play.cmd.')
    else:print(('Validation passed; configuration unchanged.' if args.check_only else 'Setup saved.')+' Run Play.cmd to create or resume an AP connection profile.')

if __name__=='__main__':
    try:main()
    except (OSError,ValueError,KeyError) as error:print(f'Setup: {error}',file=sys.stderr);sys.exit(1)
