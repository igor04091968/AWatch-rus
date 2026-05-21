#!/usr/bin/env python3
import argparse
import json
import pathlib
import re
import subprocess
import sys

OPS_ROOT = pathlib.Path('/opt/activitywatch/aw-rus-ops')
DEFAULT_INVENTORY = OPS_ROOT / 'ansible' / 'inventory.ini'
DEFAULT_DROP = OPS_ROOT / 'drop'
DEFAULT_ANSIBLE = OPS_ROOT / 'venv' / 'bin' / 'ansible'
DEFAULT_WRAPPER = pathlib.Path('/usr/local/bin/aw-hayabusa')
DEFAULT_LINKER = pathlib.Path('/usr/local/bin/aw-hayabusa-link-case')
WINDOWS_EXPORT_CMD = r"powershell.exe -ExecutionPolicy Bypass -File C:\ProgramData\AWatch-rus\export-evtx-for-hayabusa.ps1 -DaysBack {days_back} | ConvertTo-Json -Depth 8 -Compress"
WINDOWS_LATEST_ZIP_CMD = r"Get-ChildItem 'C:\ProgramData\AWatch-rus\forensics\evtx-exports' -File -Filter '*.zip' | Sort-Object LastWriteTime -Descending | Select-Object -First 1 FullName,Length,LastWriteTime | ConvertTo-Json -Compress"


def run(cmd):
    proc = subprocess.run(cmd, text=True, capture_output=True)
    if proc.returncode != 0:
        sys.stderr.write(proc.stdout)
        sys.stderr.write(proc.stderr)
        raise SystemExit(proc.returncode)
    return proc.stdout


def extract_json_blob(text):
    matches = re.findall(r'(\{.*\}|\[.*\])', text, re.S)
    for candidate in reversed(matches):
        try:
            return json.loads(candidate)
        except Exception:
            continue
    raise SystemExit('cannot parse JSON from ansible output:\n' + text)


def main():
    p = argparse.ArgumentParser(description='Run Windows EVTX export and Hayabusa intake directly from aw-server, without the laptop')
    p.add_argument('--inventory', default=str(DEFAULT_INVENTORY))
    p.add_argument('--ansible-bin', default=str(DEFAULT_ANSIBLE))
    p.add_argument('--drop-dir', default=str(DEFAULT_DROP))
    p.add_argument('--days-back', type=int, default=1)
    p.add_argument('--mode', default='incident', choices=['quick', 'incident', 'full'])
    p.add_argument('--case-id', type=int)
    p.add_argument('--link-source', default='aw-rus-ops-from-windows')
    p.add_argument('--windows-group', default='aw_windows')
    p.add_argument('--wrapper', default=str(DEFAULT_WRAPPER))
    p.add_argument('--linker', default=str(DEFAULT_LINKER))
    args = p.parse_args()

    inventory = pathlib.Path(args.inventory)
    ansible_bin = pathlib.Path(args.ansible_bin)
    drop_dir = pathlib.Path(args.drop_dir)
    wrapper = pathlib.Path(args.wrapper)
    linker = pathlib.Path(args.linker)
    if not inventory.is_file():
        raise SystemExit(f'inventory not found: {inventory}')
    if not ansible_bin.is_file():
        raise SystemExit(f'ansible binary not found: {ansible_bin}')
    if not wrapper.is_file():
        raise SystemExit(f'wrapper not found: {wrapper}')
    drop_dir.mkdir(parents=True, exist_ok=True)

    export_cmd = [str(ansible_bin), args.windows_group, '-i', str(inventory), '-m', 'win_shell', '-a', WINDOWS_EXPORT_CMD.format(days_back=args.days_back)]
    print('RUN_EXPORT', ' '.join(export_cmd))
    export_out = run(export_cmd)
    export_json = extract_json_blob(export_out)

    list_cmd = [str(ansible_bin), args.windows_group, '-i', str(inventory), '-m', 'win_shell', '-a', WINDOWS_LATEST_ZIP_CMD]
    print('RUN_LIST', ' '.join(list_cmd))
    latest_out = run(list_cmd)
    latest = extract_json_blob(latest_out)
    if isinstance(latest, list):
        latest = latest[0]
    remote_zip = latest['FullName']
    filename = pathlib.PureWindowsPath(remote_zip).name
    local_zip = drop_dir / filename

    remote_zip_posix = remote_zip.replace('\\', '/')
    fetch_cmd = [str(ansible_bin), args.windows_group, '-i', str(inventory), '-m', 'fetch', '-a', f'src={remote_zip_posix} dest={drop_dir}/ flat=yes']
    print('RUN_FETCH', ' '.join(fetch_cmd))
    run(fetch_cmd)
    if not local_zip.is_file():
        raise SystemExit(f'fetched zip not found: {local_zip}')

    accept_cmd = [str(wrapper), 'accept', '--package', str(local_zip)]
    host = export_json.get('hostname') or pathlib.Path(filename).stem.split('-')[0]
    if host:
        accept_cmd += ['--host', str(host)]
    print('RUN_ACCEPT', ' '.join(accept_cmd))
    subprocess.run(accept_cmd, check=True)

    process_cmd = [str(wrapper), 'process-inbox', '--mode', args.mode, '--limit', '1']
    print('RUN_PROCESS', ' '.join(process_cmd))
    subprocess.run(process_cmd, check=True)

    if args.case_id is not None:
        if not linker.is_file():
            raise SystemExit(f'linker not found: {linker}')
        link_cmd = [str(linker), '--case-id', str(args.case_id), '--mode', args.mode, '--link-source', args.link_source]
        print('RUN_LINK', ' '.join(link_cmd))
        subprocess.run(link_cmd, check=True)

    latest_intake = pathlib.Path('/opt/hayabusa/state/latest-intake.json')
    print('LATEST_INTAKE')
    print(latest_intake.read_text(encoding='utf-8'))


if __name__ == '__main__':
    main()
