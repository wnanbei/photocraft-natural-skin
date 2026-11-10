#!/usr/bin/env python3
"""Launch this build with an isolated bundle/config and synthetic input on macOS.
Only prepares/owns the process. UI verification must be performed separately.
"""
import argparse
import json
import plistlib
import shutil
import subprocess
import sys
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--run', type=Path, required=True)
parser.add_argument('--source', type=Path, required=True)
args = parser.parse_args()
if sys.platform != 'darwin':
    raise SystemExit('This UI harness requires macOS')
root = Path(__file__).resolve().parents[2]
run = args.run.resolve()
run.mkdir(parents=True, exist_ok=True)
bundle = run / 'Natural Skin Acceptance.app'
shutil.copytree(root / 'dist/Natural Skin.app', bundle, dirs_exist_ok=True)
info = bundle / 'Contents/Info.plist'
plist = plistlib.loads(info.read_bytes())
plist.update(CFBundleIdentifier='io.github.wnanbei.natural-skin.acceptance',
             CFBundleName='Natural Skin Acceptance', CFBundleDisplayName='Natural Skin Acceptance')
info.write_bytes(plistlib.dumps(plist))
subprocess.run(['codesign', '--force', '--sign', '-', str(bundle)], check=True)
stop = run / 'stop-ui'
if stop.exists():
    raise SystemExit('Use a fresh run directory')
with (run / 'gui.log').open('w') as log:
    child = subprocess.Popen([str(bundle / 'Contents/MacOS/natural-skin'),
                              '--data-dir', str(run / 'state'), str(args.source.resolve())],
                             stdout=log, stderr=log, stdin=subprocess.DEVNULL)
    print(json.dumps({'pid': child.pid, 'bundle': str(bundle), 'stopFile': str(stop)}), flush=True)
    try:
        while child.poll() is None and not stop.exists():
            time.sleep(0.25)
    finally:
        if child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
