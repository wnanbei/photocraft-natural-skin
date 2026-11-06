// No dependencies: Node's core WebAssembly, PNG/zlib, and an optional PhotoCraft CLI.
// Usage: node tests/verify.mjs [plugin.wasm] [photocraft-cli]
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import zlib from 'node:zlib';
import { spawnSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';

const wasmPath = path.resolve(process.argv[2] || 'target/wasm32-unknown-unknown/release/photocraft_natural_skin.wasm');
const cli = process.argv[3];
const outDir = path.resolve(process.env.NATURAL_SKIN_TEST_DIR || 'test-output');
fs.mkdirSync(outDir, { recursive: true });
const bytes = fs.readFileSync(wasmPath);
const module = new WebAssembly.Module(bytes);
assert.deepEqual(WebAssembly.Module.imports(module), []);
const fresh = () => new WebAssembly.Instance(module).exports;
const manifestHost = fresh();
const packed = manifestHost.pc_manifest();
const manifest = JSON.parse(new TextDecoder().decode(new Uint8Array(manifestHost.memory.buffer,
  Number(packed & 0xffffffffn), Number(packed >> 32n))));
assert.equal(manifestHost.pc_abi_version(), 1);
assert.equal(manifest.overlap, 40);
assert.equal(manifest.id, 'io.github.wnanbei.photocraft.natural-skin');

function run(input, w, h, settings = {}, channels = 4, format = 3 << 16 | 256 | 8) {
  const ex = fresh();
  const pp = new TextEncoder().encode(JSON.stringify(settings));
  const buf = ex.pc_alloc(input.byteLength), params = ex.pc_alloc(pp.length);
  new Float32Array(ex.memory.buffer, buf, input.length).set(input);
  new Uint8Array(ex.memory.buffer, params, pp.length).set(pp);
  const code = ex.pc_filter(buf, input.byteLength, w, h, channels, format, params, pp.length);
  assert.equal(code, 0, `pc_filter returned ${code}`);
  return new Float32Array(new Float32Array(ex.memory.buffer, buf, input.length));
}

function make(w, h, pixel) {
  const p = new Float32Array(w*h*4);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) p.set(pixel(x,y), (y*w+x)*4);
  return p;
}
function runWithHalo(input,w,h,settings){
  const halo=manifest.overlap,pw=w+2*halo,ph=h+2*halo;
  const padded=make(pw,ph,(x,y)=>{
    const xx=Math.max(0,Math.min(w-1,x-halo)),yy=Math.max(0,Math.min(h-1,y-halo));
    return input.subarray((yy*w+xx)*4,(yy*w+xx)*4+4);
  });
  const filtered=run(padded,pw,ph,settings),cropped=new Float32Array(input.length);
  for(let y=0;y<h;y++)cropped.set(filtered.subarray(((y+halo)*pw+halo)*4,((y+halo)*pw+halo+w)*4),y*w*4);
  return cropped;
}
const skinNoise = (x,y) => ((x*17+y*31+x*y*7)%101 / 100 - 0.5)*0.05;
const w = 240, h = 180;
const input = make(w,h,(x,y) => {
  const n = skinNoise(x,y);
  return x < 120 ? [0.64+n,0.44+n,0.33+n,1] : [0.16,0.29,0.87,1];
});
const defaultOutput = run(input,w,h);
let beforeNoise=0, afterNoise=0, unaffected=0;
for (let y=25;y<h-25;y++) for(let x=25;x<95;x++) {
  const i=(y*w+x)*4;
  beforeNoise+=(input[i]-0.64)**2;
  afterNoise+=(defaultOutput[i]-0.64)**2;
}
for(let y=0;y<h;y++) for(let x=130;x<w;x++) {
  const i=(y*w+x)*4;
  for(let c=0;c<4;c++) unaffected=Math.max(unaffected,Math.abs(input[i+c]-defaultOutput[i+c]));
}
assert.ok(afterNoise < beforeNoise*0.8, `Insufficient smoothing: ${afterNoise/beforeNoise}`);
assert.equal(unaffected,0,'Skin-only mode changed blue pixels');
assert.ok(Math.abs(defaultOutput[(90*w+119)*4]-input[(90*w+119)*4])<0.025,'Sharp edge was smeared');
const texture0 = run(input,w,h,{Texture:0});
const texture100 = run(input,w,h,{Texture:100});
const energy = arr => arr.reduce((s,v,i) => s + (v-input[i])**2,0);
assert.ok(energy(texture100)<energy(texture0),'Texture control should reduce high-frequency changes');

// Uniformly scaling straight alpha must not change texture protection in RGB.
// Include varying opacity and transparent holes, not just fully opaque fixtures.
for (const varyingAlpha of [false, true]) {
  const tw=48, th=36;
  const textured=make(tw,th,(x,y)=> {
    const n=skinNoise(x,y)*1.6;
    return [0.64+n,0.44+n,0.33+n,varyingAlpha ? ((x+y)%5)/4 : 1];
  });
  for (const radius of [1,6,20]) for (const texture of [60,100]) {
    const settings={Radius:radius,Texture:texture};
    const reference=run(textured,tw,th,settings);
    for (const scale of [0.1,0.5]) {
      const faded=textured.slice();
      for(let i=3;i<faded.length;i+=4) faded[i]*=scale;
      const actual=run(faded,tw,th,settings);
      let worst=0;
      for(let i=0;i<actual.length;i+=4) {
        assert.equal(reference[i+3],textured[i+3]);
        assert.equal(actual[i+3],faded[i+3]);
        for(let c=0;c<3;c++) worst=Math.max(worst,Math.abs(actual[i+c]-reference[i+c]));
      }
      assert.ok(worst<2e-6,
        `Alpha changed texture protection: varying=${varyingAlpha}, radius=${radius}, texture=${texture}, scale=${scale}, difference=${worst}`);
    }
  }
}

const zero = run(input,w,h,{Strength:0});
assert.deepEqual(new Uint8Array(zero.buffer),new Uint8Array(input.buffer));

// Entirely/partially transparent pixels, HDR values and NaNs must be preserved.
const special=make(40,30,(x,y)=>[0.64+skinNoise(x,y),0.43,0.31,((x+y)%5)/4]);
special.set([2,0.4,0.3,1],4);
special.set([NaN,0.4,0.3,1],8);
const specialOut=run(special,40,30,{Strength:100});
for(let i=0;i<special.length;i+=4) {
  assert.equal(specialOut[i+3],special[i+3]);
  if(special[i+3]===0 || i===4 || i===8) {
    for(let c=0;c<4;c++) assert.ok(Object.is(specialOut[i+c],special[i+c]));
  } else for(let c=0;c<3;c++) assert.ok(Number.isFinite(specialOut[i+c]));
}

// Replicated halos around independently processed strips must yield the same inner pixels.
for (const radius of [1,6,20]) {
  const full=runWithHalo(input,w,h,{Radius:radius,Skin_Only:false});
  let worst=0;
  for(const [start,end] of [[0,73],[73,h]]) {
    const halo=manifest.overlap, bh=end-start+2*halo;
    const band=make(w+2*halo,bh,(x,y)=> {
      const xx=Math.max(0,Math.min(w-1,x-halo));
      const yy=Math.max(0,Math.min(h-1,start+y-halo));
      return input.subarray((yy*w+xx)*4,(yy*w+xx)*4+4);
    });
    const result=run(band,w+2*halo,bh,{Radius:radius,Skin_Only:false});
    for(let y=start;y<end;y++)for(let x=0;x<w;x++)for(let c=0;c<4;c++) {
      const a=full[(y*w+x)*4+c], b=result[((y-start+halo)*(w+2*halo)+x+halo)*4+c];
      worst=Math.max(worst,Math.abs(a-b));
    }
  }
  assert.ok(worst<2e-6,`Band seam, radius=${radius}: ${worst}`);
}

// ABI failures must be explicit, including unsupported grayscale/CMYK and overflow sizes.
{
  const ex=fresh(), p=ex.pc_alloc(16), j=ex.pc_alloc(2);
  new Uint8Array(ex.memory.buffer,j,2).set([123,125]);
  assert.equal(ex.pc_filter(p,16,1,1,4,(4<<16)|8,j,2),2);
  assert.equal(ex.pc_filter(p,16,1,1,4,(3<<16)|256|8,j,2),0);
  assert.equal(ex.pc_filter(p,16,0xffffffff,2,4,(3<<16)|256|8,j,2),1);
  assert.equal(ex.pc_filter(p+1,16,1,1,4,(3<<16)|256|8,j,2),4);
}

function crc32(data) {
  let c=0xffffffff;
  for(const byte of data){c^=byte;for(let k=0;k<8;k++) c=c&1 ? 0xedb88320^(c>>>1):c>>>1;}
  return (c^0xffffffff)>>>0;
}
function chunk(type,data) {
  const t=Buffer.from(type), b=Buffer.alloc(12+data.length);
  b.writeUInt32BE(data.length);t.copy(b,4);data.copy(b,8);
  b.writeUInt32BE(crc32(Buffer.concat([t,data])),8+data.length);return b;
}
function png(p,w,h) {
  const ihdr=Buffer.alloc(13);ihdr.writeUInt32BE(w);ihdr.writeUInt32BE(h,4);ihdr[8]=8;ihdr[9]=6;
  const raw=Buffer.alloc(h*(w*4+1));
  for(let y=0;y<h;y++)for(let x=0;x<w*4;x++) raw[y*(w*4+1)+1+x]=Math.round(Math.max(0,Math.min(1,p[(y*w*4)+x]))*255);
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',ihdr),chunk('IDAT',zlib.deflateSync(raw)),chunk('IEND',Buffer.alloc(0))]);
}
function decodePng(file) {
  const p=fs.readFileSync(file), idat=[];let width,height,channels;
  for(let o=8;o<p.length;){const len=p.readUInt32BE(o),type=p.toString('ascii',o+4,o+8),data=p.subarray(o+8,o+8+len);
    if(type==='IHDR'){width=data.readUInt32BE(0);height=data.readUInt32BE(4);assert.equal(data[8],8);assert.equal(data[12],0);channels=({0:1,2:3,4:2,6:4})[data[9]];assert.ok(channels);}
    if(type==='IDAT')idat.push(data);o+=12+len;
  }
  const raw=zlib.inflateSync(Buffer.concat(idat)),row=width*channels,out=Buffer.alloc(height*row);
  function paeth(a,b,c){const p=a+b-c,pa=Math.abs(p-a),pb=Math.abs(p-b),pc=Math.abs(p-c);return pa<=pb&&pa<=pc?a:pb<=pc?b:c;}
  for(let y=0;y<height;y++){const f=raw[y*(row+1)];for(let x=0;x<row;x++){
    const a=x>=channels?out[y*row+x-channels]:0,b=y?out[(y-1)*row+x]:0,c=y&&x>=channels?out[(y-1)*row+x-channels]:0;
    out[y*row+x]=(raw[y*(row+1)+1+x]+([0,a,b,Math.floor((a+b)/2),paeth(a,b,c)][f]))&255;
  }}return {width,height,channels,data:out};
}

fs.writeFileSync(path.join(outDir,'synthetic-before.png'),png(input,w,h));
fs.writeFileSync(path.join(outDir,'synthetic-after.png'),png(defaultOutput,w,h));
const report={abi:1,wasmBytes:bytes.length,imports:0,syntheticNoiseVarianceRatio:afterNoise/beforeNoise,
  bluePixelsMaxChange:unaffected,checks:['native kernel tests separately','ABI and manifest','skin smoothing and blue protection',
    'edge retention','texture control','texture protection invariant under uniform alpha scaling',
    'zero strength identity','alpha / transparent / HDR / NaN preservation',
    'seam-free independent strips at radii 1, 6 and 20','invalid ABI inputs rejected']};

if(cli){
  function hostRun(inputFile,commands,outputFile){
    const args=['run',inputFile,'--cmd','plugin.install','--params',JSON.stringify({path:wasmPath})];
    for(const [id,params] of commands)args.push('--cmd',id,'--params',JSON.stringify(params));
    args.push('--out',outputFile);
    const result=spawnSync(cli,args,{encoding:'utf8',timeout:120000});
    assert.equal(result.status,0,result.stderr || String(result.error));
    return result;
  }
  const smallInput=path.join(outDir,'synthetic-before.png');
  const selectionPath=path.join(outDir,'host-selection.png');
  hostRun(smallInput,[['select.rect',{x:20,y:20,width:60,height:60}],
    ['plugin.run',{id:manifest.id,params:{Strength:90,Skin_Only:false}}]],selectionPath);
  const selected=decodePng(selectionPath),smallDecoded=decodePng(smallInput);
  let outsideChanges=0,insideChanges=0;
  for(let y=0;y<h;y++)for(let x=0;x<w;x++)for(let c=0;c<3;c++){
    const a=selected.data[(y*w+x)*selected.channels+c],b=smallDecoded.data[(y*w+x)*smallDecoded.channels+c];
    if(a!==b){if(x>=20&&x<80&&y>=20&&y<80)insideChanges++;else outsideChanges++;}
  }
  assert.equal(outsideChanges,0);assert.ok(insideChanges>0);
  const undoPath=path.join(outDir,'host-undo.png');
  hostRun(smallInput,[['plugin.run',{id:manifest.id}],['edit.undo',{}]],undoPath);
  const undone=decodePng(undoPath);
  for(let i=0;i<w*h;i++)for(let c=0;c<3;c++)assert.equal(undone.data[i*undone.channels+c],smallDecoded.data[i*smallDecoded.channels+c]);
  for(const bits of [16,32]){
    const outputFile=path.join(outDir,`host-rgb${bits}.pcraft`);
    const result=hostRun(smallInput,[[`image.mode.bits${bits}`,{}],['plugin.run',{id:manifest.id}]],outputFile);
    assert.ok(fs.statSync(outputFile).size>0);
    fs.writeFileSync(path.join(outDir,`host-rgb${bits}-log.txt`),result.stdout);
  }
  report.checks.push('PhotoCraft selection isolates changes','PhotoCraft Undo restores original',
    'PhotoCraft RGB 16-bit and 32-bit processing/save');
  const cw=1536,ch=1024;
  const large=make(cw,ch,(x,y)=>{
    const n=skinNoise(x,y),step=x<768?0.0:0.15;
    return [0.64+n-step,0.44+n-step,0.33+n-step,1];
  });
  const inputFile=path.join(outDir,'host-large-input.png'),outputFile=path.join(outDir,'host-large-output.png');
  fs.writeFileSync(inputFile,png(large,cw,ch));
  const args=['run',inputFile,'--cmd','plugin.install','--params',JSON.stringify({path:wasmPath}),
    '--cmd','plugin.run','--params',JSON.stringify({id:manifest.id,params:{Radius:20,Skin_Only:false}}),'--out',outputFile];
  const start=performance.now(), proc=spawnSync(cli,args,{encoding:'utf8',timeout:120000});
  assert.equal(proc.status,0,proc.stderr || String(proc.error));
  fs.writeFileSync(path.join(outDir,'host-large-log.txt'),proc.stdout+'\n'+proc.stderr);
  const seconds=(performance.now()-start)/1000;
  const decoded=decodePng(outputFile);
  assert.equal(decoded.width,cw);assert.equal(decoded.height,ch);
  // Compare host bands to an independent whole-image invocation of the same WASM.
  const quantized=new Float32Array(large.length);
  for(let i=0;i<large.length;i++)quantized[i]=Math.round(large[i]*255)/255;
  const direct=runWithHalo(quantized,cw,ch,{Radius:20,Skin_Only:false});
  let maxError=0,errors=0;
  for(let i=0;i<cw*ch;i++)for(let c=0;c<3;c++){
    const expected=Math.round(direct[i*4+c]*255),actual=decoded.data[i*decoded.channels+c];
    const e=Math.abs(expected-actual);maxError=Math.max(e,maxError);if(e>1)errors++;
  }
  assert.equal(errors,0,`Host band seams/roundtrip differences: max error ${maxError}`);
  report.host={version:spawnSync(cli,['--version'],{encoding:'utf8'}).stdout.trim(),
    width:cw,height:ch,radius:20,seconds,max8bitDifferenceFromUnsplitWasm:maxError};
}
fs.writeFileSync(path.join(outDir,'verification.json'),JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify(report,null,2));
