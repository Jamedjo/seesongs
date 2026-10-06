mod installed;
mod launcher;
mod pace;
#[cfg(test)]
mod screenshots;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use dioxus::desktop::tao::event::Event;
use dioxus::desktop::{
    use_wry_event_handler, window, Config, LogicalSize, WindowBuilder, WindowEvent,
};
use dioxus::prelude::*;
use serde::Deserialize;

const STYLE: &str = include_str!("style.css");

// Tab-separated so titles with pipes or dashes survive intact.
const FORMAT: &str =
    "{{playerName}}\t{{status}}\t{{position}}\t{{mpris:length}}\t{{artist}}\t{{album}}\t{{title}}";

// Lines change slightly early so they're on screen as they're sung.
const LEAD_SECS: f64 = 0.3;

// Lines either side of the current one shrink from the first size to the
// second with distance; how many there are depends on the window height.
const SIDE_MAX_PX: f64 = 14.0;
const SIDE_MIN_PX: f64 = 9.0;
const LINE_HEIGHT: f64 = 1.25;
const LINE_GAP_PX: f64 = 2.0;

// LRCLIB sheds load with 503s; ask again once it has had a moment.
const RETRY_AFTER: Duration = Duration::from_secs(15);

// How far a recording's length may differ before its lyric timings are ignored.
const MATCH_SECS: f64 = 3.0;

#[derive(Clone, PartialEq, Debug)]
struct Track {
    player: String,
    status: String,
    position: f64,
    length: f64,
    artist: String,
    album: String,
    title: String,
}

impl Track {
    fn key(&self) -> (String, String) {
        (self.artist.clone(), self.title.clone())
    }
}

/// Picked from the window height; each has its own class in style.css.
#[derive(Clone, Copy, PartialEq)]
enum Layout {
    Stacked,
    Inline,
    Bar,
}

impl Layout {
    fn for_height(height: f64) -> Self {
        match height {
            h if h < 90.0 => Layout::Bar,
            h if h < 160.0 => Layout::Inline,
            _ => Layout::Stacked,
        }
    }

    fn class(self) -> &'static str {
        match self {
            Layout::Stacked => "stacked",
            Layout::Inline => "inline",
            Layout::Bar => "bar",
        }
    }

    /// Height left for the lyrics: padding, header, progress bar and the gaps
    /// between them, as set in style.css.
    fn lyrics_height(self, height: f64) -> f64 {
        let chrome = match self {
            Layout::Stacked => 26.0 + 36.0 + 3.0 + 2.0 * 8.0,
            Layout::Inline => 16.0 + 26.0 + 2.0 + 2.0 * 6.0,
            Layout::Bar => 6.0 + 20.0 + 2.0 + 2.0 * 2.0,
        };
        height - chrome
    }

    fn current_line_px(self) -> f64 {
        match self {
            Layout::Bar => 14.0,
            _ => 19.0,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
enum Lyrics {
    Loading,
    Synced(Vec<(f64, String)>),
    Plain(Vec<String>),
    Instrumental,
    Missing,
    Unavailable,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrclibTrack {
    synced_lyrics: Option<String>,
    plain_lyrics: Option<String>,
    #[serde(default)]
    instrumental: bool,
    duration: Option<f64>,
}

fn main() {
    installed::run_installer_step();
    installed::keep_up_to_date();
    if let Some(launcher) = launcher::Launcher::from_env() {
        launcher.start();
    }

    let window = WindowBuilder::new()
        .with_title("Now Playing")
        .with_inner_size(LogicalSize::new(460.0, 230.0))
        .with_decorations(false);

    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                .with_window(window)
                .with_menu(None)
                .with_disable_context_menu(true),
        )
        .launch(app);
}

fn app() -> Element {
    let mut track = use_signal(|| None::<Track>);
    let mut lyrics = use_signal(|| Lyrics::Missing);
    let mut height = use_signal(|| {
        let w = window();
        w.inner_size().to_logical::<f64>(w.scale_factor()).height
    });

    use_wry_event_handler(move |event, _| match event {
        Event::WindowEvent {
            event: WindowEvent::Resized(size),
            ..
        } => height.set(size.to_logical::<f64>(window().scale_factor()).height),
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => installed::update_on_close(),
        _ => {}
    });

    use_future(move || async move {
        let mut cache: HashMap<(String, String), Lyrics> = HashMap::new();
        let mut fetched_for = None;
        let mut retry_at = None;

        loop {
            let current = poll_players().await;

            // Artist can arrive after the title, so a changed artist refetches.
            let key = current.as_ref().map(Track::key);
            let retry_due = retry_at.is_some_and(|at| Instant::now() >= at);
            if key != fetched_for || retry_due {
                fetched_for = key.clone();
                retry_at = None;
                match (&current, key) {
                    (Some(t), Some(key)) => match cache.get(&key) {
                        Some(hit) => lyrics.set(hit.clone()),
                        None => {
                            if !retry_due {
                                lyrics.set(Lyrics::Loading);
                            }
                            // Only answers are cached; a failed request is retried.
                            match fetch_lyrics(t).await {
                                Ok(found) => {
                                    cache.insert(key, found.clone());
                                    lyrics.set(found);
                                }
                                Err(_) => {
                                    lyrics.set(Lyrics::Unavailable);
                                    retry_at = Some(Instant::now() + RETRY_AFTER);
                                }
                            }
                        }
                    },
                    _ => lyrics.set(Lyrics::Missing),
                }
            }

            if *track.peek() != current {
                track.set(current);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });

    let Some(t) = track() else {
        return rsx! {
            style { {STYLE} }
            div { class: "card idle", "Nothing playing" }
        };
    };

    let lyrics = lyrics.read();
    card(&t, &lyrics, height())
}

/// The window's contents for a track, laid out for a window `height` tall.
fn card(t: &Track, lyrics: &Lyrics, height: f64) -> Element {
    let progress = if t.length > 0.0 {
        (t.position / t.length * 100.0).min(100.0)
    } else {
        0.0
    };

    let layout = Layout::for_height(height);

    rsx! {
        style { {STYLE} }
        div { class: "card {layout.class()}",
            class: if t.status != "Playing" { "paused" },
            div { class: "head",
                div { class: "meta",
                    span { class: "title", "{t.title}" }
                    if let Some(byline) = byline(t) {
                        span { class: "artist", "{byline}" }
                    }
                }
                button {
                    class: "toggle",
                    onclick: {
                        let player = t.player.clone();
                        move |_| play_pause(player.clone())
                    },
                    svg { view_box: "0 0 24 24",
                        if t.status == "Playing" {
                            path { d: "M7 5h3.5v14H7zM13.5 5H17v14h-3.5z" }
                        } else {
                            path { d: "M8 5v14l11-7z" }
                        }
                    }
                }
            }
            div { class: "progress",
                div { class: "fill", style: "width: {progress}%" }
            }
            {render_lyrics(lyrics, t.position, t.length, side_lines(layout, height))}
        }
    }
}

fn render_lyrics(lyrics: &Lyrics, position: f64, length: f64, side: usize) -> Element {
    match lyrics {
        Lyrics::Synced(lines) => {
            let now = lines
                .iter()
                .rposition(|(at, _)| *at <= position + LEAD_SECS);
            let text: Vec<&str> = lines.iter().map(|(_, line)| line.as_str()).collect();
            render_lines(&text, now, side)
        }
        // No timings: lines spread evenly over the track, so only roughly in step.
        Lyrics::Plain(lines) => {
            let at = if length > 0.0 { position / length } else { 0.0 };
            let now = ((at * lines.len() as f64) as usize).min(lines.len().saturating_sub(1));
            let text: Vec<&str> = lines.iter().map(String::as_str).collect();
            render_lines(&text, Some(now), side)
        }
        Lyrics::Loading => rsx! { div { class: "lyrics note", "Finding lyrics…" } },
        Lyrics::Instrumental => rsx! { div { class: "lyrics note", "Instrumental" } },
        Lyrics::Missing => rsx! { div { class: "lyrics note", "No lyrics found" } },
        Lyrics::Unavailable => {
            rsx! { div { class: "lyrics note", "Lyrics service busy, retrying…" } }
        }
    }
}

/// How many lines fit either side of the current one, at their average size.
fn side_lines(layout: Layout, height: f64) -> usize {
    let current = layout.current_line_px() * LINE_HEIGHT;
    let half = (layout.lyrics_height(height) - current) / 2.0;
    let average = (SIDE_MAX_PX + SIDE_MIN_PX) / 2.0 * LINE_HEIGHT + LINE_GAP_PX;
    (half / average).max(0.0) as usize
}

/// The current line with up to `side` lines either side. Room one side can't
/// use, near the start or end of a song, goes to the other. Each side shrinks
/// and fades over its own lines. `now` is None before the first line starts.
fn render_lines(lines: &[&str], now: Option<usize>, side: usize) -> Element {
    // Before the first line, the first one is shown as coming next.
    let centre = now.map_or(-1, |i| i as isize);
    let behind = centre.max(0) as usize;
    let ahead = (lines.len() as isize - centre - 1).max(0) as usize;
    let mut before = behind.min(side);
    let after = ahead.min(2 * side - before);
    before = behind.min(2 * side - after);

    rsx! {
        div { class: "lyrics",
            for offset in -(before as isize)..=after as isize {
                {
                    let text = usize::try_from(centre + offset)
                        .ok()
                        .and_then(|i| lines.get(i).copied())
                        .unwrap_or_default();
                    let count = if offset < 0 { before } else { after };
                    let d = offset.unsigned_abs() as f64;
                    let along = (d - 1.0) / (count.max(2) - 1) as f64;
                    let size = SIDE_MAX_PX - (SIDE_MAX_PX - SIDE_MIN_PX) * along;
                    let mut fade = 1.0 - 0.85 * d / (count + 1) as f64;
                    if offset < 0 {
                        fade *= 0.7;
                    }
                    if offset == 0 {
                        rsx! { div { key: "{offset}", class: "line now", "{text}" } }
                    } else {
                        rsx! {
                            div {
                                key: "{offset}",
                                class: "line",
                                style: "font-size: {size:.1}px; opacity: {fade:.2}",
                                "{text}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The system's playerctl, without the AppImage's libraries, which can be
/// older than the ones it was built against.
fn playerctl() -> tokio::process::Command {
    let mut command = tokio::process::Command::new("playerctl");
    if std::env::var_os("APPIMAGE").is_some() {
        command.env_remove("LD_LIBRARY_PATH");
    }
    command
}

/// Targets the player on screen, not playerctl's default pick.
fn play_pause(player: String) {
    spawn(async move {
        let _ = playerctl()
            .args(["-p", &player, "play-pause"])
            .status()
            .await;
    });
}

/// The playing player wins; failing that, a paused one, so the window holds
/// its place through a pause.
async fn poll_players() -> Option<Track> {
    let out = playerctl()
        .args(["-a", "metadata", "--format", FORMAT])
        .output()
        .await
        .ok()?;
    let tracks: Vec<Track> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(parse_track)
        .collect();

    tracks
        .iter()
        .find(|t| t.status == "Playing")
        .or_else(|| tracks.iter().find(|t| t.status == "Paused"))
        .cloned()
}

fn parse_track(line: &str) -> Option<Track> {
    let mut f = line.split('\t');
    let micros = |s: Option<&str>| s.and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0) / 1e6;
    let track = Track {
        player: f.next()?.to_string(),
        status: f.next()?.to_string(),
        position: micros(f.next()),
        length: micros(f.next()),
        artist: f.next()?.trim().to_string(),
        album: f.next()?.trim().to_string(),
        title: f.next()?.trim().to_string(),
    };
    (!track.title.is_empty()).then_some(track)
}

/// Err means LRCLIB couldn't answer (busy, offline) and it's worth asking again;
/// Ok(Missing) means it answered and has nothing.
async fn fetch_lyrics(t: &Track) -> reqwest::Result<Lyrics> {
    // Sites that leave the artist blank often put "Artist - Title" in the title.
    let (artist, title) = match t.title.split_once(" - ") {
        Some((a, s)) if t.artist.is_empty() => (a.trim(), s.trim()),
        _ => (t.artist.as_str(), t.title.as_str()),
    };

    let client = reqwest::Client::builder()
        .user_agent("seesongs (https://github.com/jamedjo/seesongs)")
        .timeout(Duration::from_secs(8))
        .build()
        .expect("reqwest client");

    let search_query = if artist.is_empty() {
        vec![("q", title.to_string())]
    } else {
        vec![
            ("artist_name", artist.to_string()),
            ("track_name", title.to_string()),
        ]
    };

    let mut found = None;
    let mut timed = None;
    if !artist.is_empty() {
        let mut query = vec![
            ("artist_name", artist.to_string()),
            ("track_name", title.to_string()),
        ];
        if t.length > 0.0 {
            query.push(("duration", format!("{:.0}", t.length)));
        }
        found = lrclib(&client, "get", &query).await?;
    }
    if found.is_none() {
        let mut hits: Vec<LrclibTrack> = lrclib(&client, "search", &search_query)
            .await?
            .unwrap_or_default();
        timed = Some(timed_recordings(&hits));
        // Search returns every live recording too. Their timings only fit a
        // recording of the same length, so any other match keeps just its words.
        let same_length = |h: &LrclibTrack| {
            h.duration
                .is_some_and(|d| (d - t.length).abs() <= MATCH_SECS)
        };
        let has_words = |h: &LrclibTrack| h.synced_lyrics.is_some() || h.plain_lyrics.is_some();
        found = hits
            .iter()
            .position(|h| h.synced_lyrics.is_some() && same_length(h))
            .or_else(|| hits.iter().position(has_words))
            .or_else(|| hits.iter().position(|h| h.instrumental))
            .map(|i| hits.swap_remove(i));
        if let Some(hit) = found.as_mut().filter(|h| !same_length(h)) {
            if let Some(lrc) = hit.synced_lyrics.take() {
                hit.plain_lyrics.get_or_insert_with(|| {
                    parse_lrc(&lrc)
                        .into_iter()
                        .map(|(_, line)| line)
                        .collect::<Vec<_>>()
                        .join("\n")
                });
            }
        }
    }

    let lyrics = match found {
        Some(LrclibTrack {
            synced_lyrics: Some(lrc),
            ..
        }) => Lyrics::Synced(parse_lrc(&lrc)),
        Some(LrclibTrack {
            plain_lyrics: Some(text),
            ..
        }) => Lyrics::Plain(
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect(),
        ),
        Some(hit) if hit.instrumental => Lyrics::Instrumental,
        _ => Lyrics::Missing,
    };

    // Words without timings: pace them from timed recordings of other lengths.
    // A failed search here only costs the pacing, so it isn't an error.
    if let Lyrics::Plain(lines) = &lyrics {
        let timed = match timed {
            Some(timed) => timed,
            None => lrclib::<Vec<LrclibTrack>>(&client, "search", &search_query)
                .await
                .ok()
                .flatten()
                .map(|hits| timed_recordings(&hits))
                .unwrap_or_default(),
        };
        if let Some(paced) = pace::pace(lines, &timed, t.length) {
            return Ok(Lyrics::Synced(paced));
        }
    }
    Ok(lyrics)
}

fn timed_recordings(hits: &[LrclibTrack]) -> Vec<(Vec<(f64, String)>, f64)> {
    hits.iter()
        .filter_map(|h| Some((parse_lrc(h.synced_lyrics.as_ref()?), h.duration?)))
        .collect()
}

/// None when LRCLIB has no such track (404); errors for everything else.
async fn lrclib<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    endpoint: &str,
    query: &[(&str, String)],
) -> reqwest::Result<Option<T>> {
    let response = client
        .get(format!("https://lrclib.net/api/{endpoint}"))
        .query(query)
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    response.error_for_status()?.json().await.map(Some)
}

/// "[01:23.45]text"; a line may carry several timestamps when it repeats.
fn parse_lrc(lrc: &str) -> Vec<(f64, String)> {
    let mut lines = Vec::new();
    for raw in lrc.lines() {
        let mut rest = raw;
        let mut stamps = Vec::new();
        while let Some(inner) = rest.strip_prefix('[') {
            let Some((stamp, after)) = inner.split_once(']') else {
                break;
            };
            let Some((m, s)) = stamp.split_once(':') else {
                break;
            };
            let (Ok(m), Ok(s)) = (m.parse::<f64>(), s.parse::<f64>()) else {
                break;
            };
            stamps.push(m * 60.0 + s);
            rest = after;
        }
        for at in stamps {
            lines.push((at, rest.trim().to_string()));
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    lines
}

/// "Artist · Album", whichever parts the player filled in.
fn byline(t: &Track) -> Option<String> {
    let parts: Vec<&str> = [t.artist.as_str(), t.album.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}
