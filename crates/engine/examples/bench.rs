//! How fast the engine renders a set, and what each part of it costs.
//! `cargo run --release -p mjolnir-engine --example bench -- ui/public/demos/fog.mjolnir`
use std::time::Instant;

use mjolnir_engine::controller::Controller;
use mjolnir_engine::engine::Engine;
use mjolnir_engine::export;

fn main() {
    for path in std::env::args().skip(1) {
        let json = std::fs::read_to_string(&path).expect("a set file");
        let (_engine, endpoints) = Engine::new(48000.0);
        let mut c = Controller::new(endpoints, 48000.0);
        c.open_json(&json).expect("a valid set");
        let bars = c.project.song.length_bars();
        let seconds = bars as f32 * 4.0 * 60.0 / c.project.bpm;
        let t = Instant::now();
        let mix = export::render(&c.project, c.samples(), 48000, None, 2.0);
        let all = t.elapsed().as_secs_f32();
        print!(
            "{:28} {:5.1} s of song: all {:5.1}x realtime;",
            path.rsplit('/').next().unwrap(),
            seconds,
            seconds / all
        );
        for (i, track) in c.project.tracks.iter().enumerate() {
            let t = Instant::now();
            let _ = export::render(&c.project, c.samples(), 48000, Some(i), 2.0);
            print!(
                " {} {:.0}%",
                track.name.to_lowercase(),
                100.0 * t.elapsed().as_secs_f32() / all
            );
        }
        println!(" ({} samples)", mix.len() / 2);
    }
}
