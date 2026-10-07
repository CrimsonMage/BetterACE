#!/usr/bin/env python3
"""Inventory the pinned official network source; no payload parity is implied.

Local source bytes are checked against official Git blob IDs, including the full
Network subtree and external cryptography/DDD ownership boundaries. --check
compares immutable source facts while preserving reviewed implementation status.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import tomllib
import urllib.request

ROOT = Path(__file__).resolve().parents[3]
BASELINE = tomllib.loads((ROOT / 'docs/baselines.toml').read_text())['ace']
PIN = BASELINE['commit']
NETWORK = 'Source/ACE.Server/Network/'
EXTRA = ['Source/ACE.Server/Managers/DDDManager.cs',
         'Source/ACE.Common/Cryptography/Hash32.cs',
         'Source/ACE.Common/Cryptography/ISAAC.cs',
         'Source/ACE.Common/Cryptography/CryptoSystem.cs',
         'Source/ACE.Common/DerethDateTime.cs', 'Source/ACE.Server/Entity/Timers.cs',
         'Source/ACE.Server/WorldObjects/Player_Xp.cs',
         'Source/ACE.Server/WorldObjects/Player_Attributes.cs',
         'Source/ACE.Server/WorldObjects/Player_Vitals.cs',
         'Source/ACE.Server/WorldObjects/Player_Skills.cs']


def owner(path):
    if '/ACE.DatLoader/' in path:
        return 'bace-dat'
    if Path(path).name in ('DerethDateTime.cs', 'Timers.cs'):
        return 'bace-runtime'
    if Path(path).name in ('Player_Xp.cs', 'Player_Attributes.cs', 'Player_Vitals.cs', 'Player_Skills.cs'):
        return 'bace-character'
    if '/Network/Sequence/' in path:
        return 'bace-replication'
    if path.endswith(('DDDManager.cs', 'DDDHandler.cs')):
        return 'bace-dat-service'
    if path.endswith('AuthenticationHandler.cs'):
        return 'bace-auth'
    if any(x in path for x in ('/Handlers/', '/Managers/', '/GameAction/Actions/')) or path.endswith(('Session.cs', 'SessionTerminationDetails.cs')) and not path.endswith('NetworkSession.cs'):
        return 'bace-session'
    if Path(path).name in ('NetworkSession.cs', 'NetworkBundle.cs', 'ConnectionListener.cs', 'MessageBuffer.cs', 'NetworkStatistics.cs'):
        return 'bace-transport'
    return 'bace-wire'


def facts(source, official):
    dependencies = set(EXTRA)
    for fixture in (ROOT / 'tools/bace-compat/fixtures').glob('*.json'):
        manifest = json.loads(fixture.read_text())
        if manifest.get('commit') == PIN:
            dependencies.update(manifest.get('source_sha256', {}))
    selected = sorted(p for p in official if (p.startswith(NETWORK) and p.endswith('.cs')) or p in dependencies)
    result = []
    for path in selected:
        data = (source / path).read_bytes()
        blob = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
        if blob != official[path]:
            raise SystemExit('Local source differs from pinned official Git blob: ' + path)
        text = data.decode('utf-8-sig')
        identifiers = re.findall(r'^\s*(\w+)\s*=\s*(0x[0-9A-Fa-f]+|\d+)\s*[,;]', text, re.M)
        handlers = re.findall(r'\[(?:GameAction|GameMessage)\(([^\]]+)\)\]', text)
        result.append(dict(source=path, sha256=hashlib.sha256(data).hexdigest(), git_blob=blob,
                           owner=owner(path), identifiers=[f'{n}={v}' for n, v in identifiers], handlers=handlers))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    url = f'https://api.github.com/repos/ACEmulator/ACE/git/trees/{PIN}?recursive=1'
    tree = json.load(urllib.request.urlopen(url, timeout=60))
    if tree.get('truncated'):
        raise SystemExit('Official tree response truncated')
    official = {x['path']: x['sha'] for x in tree['tree'] if x['type'] == 'blob'}
    rows = facts(args.source, official)
    destination = ROOT / 'docs/network-coverage.toml'
    previous = tomllib.loads(destination.read_text()) if destination.exists() else {}
    old = {x['source']: x for x in previous.get('sources', [])}
    if args.check:
        if previous.get('commit') != PIN or set(old) != {r['source'] for r in rows}:
            raise SystemExit('Network source inventory differs from pinned source tree')
        for row in rows:
            for key, value in row.items():
                if old[row['source']].get(key) != value:
                    raise SystemExit(f'Inventory mismatch: {row["source"]}: {key}')
        print(f'Verified {len(rows)} network source entries against official Git blobs')
        return
    output = ['# Source inventory, not a claim of complete codec or gameplay support.',
              'version = 1', 'repository = "https://github.com/ACEmulator/ACE"',
              f'commit = "{PIN}"', f'source_count = {len(rows)}']
    for row in rows:
        output.extend(['', '[[sources]]'])
        for key, value in row.items():
            output.append(f'{key} = {json.dumps(value, ensure_ascii=False)}')
        status = old.get(row['source'], {}).get('status', 'unsupported')
        evidence = old.get(row['source'], {}).get('evidence', [])
        output.extend([f'status = {json.dumps(status)}', f'evidence = {json.dumps(evidence)}'])
    destination.write_text('\n'.join(output) + '\n')
    print(f'Inventoried {len(rows)} pinned network source entries')

if __name__ == '__main__':
    main()
