use natural_skin_companion::{Frame, Result, exchange::ImageDocument, fixture};
use natural_skin_core::{NativePreview, NativeStage, retouch_native};
use serde_json::json;
use std::{env, path::Path, time::Instant};

fn same_resolution(a: &Frame, b: &Frame) -> bool {
    match (a.resolution, b.resolution) {
        (None, None) => true,
        (Some((ua, xa, ya)), Some((ub, xb, yb))) => {
            ua == ub
                && u64::from(xa[0]) * u64::from(xb[1]) == u64::from(xb[0]) * u64::from(xa[1])
                && u64::from(ya[0]) * u64::from(yb[1]) == u64::from(yb[0]) * u64::from(ya[1])
        }
        _ => false,
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        None | Some("--help") | Some("-h") => {
            println!(
                "Natural Skin native image backend\n\nCommands:\n  fixture OUTPUT DEPTH [ICC_SOURCE] [WIDTH HEIGHT]\n  inspect INPUT\n  process INPUT OUTPUT [PARAMETERS_JSON]\n  compare FIRST SECOND\n  bench WIDTH HEIGHT\n  bench-preview WIDTH HEIGHT\n  handoff-action EXCHANGE_DIR OUTPUT.json [ACTION_NAME]\n  control-inspect LOOPBACK:PORT TOKEN_FILE\n  control-check LOOPBACK:PORT TOKEN_FILE SOURCE_TICKET.json\n  dump INPUT (small conformance fixtures only)\n\nINPUT/OUTPUT: .pcraft (single raster) or RGB .tif.\nThis is the CLI backend. Run natural-skin for the desktop interface."
            );
        }
        Some("--version") => println!("Natural Skin native backend {}", env!("CARGO_PKG_VERSION")),
        Some("handoff-action") => {
            let root = Path::new(args.get(2).ok_or("Missing exchange folder")?);
            let path = Path::new(args.get(3).ok_or("Missing installer script path")?);
            let name = args.get(4).map_or("Natural Skin Send", String::as_str);
            let script = natural_skin_companion::handoff::install_action_script(root, name)?;
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            file.write_all(&serde_json::to_vec_pretty(&script)?)?;
            file.sync_all()?;
            println!(
                "{}",
                json!({"script":path,"action":name,"installsVia":"PhotoCraft File > Scripts > Browse"})
            );
        }
        Some("control-inspect") => {
            let endpoint = args.get(2).ok_or("Missing control endpoint")?.parse()?;
            let token = Path::new(args.get(3).ok_or("Missing token file")?);
            let mut client =
                natural_skin_companion::control::ControlClient::connect(endpoint, token)?;
            println!("{}", client.command("document.inspect", json!({}))?);
        }
        Some("control-check") => {
            let endpoint = args.get(2).ok_or("Missing control endpoint")?.parse()?;
            let token = Path::new(args.get(3).ok_or("Missing token file")?);
            let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(
                args.get(4).ok_or("Missing source ticket")?,
            )?)?;
            let index = saved["index"].as_u64().ok_or("Invalid source index")? as usize;
            let ticket = natural_skin_companion::control::SourceTicket::from_snapshot(
                index,
                saved["document"].clone(),
            )?;
            let mut client =
                natural_skin_companion::control::ControlClient::connect(endpoint, token)?;
            ticket.check(&mut client)?;
            println!("{}", json!({"sourceCurrent":true}));
        }
        Some("fixture") => {
            let depth = args
                .get(3)
                .ok_or("fixture OUTPUT DEPTH [ICC_SOURCE] [WIDTH HEIGHT]")?
                .parse()?;
            let icc = match args.get(4) {
                Some(p) => ImageDocument::read(Path::new(p))?.frame().icc.clone(),
                None => None,
            };
            let width: u32 = args.get(5).map_or(Ok(64), |s| s.parse())?;
            let height: u32 = args.get(6).map_or(Ok(48), |s| s.parse())?;
            if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 36_000_000 {
                return Err("Fixture limit: 36 MP".into());
            }
            ImageDocument::Tiff(fixture(width, height, depth, icc))
                .write(Path::new(args.get(2).ok_or("Missing output")?))?;
        }
        Some("inspect") => {
            let document = ImageDocument::read(Path::new(args.get(2).ok_or("Missing input")?))?;
            let image = document.frame();
            println!(
                "{}",
                json!({"width":image.width,"height":image.height,"channels":image.channels,"depth":image.depth,"iccBytes":image.icc.as_ref().map_or(0,Vec::len),"resolution":image.resolution,"hdrSamples":image.pixels.iter().filter(|x|**x>1.0).count()})
            );
        }
        Some("process") => {
            let start = Instant::now();
            eprintln!("{}", json!({"stage":"Read","elapsedMs":0}));
            let mut document = ImageDocument::read(Path::new(args.get(2).ok_or("Missing input")?))?;
            let image = document.frame_mut();
            let params = args.get(4).map_or("{}", String::as_str);
            let _: serde_json::Value = serde_json::from_str(params)?;
            let summary=retouch_native(&mut image.pixels,image.width as usize,image.height as usize,image.channels,params.as_bytes(),|p|{
                eprintln!("{}",json!({"stage":if p.stage==NativeStage::Complete {"RetouchReady".to_string()} else {format!("{:?}",p.stage)},"completed":p.completed,"total":p.total,"elapsedMs":start.elapsed().as_secs_f64()*1000.0}));true
            }).map_err(|e|format!("Native processing failed: {e:?}"))?;
            eprintln!(
                "{}",
                json!({"stage":"Write","elapsedMs":start.elapsed().as_secs_f64()*1000.0})
            );
            document.write(Path::new(args.get(3).ok_or("Missing output")?))?;
            eprintln!(
                "{}",
                json!({"stage":"Complete","elapsedMs":start.elapsed().as_secs_f64()*1000.0})
            );
            println!(
                "{}",
                json!({"status":"complete","milliseconds":start.elapsed().as_secs_f64()*1000.0,"scratchBytes":summary.scratch_bytes,"regions":summary.regions})
            );
        }
        Some("compare") => {
            let a_doc = ImageDocument::read(Path::new(args.get(2).ok_or("Missing first image")?))?;
            let b_doc = ImageDocument::read(Path::new(args.get(3).ok_or("Missing second image")?))?;
            let a = a_doc.frame();
            let b = b_doc.frame();
            if (a.width, a.height, a.channels, a.depth) != (b.width, b.height, b.channels, b.depth)
            {
                return Err("Layout/depth mismatch".into());
            }
            let mut max = 0.0f32;
            let mut alpha = 0.0f32;
            let mut hidden = 0.0f32;
            for (pa, pb) in a
                .pixels
                .chunks_exact(a.channels)
                .zip(b.pixels.chunks_exact(b.channels))
            {
                if a.channels == 4 {
                    alpha = alpha.max((pa[3] - pb[3]).abs());
                }
                for c in 0..3 {
                    if a.channels == 4 && pa[3] == 0.0 {
                        hidden = hidden.max((pa[c] - pb[c]).abs());
                    } else {
                        max = max.max((pa[c] - pb[c]).abs());
                    }
                }
            }
            println!(
                "{}",
                json!({"visibleRgbMaxDifference":max,"alphaMaxDifference":alpha,"transparentPayloadMaxDifference":hidden,"iccBytesEqual":a.icc==b.icc,"resolutionEqual":same_resolution(a,b),"depth":a.depth})
            );
        }
        Some("bench-preview") => {
            let width: u32 = args.get(2).ok_or("Missing width")?.parse()?;
            let height: u32 = args.get(3).ok_or("Missing height")?.parse()?;
            if u64::from(width) * u64::from(height) > 2_000_000 || width == 0 || height == 0 {
                return Err("Preview proof limit: 2 MP".into());
            }
            let image = fixture(width, height, 32, None);
            let mut preview = NativePreview::new(image.pixels, width as usize, height as usize, 4)
                .map_err(|e| format!("{e:?}"))?;
            let start = Instant::now();
            let (_, first) = preview
                .render(b"{}", |_| true)
                .map_err(|e| format!("{e:?}"))?;
            let prepare_ms = start.elapsed().as_secs_f64() * 1000.0;
            let mut samples = Vec::new();
            for strength in [20, 45, 80, 35, 65] {
                let parameters = serde_json::to_vec(
                    &json!({"Strength":strength,"Texture":60,"skinBrightening":30,"skinRosiness":10}),
                )?;
                let start = Instant::now();
                let (_, report) = preview
                    .render(&parameters, |_| true)
                    .map_err(|e| format!("{e:?}"))?;
                if !report.cache_hit {
                    return Err("Expected preview basis reuse".into());
                }
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "{}",
                json!({"width":width,"height":height,"firstFrameMs":prepare_ms,"remixMs":samples,"medianRemixMs":samples[2],"basisBytes":first.cached_bytes,"timing":"native preview source copy + cached mixing; host transfer, TIFF I/O and display upload excluded"})
            );
        }
        Some("dump") => {
            let document = ImageDocument::read(Path::new(args.get(2).ok_or("Missing input")?))?;
            let image = document.frame();
            if image.pixels.len() > 1_000_000 {
                return Err("Dump is limited to small conformance fixtures".into());
            }
            println!(
                "{}",
                json!({"width":image.width,"height":image.height,"channels":image.channels,"depth":image.depth,"pixels":image.pixels})
            );
        }
        Some("bench") => {
            let width: u32 = args.get(2).ok_or("Missing width")?.parse()?;
            let height: u32 = args.get(3).ok_or("Missing height")?.parse()?;
            if u64::from(width) * u64::from(height) > 36_000_000 || width == 0 || height == 0 {
                return Err("Native proof limit: 36 MP".into());
            }
            let mut samples = Vec::new();
            let mut scratch = 0;
            for _ in 0..3 {
                let mut image = fixture(width, height, 32, None);
                let start = Instant::now();
                let r = retouch_native(
                    &mut image.pixels,
                    width as usize,
                    height as usize,
                    4,
                    b"{}",
                    |_| true,
                )
                .map_err(|e| format!("{e:?}"))?;
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
                scratch = r.scratch_bytes;
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "{}",
                json!({"width":width,"height":height,"nativeMs":samples,"medianMs":samples[1],"scratchBytes":scratch,"timing":"allocation + native kernel only; source construction, TIFF I/O and host transfer excluded"})
            );
        }
        _ => {
            return Err("Unknown command; use --help".into());
        }
    }
    Ok(())
}
