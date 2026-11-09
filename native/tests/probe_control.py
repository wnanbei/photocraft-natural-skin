#!/usr/bin/env python3
"""Probe a separate PhotoCraft instance; never attach to the user's existing window."""
from pathlib import Path
import json
import hashlib
import os
import socket
import subprocess
import time

root = Path(__file__).resolve().parents[2]
executable = root / "target/release/natural-skin-companion"
if not executable.is_file():
    raise RuntimeError("Build natural-skin-companion in release mode before the desktop probe")
exchange_ext = os.environ.get("NATURAL_SKIN_EXCHANGE", "tif")
if exchange_ext not in ("tif", "pcraft"):
    raise RuntimeError("NATURAL_SKIN_EXCHANGE must be tif or pcraft")
base = root / "test-output/native-feasibility"
base.mkdir(parents=True, exist_ok=True)
out = base / f"run-{time.time_ns()}"
out.mkdir(parents=True, exist_ok=True)
(base / ("latest-pcraft-run.json" if exchange_ext == "pcraft" else "latest-run.json")).write_text(json.dumps({"directory":str(out)}))
config = out / "gui-config"
config.mkdir(exist_ok=True)
token_file = out / "control.token"
with socket.socket() as reservation:
    reservation.bind(("127.0.0.1", 0))
    port = reservation.getsockname()[1]
env = os.environ.copy()
env["PHOTOCRAFT_CONFIG_DIR"] = str(config)
binary = os.environ.get("PHOTOCRAFT_APP", "/Applications/PhotoCraft.app/Contents/MacOS/PhotoCraft")
log = (out / "gui.log").open("w")
app = subprocess.Popen([binary, "--safe-gpu", "--control", str(port),
    "--control-token-file", str(token_file), "--automation-read-root", str(out),
    "--automation-write-root", str(out)], env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
connection = None
report = {"transport": "isolated desktop / authenticated TCP", "exchangeFormat": exchange_ext, "results": []}
try:
    for attempt in range(100):
        if app.poll() is not None:
            raise RuntimeError("Isolated PhotoCraft exited; see gui.log")
        try:
            connection = socket.create_connection(("127.0.0.1", port), timeout=.5)
            break
        except (OSError, TimeoutError):
            time.sleep(.2)
    if connection is None:
        raise RuntimeError("Control socket did not start")
    connection.settimeout(30)
    stream = connection.makefile("rwb")
    sequence = 0
    def call(method, params):
        global sequence
        sequence += 1
        stream.write((json.dumps({"id": sequence, "method": method, "params": params})+"\n").encode())
        stream.flush()
        reply = json.loads(stream.readline())
        if method != "auth":
            report["results"].append({"method": method, "params": params, "reply": reply})
            (out / "desktop-control.json").write_text(json.dumps(report, indent=2)+"\n")
            print(method, params.get("command", ""), "ok" if reply.get("ok") else reply.get("error"), flush=True)
        return reply
    assert call("auth", {"token": token_file.read_text().strip()})["ok"]
    assert call("engine.execute", {"command": "file.new", "params": {"width":64,"height":48,"mode":"rgb","depth":32}})["ok"]
    before=call("engine.execute", {"command":"document.inspect","params":{}})
    call("doc.render", {"maxSide":64})
    call("engine.execute", {"command":"file.saveACopy","params":{"path":str(out/"copy.tif"),"tiffTransparency":True}})
    call("app.save", {"path":str(out/"composite.tif"),"tiffTransparency":True})
    layers = out / "layers"
    layers.mkdir(exist_ok=True)
    call("engine.execute", {"command":"file.export.layersToFiles","params":{"dir":str(layers),"format":"tiff","visibleOnly":False}})
    if (out/"composite.tif").exists():
        call("engine.execute", {"command":"file.placeEmbedded","params":{"path":str(out/"composite.tif"),"fit":False,"scale":100}})
    duplicate=call("engine.execute", {"command":"layer.duplicate","params":{"document":"new","documentName":"Natural Skin capture"}})
    if duplicate.get("ok"):
        call("app.save", {"path":str(out/"layer.tif"),"tiffTransparency":True})
        call("ui.menu.invoke", {"id":"window.document.0"})
        unchanged=call("engine.execute", {"command":"document.inspect","params":{}})
        report["sourceUnchangedAfterCapture"]=unchanged.get("result")==before.get("result")
        opened=call("app.open", {"path":str(out/"layer.tif")})
        if opened.get("ok"):
            copied=call("engine.execute", {"command":"layer.copyToDocument","params":{"document":0,"name":"Natural Skin result"}})
            if copied.get("ok"):
                call("engine.execute", {"command":"edit.undo","params":{}})
    call("engine.execute", {"command":"document.inspect","params":{}})
    if executable.exists():
        def command(name,params=None):
            result=call("engine.execute",{"command":name,"params":params or {}})
            if not result.get("ok"):raise RuntimeError(result.get("error"))
            return result["result"]
        def native(*arguments):
            result=subprocess.run([str(executable),*map(str,arguments)],capture_output=True,text=True,check=True)
            parsed=json.loads(result.stdout) if result.stdout.strip() else None
            if arguments[0]=="process":parsed["events"]=[json.loads(line) for line in result.stderr.splitlines() if line.startswith("{")]
            return parsed
        report["rustControlClientConnected"]=native("control-inspect",f"127.0.0.1:{port}",token_file)["width"]==64
        report["nativeExecutableSha256"]=hashlib.sha256(executable.read_bytes()).hexdigest()
        report["precisionCases"]=[]
        for depth in [8,16,32]:
            input_path=out/f"input-{depth}.{exchange_ext}"
            dimensions=[267,259] if exchange_ext=="pcraft" else [64,48]
            native("fixture",input_path,depth,out/"layer.tif",*dimensions)
            for mode,parameters in [("identity",{"Strength":0}),("retouch",{})]:
                assert call("app.open",{"path":str(input_path)})["ok"]
                original_index=command("session.inspect")["active"]
                original=command("document.inspect")
                temporary=command("layer.duplicate",{"layer":original["activeLayer"],"document":"new","documentName":"Natural Skin precision capture"})["document"]
                if depth==32 and mode=="identity":
                    snapshot=out/"raw-layer.pcraft"
                    assert call("app.save",{"path":str(snapshot)})["ok"]
                    report["rawSnapshotPath"]=str(snapshot)
                captured=out/f"capture-{depth}-{mode}.{exchange_ext}"
                assert call("app.save",{"path":str(captured),"tiffTransparency":True})["ok"]
                command("file.close",{"document":temporary,"save":"discard"})
                command("document.activate",{"document":original_index})
                unchanged=command("document.inspect")==original
                assert unchanged, "Capture changed the source document"
                processed=out/f"processed-{depth}-{mode}.{exchange_ext}"
                processing=native("process",captured,processed,json.dumps(parameters))
                before_commit=command("document.inspect")
                assert before_commit["revision"]==original["revision"] and before_commit["activeLayer"]==original["activeLayer"],"Stale source"
                assert call("app.open",{"path":str(processed)})["ok"]
                result_index=command("session.inspect")["active"]
                copied=command("layer.copyToDocument",{"source":result_index,"document":original_index,"name":"Natural Skin"})
                command("layer.setProps",{"layer":original["activeLayer"],"visible":False})
                returned=out/f"returned-{depth}-{mode}.{exchange_ext}"
                if exchange_ext=="pcraft":
                    returned_doc=command("layer.duplicate",{"layer":copied["layers"][0],"document":"new","documentName":"Natural Skin return verification"})["document"]
                assert call("app.save",{"path":str(returned),"tiffTransparency":True})["ok"]
                if exchange_ext=="pcraft":
                    command("file.close",{"document":returned_doc,"save":"discard"})
                    command("document.activate",{"document":original_index})
                result={"depth":depth,"mode":mode,"sourceUnchangedAfterCapture":unchanged,
                    "capture":native("compare",input_path,captured),
                    "return":native("compare",processed,returned),"nativeProcessing":processing,
                    "captureInfo":native("inspect",captured),"returnedInfo":native("inspect",returned)}
                tolerance=2e-6 if depth==32 else 1/(255 if depth==8 else 65535)
                for direction in ["capture","return"]:
                    assert result[direction]["visibleRgbMaxDifference"]<=tolerance
                    assert result[direction]["alphaMaxDifference"]==0
                    if exchange_ext=="pcraft":
                        assert result[direction]["visibleRgbMaxDifference"]==0
                        assert result[direction]["transparentPayloadMaxDifference"]==0
                    assert result[direction]["iccBytesEqual"] and result[direction]["resolutionEqual"]
                command("edit.undo");command("edit.undo")
                result["undoRestoresLayers"]=command("document.inspect")["layers"]==original["layers"]
                assert result["undoRestoresLayers"], "Two-step Undo failed to restore source layers"
                report["precisionCases"].append(result)
                command("file.close",{"document":result_index,"save":"discard"})
                command("file.close",{"document":original_index,"save":"discard"})
                (out/"desktop-control.json").write_text(json.dumps(report,indent=2)+"\n")
        command("file.new",{"width":64,"height":48,"mode":"rgb","depth":32})
        index=command("session.inspect")["active"]
        ticket=out/"source-ticket.json"
        ticket.write_text(json.dumps({"index":index,"document":command("document.inspect")}))
        native("control-check",f"127.0.0.1:{port}",token_file,ticket)
        command("layer.new.layer",{"name":"Simulated source change"})
        stale=subprocess.run([str(executable),"control-check",f"127.0.0.1:{port}",str(token_file),str(ticket)],capture_output=True,text=True)
        report["liveStaleGuardRejectedChange"]=stale.returncode!=0 and "Source changed" in stale.stderr
        assert report["liveStaleGuardRejectedChange"]
        command("file.close",{"document":index,"save":"discard"})
    call("app.quit", {})
finally:
    if connection is not None:
        connection.close()
    try:
        app.wait(timeout=5)
    except subprocess.TimeoutExpired:
        app.terminate()
        app.wait(timeout=5)
    log.close()
