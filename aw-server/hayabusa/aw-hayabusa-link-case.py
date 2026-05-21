#!/usr/bin/env python3
import argparse
import json
import pathlib
import sys
import urllib.error
import urllib.request


def build_payload(intake, mode, link_source):
    report_dir = pathlib.Path(intake['report_dir'])
    return {
        'tool': 'hayabusa',
        'host': intake['host'],
        'mode': mode,
        'status': intake['status'],
        'intake_id': intake['intake_id'],
        'package_path': intake['package_path'],
        'sha256': intake['sha256'],
        'report_dir': intake['report_dir'],
        'summary_html': str(report_dir / 'summary.html'),
        'timeline_path': str(report_dir / 'timeline.jsonl'),
        'manifest_path': str(report_dir / 'manifest.json'),
        'link_source': link_source,
    }


def post_json(url, payload):
    data = json.dumps(payload, ensure_ascii=False).encode('utf-8')
    req = urllib.request.Request(url, data=data, method='POST', headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read().decode('utf-8'))


def get_json(url):
    with urllib.request.urlopen(url) as resp:
        return json.loads(resp.read().decode('utf-8'))


def main():
    p = argparse.ArgumentParser(description='Link latest or specified Hayabusa intake metadata to AW-rus case management')
    p.add_argument('--case-id', type=int, required=True)
    p.add_argument('--intake-json', default='/opt/hayabusa/state/latest-intake.json')
    p.add_argument('--case-api-base', default='http://127.0.0.1:5602')
    p.add_argument('--mode', default='incident')
    p.add_argument('--link-source', default='aw-rus-ops')
    args = p.parse_args()

    intake_path = pathlib.Path(args.intake_json)
    if not intake_path.is_file():
        raise SystemExit(f'intake json not found: {intake_path}')
    intake = json.loads(intake_path.read_text(encoding='utf-8'))
    payload = build_payload(intake, args.mode, args.link_source)
    case_url = f"{args.case_api_base.rstrip('/')}/api/0/dlp/cases/{args.case_id}/forensics/hayabusa"
    try:
        post_json(case_url, payload)
    except urllib.error.HTTPError as exc:
        body = exc.read().decode('utf-8', errors='replace')
        raise SystemExit(f'case API POST failed: HTTP {exc.code}: {body}')
    case = get_json(f"{args.case_api_base.rstrip('/')}/api/0/dlp/cases/{args.case_id}")
    print(json.dumps({'case_id': args.case_id, 'intake': intake, 'forensics': case.get('forensics')}, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
