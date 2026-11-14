//! The ABI's layouts, which are its version 1: a change fails here first.
//! And the SDK's own: descriptors as the host reads them, images, and
//! running a node in memory.

use std::mem::{align_of, offset_of, size_of};
use std::sync::Mutex;

use oscope_ext_sdk::abi::{self, Extension, Host, ImageMut, ImageRef, Str};
use oscope_ext_sdk::{
    Descriptor, Image, Outputs, OwnedImage, ProcessingNode, run_in_memory, serde_json,
};
use serde_json::json;

#[test]
fn the_abi_s_layouts_are_version_1_s() {
    assert_eq!(abi::ABI_VERSION, 1);
    assert_eq!(abi::ENTRY, b"oscope_extension\0");
    assert_eq!((abi::OK, abi::FAILED, abi::PANICKED), (0, 1, 2));
    // 64-bit pointers: OScope's platforms are x86_64 (ADR-0003).
    assert_eq!((size_of::<Str>(), align_of::<Str>()), (16, 8));
    assert_eq!((offset_of!(Str, data), offset_of!(Str, len)), (0, 8));
    for (size, align, offsets) in [
        (
            size_of::<ImageRef>(),
            align_of::<ImageRef>(),
            [
                offset_of!(ImageRef, data),
                offset_of!(ImageRef, width),
                offset_of!(ImageRef, height),
                offset_of!(ImageRef, channels),
            ],
        ),
        (
            size_of::<ImageMut>(),
            align_of::<ImageMut>(),
            [
                offset_of!(ImageMut, data),
                offset_of!(ImageMut, width),
                offset_of!(ImageMut, height),
                offset_of!(ImageMut, channels),
            ],
        ),
    ] {
        assert_eq!((size, align, offsets), (24, 8, [0, 8, 12, 16]));
    }
    assert_eq!((size_of::<Host>(), align_of::<Host>()), (32, 8));
    let host = [
        offset_of!(Host, abi_version),
        offset_of!(Host, describe),
        offset_of!(Host, output),
        offset_of!(Host, fail),
    ];
    assert_eq!(host, [0, 8, 16, 24]);
    assert_eq!((size_of::<Extension>(), align_of::<Extension>()), (24, 8));
    let extension = [
        offset_of!(Extension, abi_version),
        offset_of!(Extension, describe),
        offset_of!(Extension, run),
    ];
    assert_eq!(extension, [0, 8, 16]);
}

#[test]
fn a_descriptor_reads_as_the_pipeline_s() {
    let schema = json!({ "type": "object", "properties": { "k": { "type": "number" } } });
    let descriptor = Descriptor::new("example.mix", 2, "example", "Mixes two images.")
        .input("a")
        .input("b")
        .output("mixed")
        .params(schema.clone())
        .live()
        .tileable(3);
    let expected = json!({
        "id": "example.mix",
        "version": 2,
        "category": "example",
        "description": "Mixes two images.",
        "inputs": [{ "name": "a", "type": "image" }, { "name": "b", "type": "image" }],
        "outputs": [{ "name": "mixed", "type": "image" }],
        "params": schema,
        "capabilities": { "cpu": true, "cuda": false, "live": true, "tileable": true, "halo": 3 },
    });
    assert_eq!(descriptor.to_json(), expected);
    let plain = Descriptor::new("example.plain", 1, "example", "").to_json();
    assert_eq!(plain["capabilities"]["tileable"], false);
    assert_eq!(
        plain["params"],
        json!({ "type": "object", "additionalProperties": false })
    );
}

#[test]
fn an_image_holds_exactly_its_samples_and_repeats_its_edges() {
    let data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let image = Image::new(3, 2, 1, &data).unwrap();
    assert_eq!(
        (image.clamped(-5, 0, 0), image.clamped(9, 9, 0)),
        (1.0, 6.0)
    );
    assert_eq!(image.clamped(1, 1, 0), 5.0);
    let error = Image::new(3, 2, 2, &data).unwrap_err();
    assert_eq!(error, "3 × 2 × 2 samples, but 6 given");
}

/// Sums its two inputs into its first output, and makes the second only
/// when asked; or misbehaves as `params["misbehave"]` says.
struct Sum;

impl ProcessingNode for Sum {
    fn descriptor(&self) -> Descriptor {
        Descriptor::new("test.sum", 1, "test", "")
            .input("a")
            .input("b")
            .output("sum")
            .output("extra")
    }

    fn run(
        &self,
        params: &serde_json::Value,
        inputs: &[Image<'_>],
        outputs: &mut Outputs<'_>,
    ) -> Result<(), String> {
        let (a, b) = (inputs[0], inputs[1]);
        let sum = outputs.image(0, a.width(), a.height(), a.channels())?;
        match params["misbehave"].as_str() {
            Some("twice") => return outputs.image(0, 1, 1, 1).map(drop),
            Some("beyond") => return outputs.image(2, 1, 1, 1).map(drop),
            Some("fail") => return Err("asked to fail".to_owned()),
            _ => {}
        }
        let extra = outputs.image(1, 1, 1, 1)?;
        for ((sum, a), b) in sum.iter_mut().zip(a.data()).zip(b.data()) {
            *sum = a + b;
        }
        extra[0] = 7.0;
        Ok(())
    }
}

#[test]
fn a_node_runs_in_memory_for_its_tests() {
    let (a, b) = ([1.0, 2.0, 3.0, 4.0], [10.0, 20.0, 30.0, 40.0]);
    let inputs = [
        Image::new(2, 2, 1, &a).unwrap(),
        Image::new(2, 2, 1, &b).unwrap(),
    ];
    let made = run_in_memory(&Sum, &json!({}), &inputs).unwrap();
    let sum = OwnedImage {
        width: 2,
        height: 2,
        channels: 1,
        data: vec![11.0, 22.0, 33.0, 44.0],
    };
    let extra = OwnedImage {
        width: 1,
        height: 1,
        channels: 1,
        data: vec![7.0],
    };
    assert_eq!(made, [sum, extra]);
    for (misbehave, said) in [
        ("twice", "output 0 can't be made"),
        ("beyond", "output 2 can't be made"),
        ("fail", "asked to fail"),
    ] {
        let error = run_in_memory(&Sum, &json!({ "misbehave": misbehave }), &inputs).unwrap_err();
        assert!(error.starts_with(said), "{misbehave}: {error}");
    }
}

/// Panics when run.
struct Panics;

impl ProcessingNode for Panics {
    fn descriptor(&self) -> Descriptor {
        Descriptor::new("test.panics", 1, "test", "").output("image")
    }

    fn run(
        &self,
        _: &serde_json::Value,
        _: &[Image<'_>],
        _: &mut Outputs<'_>,
    ) -> Result<(), String> {
        panic!("boom, at sample {}", 3)
    }
}

oscope_ext_sdk::export!(Sum, Panics);

/// What the extension last told the host, through `describe` or `fail`.
static TOLD: Mutex<String> = Mutex::new(String::new());

extern "C" fn told(_: *mut abi::Call, text: Str) {
    // SAFETY: the extension lends the text for the call.
    let bytes = unsafe { std::slice::from_raw_parts(text.data, text.len) };
    *TOLD.lock().unwrap() = String::from_utf8_lossy(bytes).into_owned();
}

extern "C" fn no_output(
    _: *mut abi::Call,
    _: u32,
    width: u32,
    height: u32,
    channels: u32,
) -> ImageMut {
    let data = std::ptr::null_mut();
    ImageMut {
        data,
        width,
        height,
        channels,
    }
}

#[test]
fn the_glue_refuses_another_host_and_catches_panics() {
    // SAFETY: `export!`'s table lives as long as the program.
    let table = unsafe { &*oscope_extension() };
    assert_eq!(table.abi_version, abi::ABI_VERSION);
    let host = |abi_version| Host {
        abi_version,
        describe: told,
        output: no_output,
        fail: told,
    };
    let call = std::ptr::null_mut();
    let other = host(abi::ABI_VERSION + 1);
    assert_eq!((table.describe)(&other, call), abi::FAILED);
    let said = TOLD.lock().unwrap().clone();
    assert_eq!(
        said,
        "the host speaks the extension ABI 2, and this extension 1"
    );

    let ours = host(abi::ABI_VERSION);
    assert_eq!((table.describe)(&ours, call), abi::OK);
    let described: serde_json::Value = serde_json::from_str(&TOLD.lock().unwrap()).unwrap();
    assert_eq!(
        (described[0]["id"].as_str(), described[1]["id"].as_str()),
        (Some("test.sum"), Some("test.panics"))
    );
    let (params, none) = (Str::new("{}"), std::ptr::null());
    assert_eq!((table.run)(&ours, call, 1, params, none, 0), abi::PANICKED);
    assert!(
        TOLD.lock().unwrap().contains("boom, at sample 3"),
        "{}",
        TOLD.lock().unwrap()
    );
    assert_eq!((table.run)(&ours, call, 9, params, none, 0), abi::FAILED);
    assert_eq!(*TOLD.lock().unwrap(), "there is no such node");
}
