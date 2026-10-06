# seesongs

A small floating window showing the song that's playing, with its lyrics in
time.

![The window with a song playing](screenshots/window.png)

It reads the current track from any MPRIS player (Spotify, browsers, mpv and
most Linux music apps) and fetches lyrics from [LRCLIB](https://lrclib.net).

## Sizes

The layout follows the window. Taller windows show more of the song, fading
with distance from the current line:

<img src="screenshots/tall.png" alt="A taller window showing more of the song" width="360">

Short windows put the title, artist and album on one row:

![A wide, short window](screenshots/inline.png)

Down to a 50px bar with only the current line:

<img src="screenshots/bar.png" alt="A 350 by 50 bar" width="350">

## Lyrics

- Timed lyrics from LRCLIB are used when they match the track's length.
- Timed lyrics from other recordings, usually live ones, are not used directly.
- Lyrics without timings are paced from timed recordings of other lengths:
  the gaps between sung lines are copied, and the breaks between sections
  stretch or shrink to fit the track.
- Failing that, lines are spread evenly over the track.
- Tracks marked instrumental say so.

## Installing

Download `seesongs-x86_64.AppImage` from the
[latest release](https://github.com/Jamedjo/seesongs/releases/latest), make it
executable and run it. It needs `playerctl` (`sudo apt install playerctl`).

The first time it runs, it adds itself to the app launcher, unless
AppImageLauncher or Gear Lever has added it already. Removing it from the
launcher is respected. It looks for a newer release every few hours, downloads
it in the background, and replaces itself with it when the window closes, or
else the next time it starts.

## Releasing

Every push to `main` builds the AppImage as a workflow artifact, and a `v*`
tag publishes it as a GitHub release, which installed copies update from:

```
scripts/release.sh 0.2.0        # raises the version, for a release pull request
scripts/release.sh --tag 0.2.0  # once that's merged, tags main and pushes the tag
```

The tag has to match the crate's version, since installed copies compare it to
decide whether to update. `packaging/linux/appimage.sh` builds the AppImage
with [Velopack](https://velopack.io).

## Building from source

- `playerctl`
- Rust, plus the Dioxus desktop libraries. On Debian and Ubuntu:

  ```
  sudo apt install playerctl libwebkit2gtk-4.1-dev libxdo-dev libssl-dev
  ```

## Running

```
cargo run --release
```

For live reload while working on it, `dx serve --platform desktop`.

The window is titled `Now Playing`. In sway, to float it and keep it on every
workspace:

```
for_window [title="^Now Playing$"] floating enable, sticky enable
```

## Browsers

Chrome and Firefox pass on whatever the page sets with the Media Session API.
Pages that set nothing show only the tab title. Bandcamp's embedded player is
one of these.

## Screenshots

The screenshots render the window's own markup at moments of Sway's
"This Is My Demo", with lyrics from LRCLIB. To regenerate them (needs
Google Chrome and jq):

```
screenshots/shoot.sh
```
