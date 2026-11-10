use natural_skin_companion::{
    color::ColorPipeline,
    exchange::ImageDocument,
    fixture,
    pcraft::PcraftDocument,
    processing::{self, Crop},
    worker::{Event, Job, Worker},
};
use serde_json::json;
use std::time::Duration;
fn completed(worker: &Worker, id: u64) -> Event {
    loop {
        let event = worker.events.recv_timeout(Duration::from_secs(10)).unwrap();
        if event.id() == id && !matches!(event, Event::Progress { .. }) {
            return event;
        }
    }
}
#[test]
fn detail_uses_full_source_while_export_keeps_full_dimensions_and_icc() {
    let root = std::env::temp_dir().join(format!(
        "natural-skin-detail-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let source = fixture(
        301,
        287,
        32,
        Some(moxcms::ColorProfile::new_display_p3().encode().unwrap()),
    );
    let input = root.join("input.pcraft");
    let output = root.join("full.pcraft");
    PcraftDocument::from_frame(source.clone(), "Detail acceptance")
        .unwrap()
        .write(&input)
        .unwrap();
    let worker = Worker::new(|| {});
    let id = worker.submit(Job::Load(input.clone())).unwrap();
    assert!(matches!(completed(&worker, id), Event::Loaded { .. }));
    let parameters = json!({"Radius":20,"Strength":55,"Texture":65,"skinBrightening":20});
    let crop = Crop {
        x: 243,
        y: 229,
        width: 58,
        height: 58,
    };
    let id = worker
        .submit(Job::Detail(crop, parameters.clone()))
        .unwrap();
    let pixels = match completed(&worker, id) {
        Event::Rendered {
            pixels,
            original: Some(original),
            ..
        } => {
            assert_eq!((pixels.width, pixels.height), (58, 58));
            assert_eq!((original.width, original.height), (58, 58));
            pixels
        }
        _ => panic!("Expected detail preview"),
    };
    let id = worker
        .submit(Job::Export(output.clone(), parameters.clone()))
        .unwrap();
    assert!(matches!(completed(&worker, id), Event::Exported { .. }));
    let exported = ImageDocument::read(&output).unwrap();
    let frame = exported.frame();
    assert_eq!((frame.width, frame.height), (301, 287));
    assert_eq!(frame.icc, source.icc);
    let (au, ax, ay) = frame.resolution.unwrap();
    let (bu, bx, by) = source.resolution.unwrap();
    assert_eq!(au, bu);
    for (a, b) in [(ax, bx), (ay, by)] {
        assert_eq!(
            u64::from(a[0]) * u64::from(b[1]),
            u64::from(b[0]) * u64::from(a[1])
        );
    }
    let cms = ColorPipeline::new(source.icc.as_deref()).unwrap();
    let full_crop = processing::copy_crop(frame, crop).unwrap();
    let working = cms.to_working(&full_crop, 4).unwrap();
    let bytes: Vec<u8> = working
        .iter()
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect();
    assert!(
        pixels
            .rgba
            .iter()
            .zip(bytes)
            .all(|(a, b)| a.abs_diff(b) <= 1)
    );
    let original = ImageDocument::read(&input).unwrap();
    assert!(
        original
            .frame()
            .pixels
            .iter()
            .zip(&source.pixels)
            .all(|(a, b)| a.to_bits() == b.to_bits())
    );
    let id = worker
        .submit(Job::Detail(
            Crop {
                x: usize::MAX,
                ..crop
            },
            parameters,
        ))
        .unwrap();
    assert!(matches!(completed(&worker, id), Event::Failed { .. }));
    let id = worker.submit(Job::Preview(json!({"Strength":0}))).unwrap();
    assert!(matches!(completed(&worker, id), Event::Rendered { .. }));
    let mask = root.join("mask.pcraft");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_natural-skin-companion"))
        .args([
            "process",
            input.to_str().unwrap(),
            mask.to_str().unwrap(),
            r#"{"Show_Mask":true}"#,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let mut actual = ImageDocument::read(&mask)
        .unwrap()
        .frame()
        .icc
        .clone()
        .unwrap();
    let mut expected = moxcms::ColorProfile::new_srgb().encode().unwrap();
    // ICC header creation date is intentionally time-dependent. Compare colour
    // content, not timestamps from two separately generated profiles.
    actual[24..36].fill(0);
    expected[24..36].fill(0);
    assert_eq!(actual, expected);
    drop(worker);
    std::fs::remove_dir_all(root).unwrap();
}
