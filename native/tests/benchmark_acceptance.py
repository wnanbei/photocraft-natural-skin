#!/usr/bin/env python3
"""Sequential release-build measurements; never runs builds or benchmarks concurrently.

python3 native/tests/benchmark_acceptance.py --output test-output/acceptance-2026-11-10
"""
import argparse
import hashlib
import json
import platform
import re
import subprocess
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, default=Path('target/release/natural-skin-companion'))
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if (output / 'measurements.json').exists():
        parser.error('Use a fresh output directory; previous reports are not overwritten')
    records = {}

    def run(name, *arguments, parse=True):
        command = [str(binary), *map(str, arguments)]
        measured = ['/usr/bin/time', '-l' if platform.system() == 'Darwin' else '-v', *command]
        start = time.monotonic()
        result = subprocess.run(measured, text=True, capture_output=True, check=False)
        (output / f'{name}.stdout').write_text(result.stdout)
        (output / f'{name}.stderr').write_text(result.stderr)
        result.check_returncode()
        rss_match = re.search(r'(\d+)\s+maximum resident set size', result.stderr)
        rss = int(rss_match[1]) if rss_match else None
        if rss is None:
            rss_match = re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)', result.stderr)
            rss = int(rss_match[1]) * 1024 if rss_match else None
        records[name] = {'command': command, 'wallMs': (time.monotonic() - start)*1000,
                         'maxRssBytes': rss, 'result': json.loads(result.stdout) if parse else None}
        print(name, 'complete', flush=True)
        return records[name]['result']

    quality = output / 'quality'
    run('quality', 'quality-suite', quality)
    run('whole_24mp', 'bench', 6000, 4000)
    run('tiled_24mp', 'bench-tiled', 6000, 4000)
    run('cached_preview', 'bench-preview', 1024, 768)
    for index in range(3):
        run(f'cancel_{index}', 'bench-cancel', 6000, 4000)
    source = output / 'source-24mp.tiff'
    run('fixture', 'fixture', source, 16, quality / 'synthetic-original.tiff', 6000, 4000, parse=False)
    source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
    result = output / 'result-24mp.tiff'
    run('workflow_24mp', 'process', source, result)
    run('preservation', 'compare', source, result)
    identity = output / 'identity-24mp.tiff'
    run('identity_workflow', 'process', source, identity, '{"Strength":0}')
    identity_report = run('identity_comparison', 'compare', source, identity)
    assert all(identity_report[key] == 0 for key in ['visibleRgbMaxDifference', 'alphaMaxDifference', 'transparentPayloadMaxDifference'])
    assert hashlib.sha256(source.read_bytes()).hexdigest() == source_hash
    summary = {'platform': platform.platform(), 'machine': platform.machine(),
               'binarySha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
               'sourceSha256Unchanged': source_hash, 'records': records,
               'scope': 'Synthetic release build on this machine. File workflow includes read/ICC/process/write; PhotoCraft capture/return and display excluded. RSS is whole child process peak, not scratch allocation.'}
    (output / 'measurements.json').write_text(json.dumps(summary, indent=2, ensure_ascii=False))
    print(output / 'measurements.json', flush=True)


if __name__ == '__main__':
    main()
