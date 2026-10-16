"""Run the compiled native bridge self-test, without window/audio/game assets.

Synthetic GS and actual bridge methods only. This is not gameplay acceptance.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess


def main(args):
    project=Path(__file__).resolve().parents[1]
    assert not args.output.exists(), 'Use a fresh self-test directory'
    env=dict(os.environ,SLY_AP_MAPPINGS=str(project/'world/mappings.json'))
    env.pop('SLY_AP_PROFILE',None)
    result=subprocess.run([str(args.exe),'--ap-self-test',str(args.output)],env=env,
        capture_output=True,text=True,timeout=40,
        creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
    args.output.mkdir(parents=True,exist_ok=True)
    (args.output/'process.stdout.log').write_text(result.stdout)
    (args.output/'process.stderr.log').write_text(result.stderr)
    payload=json.loads((args.output/'native-selftest.json').read_text())
    assert result.returncode==0 and payload['passed'] is True,payload
    names={case['name'] for case in payload['cases'] if case['passed']}
    required={'key_applies','duplicate_receipt_no_extra','second_index_is_second_key',
        'changed_payload_rejected','notready_has_no_effect','notready_attempt_stays_inert',
        'new_attempt_after_notready_applies','foreign_identity_no_effect','foreign_challenge_no_effect',
        'foreign_boot_no_effect','charm_applies','spent_charm_saved','native_save_load',
        'spent_charm_replay_not_refilled','victory_receipt_does_not_forge_goal','failed_commit_has_no_effect',
        'shared_reader_allows_atomic_replacement'}
    assert required<=names,required-names
    print(f'{len(names)} compiled native-method assertions passed (synthetic GS; not gameplay)')


if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--exe',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    main(parser.parse_args())
