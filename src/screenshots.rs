//! README screenshots: the window rendered at a chosen moment of Sway's
//! "This Is My Demo", written as HTML for screenshots/shoot.sh to capture.
//! The lyrics are fetched by that script, not kept here.
//!
//!     SEESONGS_LRC=lyrics.lrc cargo test --bin seesongs -- --ignored render_screenshots

use dioxus::prelude::*;

use crate::{card, parse_lrc, Lyrics, Track};

/// Name, width, height, and the line being sung in that shot.
const SHOTS: [(&str, u32, u32, &str); 4] = [
    ("window", 460, 230, "This is my demo"),
    ("tall", 360, 330, "This is my dream"),
    ("inline", 640, 120, "Welcome to the end of the beginning"),
    ("bar", 350, 50, "My name is Sway"),
];

#[derive(Props, Clone, PartialEq)]
struct ShotProps {
    track: Track,
    lyrics: Lyrics,
    height: f64,
}

#[allow(non_snake_case)]
fn Shot(props: ShotProps) -> Element {
    card(&props.track, &props.lyrics, props.height)
}

#[test]
#[ignore]
fn render_screenshots() {
    let path = std::env::var("SEESONGS_LRC").expect("SEESONGS_LRC names the lyrics file");
    let lines = parse_lrc(&std::fs::read_to_string(path).unwrap());
    let track = Track {
        player: "demo".into(),
        status: "Playing".into(),
        position: 0.0,
        length: 361.0,
        artist: "Sway".into(),
        album: "This Is My Demo".into(),
        title: "This Is My Demo".into(),
    };

    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/target/screenshots");
    std::fs::create_dir_all(dir).unwrap();
    for (name, width, height, sung) in SHOTS {
        let (at, _) = lines
            .iter()
            .find(|(_, line)| line == sung)
            .unwrap_or_else(|| panic!("no line {sung:?}"));
        let props = ShotProps {
            track: Track {
                position: at + 0.5,
                ..track.clone()
            },
            lyrics: Lyrics::Synced(lines.clone()),
            height: height as f64,
        };
        let mut dom = VirtualDom::new_with_props(Shot, props);
        dom.rebuild_in_place();
        let html = format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"></head><body>{}</body></html>",
            dioxus_ssr::render(&dom)
        );
        std::fs::write(format!("{dir}/{name}-{width}x{height}.html"), html).unwrap();
    }
}
