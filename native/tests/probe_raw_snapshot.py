#!/usr/bin/env python3
"""Inspect an owned 64x48 RGBA32 fixture snapshot, not a general pcraft reader.

Requires the optional zstd CLI for this development probe only. The native
companion runtime does not depend on Python or the zstd executable.
"""
from pathlib import Path
import json
import struct
import subprocess
import zipfile

root = Path(__file__).resolve().parents[2]
base = root / "test-output/native-feasibility"
directory = Path(json.loads((base / "latest-run.json").read_text())["directory"])
desktop = json.loads((directory / "desktop-control.json").read_text())
with zipfile.ZipFile(desktop["rawSnapshotPath"]) as archive:
    manifest = json.loads(archive.read("manifest.json"))
    document = manifest["document"]
    assert manifest["format_version"] == 1 and document["size"] == {"width":64,"height":48}
    surface = document["layers"][0]["content"]["surface"]
    assert surface["format"] == {"mode":"Rgb","sample":"F32","alpha":True}
    assert len(surface["tiles"]) == 1
    tile = surface["tiles"][0]
    assert (tile["tx"],tile["ty"]) == (0,0)
    decoded = subprocess.run(["zstd","-q","-d","-c"],input=archive.read("tiles/"+tile["hash"]+".zst"),capture_output=True,check=True,timeout=10).stdout
assert len(decoded) == 256*256*4*4
expected = json.loads(subprocess.check_output([str(root/"target/release/natural-skin-companion"),"dump",str(directory/"input-32.tif")],text=True))
maximum = hidden = 0.0
for y in range(48):
    for x in range(64):
        actual = struct.unpack_from("<4f",decoded,(y*256+x)*16)
        source = struct.unpack("<4f",struct.pack("<4f",*expected["pixels"][(y*64+x)*4:(y*64+x+1)*4]))
        maximum = max(maximum,*[abs(a-b) for a,b in zip(actual,source)])
        if source[3] == 0:
            hidden = max(hidden,*[abs(a-b) for a,b in zip(actual[:3],source[:3])])
assert maximum == hidden == 0
report = {"formatVersion":manifest["format_version"],"generator":manifest["generator"],"tileBytes":len(decoded),
    "rawPixelMaximumDifference":maximum,"transparentPayloadMaximumDifference":hidden,
    "metadataProfileReferencePresent":bool(document.get("icc_profile")),"resolutionDpi":document["resolution_dpi"],
    "proofOnly":"Read-only Python ZIP/zstd inspection; the native pcraft reader/writer is not implemented."}
(base/"raw-snapshot.json").write_text(json.dumps(report,indent=2)+"\n")
print(json.dumps(report,indent=2))
