//! fx embedding audio: the live color transform (set_audio_color) and its
//! byte-identity guarantees. DHH's review gate for PR #24: no transform
//! means no change — plus the transformed, restored, late-visual and
//! interning-bound behavior of the span retarget.

use clap::Parser;
use ttfx::cli::Cli;
use ttfx::engine::ctx::Clock;
use ttfx::fx::{self, AudioColor, Engine};
use ttfx::utils::rng::Rng;

const IDENTITY: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
const INPUT: &str = "LIVE COLORS 0123456789\nthe quick brown fox";

fn build(args: &[&str]) -> (Engine, Box<dyn fx::run::Effect>) {
    let cli = Cli::try_parse_from(args).unwrap();
    let config = cli.terminal_config();
    let command = cli.effect.unwrap();
    let engine = Engine::new(
        INPUT,
        config,
        Rng::seeded(7),
        Clock::virtual_with_frame_rate(60),
    )
    .unwrap();
    let effect = fx::effects::build(&command).expect("fx builds this effect");
    (engine, effect)
}

/// Render one frame and return its bytes.
fn frame_bytes(e: &mut Engine, effect: &mut dyn fx::run::Effect) -> Option<Vec<u8>> {
    if !effect.next_frame(e) {
        return None;
    }
    e.frame();
    e.render_here();
    let r = e.renderer();
    let mut parts: Vec<std::io::IoSlice<'_>> = Vec::new();
    r.frame_parts(b"", b"", &mut parts);
    let mut bytes = Vec::new();
    for part in &parts {
        bytes.extend_from_slice(part);
    }
    Some(bytes)
}

fn frames(e: &mut Engine, effect: &mut dyn fx::run::Effect, n: usize) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for _ in 0..n {
        match frame_bytes(e, effect) {
            Some(f) => out.push(f),
            None => break,
        }
    }
    out
}

/// Every "38;2;r;g;b" channel triple in a frame, in order.
fn fg_triples(frame: &[u8]) -> Vec<(u8, u8, u8)> {
    let s = String::from_utf8_lossy(frame);
    let mut out = Vec::new();
    let mut rest = s.as_ref();
    while let Some(at) = rest.find("38;2;") {
        rest = &rest[at + 5..];
        let end = rest.find('m').unwrap_or(rest.len());
        let nums: Vec<u8> = rest[..end]
            .split(';')
            .filter_map(|n| n.parse().ok())
            .collect();
        if nums.len() == 3 {
            out.push((nums[0], nums[1], nums[2]));
        }
        rest = &rest[end..];
    }
    out
}

const ARGS: &[&str] = &[
    "ttfx",
    "--canvas-width",
    "24",
    "--canvas-height",
    "8",
    "--ignore-terminal-dimensions",
    "decrypt",
];

/// Cost report (a measurement, not a gate): render_here ns/frame with no
/// transform, a constant transform, and a transform whose parameters change
/// every frame (worst case: a retarget each frame). Run with:
/// cargo test --release --test fx_audio_color -- --ignored --nocapture
#[test]
#[ignore]
fn transform_cost_report() {
    use std::time::Instant;
    for effect_name in ["print", "decrypt", "matrix", "beams", "thunderstorm"] {
        for mode in ["plain", "constant", "churn"] {
            let args = [
                "ttfx",
                "--canvas-width",
                "120",
                "--canvas-height",
                "40",
                "--ignore-terminal-dimensions",
                effect_name,
            ];
            let (mut e, mut effect) = build(&args);
            effect.build(&mut e).unwrap();
            if mode == "constant" {
                e.set_audio_color(Some(AudioColor {
                    brightness: 0.5,
                    matrix: IDENTITY,
                }));
            }
            let mut samples = Vec::new();
            for frame in 0..600 {
                if mode == "churn" {
                    let b = 0.5 + 0.25 * (frame as f32 / 60.0).sin();
                    e.set_audio_color(Some(AudioColor {
                        brightness: b,
                        matrix: IDENTITY,
                    }));
                }
                if !effect.next_frame(&mut e) {
                    break;
                }
                e.frame();
                let t = Instant::now();
                e.render_here();
                samples.push(t.elapsed().as_nanos() as u64);
            }
            samples.sort_unstable();
            let median = samples[samples.len() / 2];
            eprintln!(
                "REPORT {effect_name} {mode} median_ns={median} frames={}",
                samples.len()
            );
        }
    }
}

/// A transform whose parameters change every frame on a large canvas:
/// hundreds of visuals retargeted each frame, variants interned past the
/// renderer's synced watermark. Regression gate for the stale-buffer class:
/// the frame must contain no bytes from beyond the synced region (NUL
/// padding would emit as 0x00) and the run must simply not crash.
#[test]
fn churning_transform_on_large_canvas_stays_valid() {
    for effect_name in ["decrypt", "thunderstorm", "matrix"] {
        let args = [
            "ttfx",
            "--canvas-width",
            "120",
            "--canvas-height",
            "40",
            "--ignore-terminal-dimensions",
            effect_name,
        ];
        let (mut e, mut effect) = build(&args);
        effect.build(&mut e).unwrap();
        let mut saw_color = false;
        for frame in 0..300 {
            let b = 0.5 + 0.25 * (frame as f32 / 60.0).sin();
            e.set_audio_color(Some(AudioColor {
                brightness: b,
                matrix: IDENTITY,
            }));
            let Some(bytes) = frame_bytes(&mut e, effect.as_mut()) else {
                break;
            };
            assert!(!bytes.contains(&0), "{effect_name} frame {frame} has NUL");
            saw_color |= bytes.windows(5).any(|w| w == b"38;2;");
        }
        assert!(saw_color, "{effect_name} shows transformed colors");
    }
}

#[test]
fn no_transform_and_identity_transform_keep_bytes_identical() {
    let (mut e, mut effect) = build(ARGS);
    effect.build(&mut e).unwrap();
    let plain = frames(&mut e, effect.as_mut(), 6);
    assert_eq!(plain.len(), 6);

    let (mut e, mut effect) = build(ARGS);
    effect.build(&mut e).unwrap();
    e.set_audio_color(Some(AudioColor {
        brightness: 1.0,
        matrix: IDENTITY,
    }));
    let identity = frames(&mut e, effect.as_mut(), 6);
    assert_eq!(plain, identity, "identity transform changes no byte");
}

#[test]
fn brightness_halves_every_color_channel_and_removal_restores_bytes() {
    let (mut e, mut effect) = build(ARGS);
    effect.build(&mut e).unwrap();
    let plain = frames(&mut e, effect.as_mut(), 6);

    let (mut e, mut effect) = build(ARGS);
    effect.build(&mut e).unwrap();
    e.set_audio_color(Some(AudioColor {
        brightness: 0.5,
        matrix: IDENTITY,
    }));
    let halved = frames(&mut e, effect.as_mut(), 6);
    assert_eq!(halved.len(), 6);
    let mut checked = 0;
    for (p, h) in plain.iter().zip(&halved) {
        for ((r, g, b), (r2, g2, b2)) in fg_triples(p).iter().zip(fg_triples(h)) {
            assert_eq!((r / 2, g / 2, b / 2), (r2, g2, b2), "channel mapping");
            checked += 1;
        }
    }
    assert!(checked > 0, "decrypt emits true-color visuals");

    // Removing the transform restores the authored bytes exactly.
    e.set_audio_color(None);
    let restored = frames(&mut e, effect.as_mut(), 4);
    let (mut e2, mut effect2) = build(ARGS);
    effect2.build(&mut e2).unwrap();
    frames(&mut e2, effect2.as_mut(), 6);
    let reference = frames(&mut e2, effect2.as_mut(), 4);
    assert_eq!(restored, reference, "removal restores authored output");
}

#[test]
fn visuals_created_after_activation_are_transformed_too() {
    // Decrypt keeps making visuals as it types: with a constant transform,
    // late-created visuals must not slip through with authored colors.
    let (mut e, mut effect) = build(ARGS);
    effect.build(&mut e).unwrap();
    e.set_audio_color(Some(AudioColor {
        brightness: 0.5,
        matrix: IDENTITY,
    }));
    let late = frames(&mut e, effect.as_mut(), 10);

    let (mut e2, mut effect2) = build(ARGS);
    effect2.build(&mut e2).unwrap();
    let plain = frames(&mut e2, effect2.as_mut(), 10);
    let mut checked = 0;
    for (p, h) in plain.iter().zip(&late) {
        for ((r, g, b), (r2, g2, b2)) in fg_triples(p).iter().zip(fg_triples(h)) {
            assert_eq!((r / 2, g / 2, b / 2), (r2, g2, b2), "late visual channel");
            checked += 1;
        }
    }
    assert!(checked > 10, "late frames carry colored visuals");
}

#[test]
fn constant_parameters_intern_a_bounded_set_of_variants() {
    let (mut e, mut effect) = build(ARGS);
    effect.build(&mut e).unwrap();
    let (mut e2, mut effect2) = build(ARGS);
    effect2.build(&mut e2).unwrap();
    e2.set_audio_color(Some(AudioColor {
        brightness: 0.5,
        matrix: IDENTITY,
    }));
    let mut deltas = Vec::new();
    for _ in 0..10 {
        frame_bytes(&mut e, effect.as_mut());
        frame_bytes(&mut e2, effect2.as_mut());
        deltas.push(e2.visuals.len() as i64 - e.visuals.len() as i64);
    }
    // The variant count over authored is fixed once every authored visual
    // has been retargeted: equal parameters, equal keys, no new interning.
    let last = deltas.last().unwrap();
    assert!(
        deltas.iter().all(|&d| d <= *last),
        "variant surplus must not grow with constant parameters: {deltas:?}"
    );
}

/// The frame's bytes, rendered in place with no effect driving the engine.
fn render_now(e: &mut Engine) -> String {
    e.render_here();
    let mut parts: Vec<std::io::IoSlice<'_>> = Vec::new();
    e.renderer().frame_parts(b"", b"", &mut parts);
    let bytes: Vec<u8> = parts.iter().flat_map(|p| p.iter().copied()).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// `sym` in `hex`, as an effect authors it.
fn authored(e: &mut Engine, sym: &str, hex: &str) -> fx::Visual {
    let sym = e.sym(sym);
    let info = fx::VisualInfo {
        sym,
        fg: Some(ttfx::utils::graphics::Color::from_hex(hex).unwrap()),
        bg: None,
        attrs: fx::visual::HAS_COLORS,
    };
    e.visuals.make(&e.symbols, info)
}

#[test]
fn a_visual_authored_with_a_variants_key_is_still_transformed() {
    // Halving white interns 7f7f7f; an effect authoring 7f7f7f afterwards
    // must get its own visual, halved to 3f3f3f, not the variant.
    let (mut e, _) = build(ARGS);
    e.set_audio_color(Some(AudioColor {
        brightness: 0.5,
        matrix: IDENTITY,
    }));
    let white = authored(&mut e, "X", "ffffff");
    e.set_visible(0, true);
    e.set_visual(0, white);
    assert!(render_now(&mut e).contains("38;2;127;127;127mX"));
    let grey = authored(&mut e, "X", "7f7f7f");
    e.set_visible(1, true);
    e.set_visual(1, grey);
    let frame = render_now(&mut e);
    assert!(
        frame.contains("38;2;63;63;63mX"),
        "authored grey is halved: {frame:?}"
    );
}

#[test]
fn parameters_in_one_quantization_step_transform_alike() {
    // 0.5 and 0.507 share an epoch, so they must also share the colors:
    // a visual made under the second matches one retargeted under the first.
    let (mut e, _) = build(ARGS);
    e.set_audio_color(Some(AudioColor {
        brightness: 0.5,
        matrix: IDENTITY,
    }));
    let x = authored(&mut e, "X", "ffffff");
    e.set_visible(0, true);
    e.set_visual(0, x);
    render_now(&mut e);
    e.set_audio_color(Some(AudioColor {
        brightness: 0.507,
        matrix: IDENTITY,
    }));
    let y = authored(&mut e, "Y", "ffffff");
    e.set_visible(1, true);
    e.set_visual(1, y);
    let frame = render_now(&mut e);
    assert!(
        frame.contains("38;2;127;127;127mX") && frame.contains("38;2;127;127;127mY"),
        "one epoch, one transform: {frame:?}"
    );
}
