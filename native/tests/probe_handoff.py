#!/usr/bin/env python3
"""Prepare/hold an isolated macOS action-handoff experiment; UI clicks are manual.

No control socket or user's ordinary PhotoCraft configuration is used. Close the
owned test apps or create RUN/stop-route to finish. --reuse RUN keeps evidence.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import plistlib
import subprocess
import sys
import time

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reuse',type=Path)
parser.add_argument('--desktop',action='store_true',help='Exercise first-run setup with isolated persistent desktop state')
options=parser.parse_args()
if sys.platform!='darwin':raise SystemExit('This isolated desktop probe currently requires macOS')
root=Path(__file__).resolve().parents[2]
backend=root/'dist/natural-skin-companion'
native=root/'dist/Natural Skin.app/Contents/MacOS/natural-skin'
if not backend.is_file() or not native.is_file():raise SystemExit('Run sh build.sh first')
base=root/('test-output/desktop-workflow' if options.desktop else 'test-output/handoff-route')
base.mkdir(parents=True,exist_ok=True)
run=options.reuse.resolve() if options.reuse else base/f'run-{time.time_ns()}'
run=run.resolve();run.mkdir(parents=True,exist_ok=True)
(base/'latest.json').write_text(json.dumps({'directory':str(run)}))
for folder in ['config','inbox']:(run/folder).mkdir(mode=0o700,exist_ok=True)
bundle=run/'PhotoCraft Route Test.app'
if not bundle.exists():
    subprocess.run(['cp','-cR','/Applications/PhotoCraft.app',str(bundle)],check=True)
    info=bundle/'Contents/Info.plist';p=plistlib.loads(info.read_bytes())
    p.update(CFBundleIdentifier='io.github.wnanbei.photocraft.handoff-test',CFBundleName='PhotoCraft Route Test',CFBundleDisplayName='PhotoCraft Route Test')
    info.write_bytes(plistlib.dumps(p))
    subprocess.run(['codesign','--force','--sign','-',str(bundle)],check=True)
preferences=run/'config/preferences.json'
if not preferences.exists():preferences.write_text(json.dumps({'integrations':{'allowAgentControl':False,'controlPort':0}}))
if json.loads(preferences.read_text()).get('integrations',{}).get('allowAgentControl') is not False:raise SystemExit('Test config must keep Agent Control off')
source=run/'source.pcraft'
if not source.exists():subprocess.run([str(backend),'fixture',str(source),'16'],check=True)
installer=run/'Install Native Action.json'
if not options.desktop and not installer.exists():subprocess.run([str(backend),'handoff-action',str(run/'inbox'),str(installer),'Natural Skin Native Send'],check=True)
stop=run/'stop-route';stop.unlink(missing_ok=True)
env=os.environ.copy();env['PHOTOCRAFT_CONFIG_DIR']=str(run/'config')
logs=[(run/'desktop-resumed.log').open('w'),(run/'native-resumed.log').open('w')]
processes=[]
try:
    processes.append(subprocess.Popen([str(bundle/'Contents/MacOS/PhotoCraft'),'--safe-gpu',str(source)],env=env,stdin=subprocess.DEVNULL,stdout=logs[0],stderr=logs[0]))
    native_args=([str(native),'--data-dir',str(run/'state')] if options.desktop else [str(native),'--watch',str(run/'inbox'),'--return-to',str(bundle)])
    processes.append(subprocess.Popen(native_args,stdin=subprocess.DEVNULL,stdout=logs[1],stderr=logs[1]))
    evidence={'run':str(run),'hostPid':processes[0].pid,'nativePid':processes[1].pid,'agentControl':False,'sourceSha256':hashlib.sha256(source.read_bytes()).hexdigest(),'guiSha256':hashlib.sha256(native.read_bytes()).hexdigest(),'backendSha256':hashlib.sha256(backend.read_bytes()).hexdigest(),'installer':str(run/'state/Install Natural Skin.json' if options.desktop else installer),'desktopMode':options.desktop}
    (run/'route-processes.json').write_text(json.dumps(evidence,indent=2)+'\n')
    print(json.dumps(evidence,indent=2),flush=True)
    if options.desktop:print('In Natural Skin, choose the isolated test app, Enable connection, then import the generated installer in PhotoCraft.',flush=True)
    print('In the test app: File > Scripts > Browse installer (once); Window > Actions > Button Mode. Click Natural Skin Native Send; then Apply to PhotoCraft.',flush=True)
    while not stop.exists() and all(p.poll() is None for p in processes):time.sleep(.25)
finally:
    for process in processes:
        if process.poll() is None:process.terminate()
    for process in processes:
        try:process.wait(timeout=10)
        except subprocess.TimeoutExpired:process.kill();process.wait()
    for log in logs:log.close()
