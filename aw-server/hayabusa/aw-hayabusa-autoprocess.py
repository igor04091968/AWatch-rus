#!/usr/bin/env python3
import argparse
import fcntl
import json
import pathlib
import shutil
import subprocess
import sys
from typing import Optional

DROP_DIR = pathlib.Path('/opt/activitywatch/aw-rus-ops/drop')
LOCK_PATH = pathlib.Path('/opt/hayabusa/state/aw-hayabusa-autoprocess.lock')
WRAPPER = pathlib.Path('/usr/local/bin/aw-hayabusa')
LINKER = pathlib.Path('/usr/local/bin/aw-hayabusa-link-case')


def run(cmd):
    print('RUN', ' '.join(str(x) for x in cmd), flush=True)
    subprocess.run(cmd, check=True)


def read_latest_intake():
    return json.loads(pathlib.Path('/opt/hayabusa/state/latest-intake.json').read_text(encoding='utf-8'))


def load_sidecars(zip_path: pathlib.Path):
    base = zip_path.with_suffix('')
    caseid_path = base.with_suffix('.caseid')
    meta_path = base.with_suffix('.meta.json')
    meta = {}
    if meta_path.is_file():
        meta = json.loads(meta_path.read_text(encoding='utf-8'))
    case_id = meta.get('case_id')
    if case_id is None and caseid_path.is_file():
        raw = caseid_path.read_text(encoding='utf-8').strip()
        if raw:
            case_id = int(raw)
    return {
        'base': base,
        'case_id': case_id,
        'host': meta.get('host'),
        'mode': meta.get('mode', 'incident'),
        'link_source': meta.get('link_source', 'aw-rus-drop-autoprocess'),
        'caseid_path': caseid_path,
        'meta_path': meta_path,
    }


def archive_sidecars(report_dir: pathlib.Path, sidecars):
    target_dir = report_dir / 'input-sidecars'
    target_dir.mkdir(parents=True, exist_ok=True)
    for path in (sidecars['caseid_path'], sidecars['meta_path']):
        if path.is_file():
            shutil.move(str(path), str(target_dir / path.name))


def archive_drop_package(report_dir: pathlib.Path, zip_path: pathlib.Path):
    target_dir = report_dir / 'input-drop'
    target_dir.mkdir(parents=True, exist_ok=True)
    target_path = target_dir / zip_path.name
    if target_path.exists():
        target_path.unlink()
    shutil.move(str(zip_path), str(target_path))


def guess_host(zip_path: pathlib.Path, sidecars) -> Optional[str]:
    if sidecars['host']:
        return str(sidecars['host'])
    name = zip_path.stem
    if '-' in name:
        return name.split('-', 1)[0]
    return name or None


def process_one(zip_path: pathlib.Path):
    sidecars = load_sidecars(zip_path)
    host = guess_host(zip_path, sidecars)
    mode = sidecars['mode'] or 'incident'
    accept_cmd = [str(WRAPPER), 'accept', '--package', str(zip_path)]
    if host:
        accept_cmd += ['--host', host]
    run(accept_cmd)
    run([str(WRAPPER), 'process-inbox', '--mode', mode, '--limit', '1'])
    latest = read_latest_intake()
    report_dir = pathlib.Path(latest['report_dir'])
    archive_sidecars(report_dir, sidecars)
    archive_drop_package(report_dir, zip_path)
    if sidecars['case_id'] is not None:
        run([str(LINKER), '--case-id', str(sidecars['case_id']), '--mode', mode, '--link-source', sidecars['link_source']])
    return latest


def main():
    p = argparse.ArgumentParser(description='Auto-process Hayabusa zip packages dropped onto aw-rus server')
    p.add_argument('--drop-dir', default=str(DROP_DIR))
    p.add_argument('--once', action='store_true', default=True)
    args = p.parse_args()

    drop_dir = pathlib.Path(args.drop_dir)
    drop_dir.mkdir(parents=True, exist_ok=True)
    LOCK_PATH.parent.mkdir(parents=True, exist_ok=True)
    with LOCK_PATH.open('w') as lock_fh:
        try:
            fcntl.flock(lock_fh, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            print('autoprocess already running', file=sys.stderr)
            return 0
        zips = sorted(drop_dir.glob('*.zip'))
        if not zips:
            print('no zip packages in drop dir')
            return 0
        for zip_path in zips:
            latest = process_one(zip_path)
            print(json.dumps({'processed': str(zip_path), 'latest_intake': latest}, ensure_ascii=False, indent=2))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
