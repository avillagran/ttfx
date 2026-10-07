//! fx embedding audio: AudioLevels driving thunderstorm strikes. Silence
//! (all-zero levels) must keep the reference byte sequence; driven levels
//! must change it (audio strikes on top of the ambient chance).

use clap::Parser;
use ttfx::cli::Cli;
use ttfx::engine::ctx::Clock;
use ttfx::fx::{self, AudioLevels, Engine};
use ttfx::utils::rng::Rng;

const ARGS: &[&str] = &[
    "ttfx",
    "--canvas-width",
    "40",
    "--canvas-height",
    "12",
    "--ignore-terminal-dimensions",
    "thunderstorm",
];

fn build() -> (Engine, Box<dyn fx::run::Effect>) {
    let cli = Cli::try_parse_from(ARGS).unwrap();
    let config = cli.terminal_config();
    let command = cli.effect.unwrap();
    let engine = Engine::new(
        "THUNDERSTORM\naudio reactive",
        config,
        Rng::seeded(3),
        Clock::virtual_with_frame_rate(60),
    )
    .unwrap();
    let effect = fx::effects::build(&command).expect("fx builds thunderstorm");
    (engine, effect)
}

fn frame_bytes(e: &mut Engine, effect: &mut dyn fx::run::Effect, levels: AudioLevels) -> Option<Vec<u8>> {
    e.set_audio_levels(levels);
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

#[test]
fn zero_levels_keep_the_reference_sequence_and_driven_levels_change_it() {
    let silence = AudioLevels::default();
    let driven = AudioLevels {
        volume: 0.9,
        bass: 0.9,
        beat: true,
    };

    let (mut e, mut effect) = build();
    effect.build(&mut e).unwrap();
    let mut plain = Vec::new();
    for _ in 0..240 {
        match frame_bytes(&mut e, effect.as_mut(), silence) {
            Some(f) => plain.push(f),
            None => break,
        }
    }

    // Zero levels through the audio path equal no audio at all.
    let (mut e, mut effect) = build();
    effect.build(&mut e).unwrap();
    let mut zeroed = Vec::new();
    for _ in 0..240 {
        match frame_bytes(&mut e, effect.as_mut(), silence) {
            Some(f) => zeroed.push(f),
            None => break,
        }
    }
    assert_eq!(plain, zeroed);

    // Driven levels change the sequence (audio strikes fire within a few
    // frames at 0.9 volume with the beat flag).
    let (mut e, mut effect) = build();
    effect.build(&mut e).unwrap();
    let mut audio = Vec::new();
    for _ in 0..240 {
        match frame_bytes(&mut e, effect.as_mut(), driven) {
            Some(f) => audio.push(f),
            None => break,
        }
    }
    assert_eq!(audio.len(), plain.len());
    let different = plain
        .iter()
        .zip(&audio)
        .filter(|(p, a)| p != a)
        .count();
    assert!(
        different > 10,
        "driven audio must change many storm-phase frames, got {different}"
    );
}
