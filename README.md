# niraboard

A compact, keyboard-first laptop power dashboard for the terminal.

`niraboard` is a small Ratatui/Crossterm TUI for CachyOS/Arch laptops. It shows
battery, CPU, memory, Wi-Fi, local/global IP addresses, power profile,
brightness, and PipeWire volume in one screen, and lets you change the profile,
brightness, volume, and mute state without leaving the terminal. Clicking the
network card (or pressing `w`) opens NetworkManager's `nmtui` for connection
changes. It is an on-demand utility: there is no daemon, no background service,
and no polling when it is not running.

Colours, the startup layout, and the brightness/volume step sizes can be set in
an optional TOML configuration file. See [Configuration](#configuration).

```text
 WAYBOARD  ThinkPad L580  ·  laptop power                     Vertical · 2s tick
╭ Battery ─────────────────────────────────────────────────────────────────────╮
│████████████79% ██████▏                                                       │
│ 79%  Not charging                                                            │
│AC connected · 30.0 Wh                                                        │
╰──────────────────────────────────────────────────────────────────────────────╯
╭ CPU ─────────────────────────────────────────────────────────────────────────╮
│██▉                   6%                                                      │
│Usage 6%                                                                      │
│Temp  42.0 °C   Freq  0.75 GHz                                                │
╰──────────────────────────────────────────────────────────────────────────────╯
╭ Memory ──────────────────────────────────────────────────────────────────────╮
│██▌       11%                                                                 │
│Used  1.6 / 15.4 GiB                                                          │
│13.8 GiB available                                                            │
╰──────────────────────────────────────────────────────────────────────────────╯
╭ Network ─────────────────────────────────────────────────────────────────────╮
│● Connected                                                                   │
│SSID-3D3910                                                                   │
│Signal 97% · 130 Mb/s                                                         │
│Local  192.168.1.42                                                           │
│Global 203.0.113.7                                                            │
│click / w: nmtui                                                              │
╰──────────────────────────────────────────────────────────────────────────────╯
╭ Power profile ───────────────────────────────────────────────────────────────╮
│● power-saver                                                                 │
│  balanced                                                                    │
│  performance                                                                 │
╰──────────────────────────────────────────────────────────────────────────────╯
╭ Brightness ──────────────────────────────────────────────────────────────────╮
│█▏        5%                                                                  │
│[-]       5%        [+]                                                       │
│5% steps                                                                      │
╰──────────────────────────────────────────────────────────────────────────────╯
╭ Audio ───────────────────────────────────────────────────────────────────────╮
│██████████▍40%                                                                │
│[-]         40%        [+]                                                    │
│          [MUTE]                                                              │
╰──────────────────────────────────────────────────────────────────────────────╯
tab focus  ←/→ adjust  m mute  v layout  w nmtui  1-3 profile  r refresh  q quit
ready · click cards or buttons
```

## Controls

| Input | Action |
| --- | --- |
| `tab` / `shift+tab` | Move focus between the power, brightness, and audio cards |
| `←` / `→` or `h` / `l` | Adjust the focused card: cycle profile, brightness, or volume by the configured step |
| `m` | Toggle mute |
| `v` | Cycle the card layout (see below) |
| `w` | Open `nmtui` to manage Wi-Fi connections |
| `1` / `2` / `3` | Set `power-saver`, `balanced`, or `performance` directly |
| `r` | Refresh all data immediately |
| `q`, `Esc`, `Ctrl-C` | Quit cleanly |
| Mouse | Click a profile row, `[-]`, `[+]`, `[MUTE]`, a card to focus it, the network card, or the layout name in the header |

Clicking `[-]` or `[+]` changes brightness or volume by the configured step
(5% by default). Clicking a profile row selects it directly. Clicking anywhere
else in an interactive card gives that card keyboard focus. Clicking the
network card (or pressing `w`) temporarily leaves niraboard and opens `nmtui`;
the network data is refreshed when it exits. Clicking the layout name in the
top-right corner (or pressing `v`) changes the arrangement. Keyboard operation
never depends on the mouse.

### Layouts

`v` cycles through four arrangements. The startup layout comes from the
configuration file and falls back to `Vertical`. The current layout is shown in
the top-right corner of the header.

| Layout | Arrangement |
| --- | --- |
| `Vertical` | One full-width card per row (default) |
| `Horizontal` | One full-height card per column |
| `2 columns` | Two cards per row in a row-major grid |
| `3 columns` | Three cards per row in a row-major grid |

In the two- and three-column layouts, every card in a column has exactly the
same width. Leftover terminal cells are absorbed by the gaps rather than making
one column wider than the others. The layout is not persisted between runs; set
`layout` in the configuration file to choose the startup layout.

Because the application enables terminal mouse reporting, normal terminal text
selection may need `Shift` while `niraboard` is open. This is a terminal
convention, not a setting in `niraboard`.

## Requirements

- CachyOS or another Arch-based, Linux system
- Rust 1.85 or newer (edition 2024; the CachyOS `rust` package is current)
- A true terminal (the program refuses to start when stdin/stdout is not a TTY)

Runtime integrations are optional. Missing tools are shown as unavailable and
never crash the application:

| Integration | Used for |
| --- | --- |
| `/proc/stat`, `/proc/meminfo` | CPU utilisation and memory |
| `/sys/class/power_supply` | Battery percentage, state, power, health |
| `/sys/class/backlight` | Brightness reading |
| `/sys/class/hwmon`, `/sys/class/thermal` | CPU temperature |
| `brightnessctl` | Changing brightness |
| `wpctl` (PipeWire/WirePlumber) | Default output volume and mute |
| `nmcli` (NetworkManager) | Wi-Fi SSID, signal, link rate |
| `ip` (iproute2) | Local IPv4 address used for the default route |
| `curl` | Public IPv4 lookup over HTTPS |
| `nmtui` (NetworkManager) | Interactive Wi-Fi and connection editor |
| `powerprofilesctl` (power-profiles-daemon) | Changing power profile |
| `busctl` (systemd) | Cheap read of the active profile property |
| `upower` | Battery fallback when no sysfs battery exists |

On Arch/CachyOS, the usual package set is:

```sh
sudo pacman -S --needed rust cargo brightnessctl networkmanager \
  power-profiles-daemon pipewire wireplumber upower iproute2 curl
```

`powerprofilesctl` and `networkmanager` may already be installed and running.

## Build and run

```sh
git clone <your-repository> niraboard
cd niraboard
cargo run --release
# or
cargo build --release
./target/release/niraboard
```

The project uses `Cargo.lock`, so reproducible builds can use `--locked`:

```sh
cargo build --release --locked
```

The release profile enables thin LTO and strips symbols. The resulting binary
is roughly 1 MB on x86_64.

## Install

For the current user:

```sh
cargo install --path . --locked
```

This places `niraboard` in `~/.cargo/bin`. Make sure that directory is in
`PATH`.

For a system-wide manual install:

```sh
sudo install -Dm755 target/release/niraboard /usr/local/bin/niraboard
```

### Arch/CachyOS package (optional)

A `packaging/PKGBUILD` template is included. It expects a source tarball named
`niraboard-0.1.0.tar.gz` next to the PKGBUILD. Create one from the source tree
without needing a committed Git history:

```sh
mkdir -p packaging
tar --exclude=target --exclude=.git \
  --transform 's,^,niraboard-0.1.0/,' \
  -czf packaging/niraboard-0.1.0.tar.gz \
  Cargo.toml Cargo.lock src README.md LICENSE examples packaging/niraboard.desktop

cd packaging
makepkg -si
```

Update `pkgver`, the tarball name, and `url` in the PKGBUILD if you publish a
release. The PKGBUILD lists runtime tools as optional dependencies and builds
with `cargo build --release --locked`.

### fuzzel and other launchers

`packaging/niraboard.desktop` is a freedesktop entry that lets `fuzzel` (and
other XDG application launchers) find niraboard. Install it for the current
user:

```sh
install -Dm644 packaging/niraboard.desktop \
  ~/.local/share/applications/niraboard.desktop

# optional, but refreshes desktop caches immediately
update-desktop-database ~/.local/share/applications
```

Then start `fuzzel` and search for `niraboard`. The entry uses `Terminal=true`,
so fuzzel opens it in the terminal configured by its `terminal=` setting (for
example `terminal=foot -e`). If `niraboard` is not in the `PATH` seen by fuzzel,
change `Exec=niraboard` in the installed file to the absolute path, such as
`Exec=/home/you/.cargo/bin/niraboard`.

## Configuration

niraboard reads an optional TOML file. The first path that applies is used:

1. `$WAYBOARD_CONFIG`
2. `$XDG_CONFIG_HOME/niraboard/config.toml`
3. `$HOME/.config/niraboard/config.toml`

A missing file simply means defaults. `examples/config.toml` is a commented
starting point:

```sh
mkdir -p ~/.config/niraboard
cp examples/config.toml ~/.config/niraboard/config.toml
```

```toml
# Startup layout: "vertical", "horizontal", "2-columns", or "3-columns".
layout = "vertical"

[colors]
# Accent for the header, focused borders, buttons, and key hints.
main = "cyan"

# Normal gauge fill. Warning and critical states stay yellow/red.
bar = "green"

[steps]
# Percentage changed by one brightness / volume adjustment (1-100).
brightness = 5
volume = 5
```

Colours accept names (`cyan`, `light-blue`, `red`, ...), an ANSI index
(`0`-`255`), or a hexadecimal value (`"#4fc3f7"`). Unknown keys, invalid
colours, and steps outside 1-100 are reported in the footer; niraboard then
starts with defaults instead of exiting.

## Permissions and safety

`niraboard` never runs anything with elevated privileges. It also never writes
sysfs directly.

- **Brightness**: reading `/sys/class/backlight` needs no special permissions.
  Changing it uses `brightnessctl`, which normally works through a udev rule or
  membership in the `video` group. If changing brightness fails, check
  `brightnessctl -m` and make sure your user is in the `video` group (log out
  and back in after adding a group).
- **Power profile**: `powerprofilesctl set ...` goes through
  power-profiles-daemon and polkit. It should work from a normal desktop
  session. Running `niraboard` as root is not recommended.
- **Audio**: `wpctl` talks to the user's PipeWire session. Run `niraboard` in
  the graphical session, not from `sudo`.
- **Wi-Fi**: `nmcli` reads NetworkManager state. Clicking the network card (or
  pressing `w`) runs `nmtui` for connection changes; NetworkManager and polkit
  apply the usual authorization. A user in the `network` group or with the
  usual desktop policy can query and change connections.
- **IP addresses**: the local address is read from `ip -4 route get`. The
  global address is fetched over HTTPS from `https://api.ipify.org` with
  `curl`. Nothing else is sent to that service. If `curl` is missing or the
  lookup fails, the global line shows `—`.
- **Battery fallback**: `upower` reads the system UPower daemon.
- **Commands**: every process is started with `Command::new(program).args(...)`.
  The code never builds a shell command string, and the only variable arguments
  are profile names and numeric brightness/volume steps from the configuration
  file (validated to 1-100). `nmtui` runs with the terminal's own stdio after
  niraboard temporarily restores the terminal.

If a command is missing or fails, the failure appears in the footer instead of
terminating the UI. For example, with no `brightnessctl` in `PATH`, the
brightness card remains readable and the footer reports the failed action.

## Data sources and refresh policy

The event loop wakes every 2 seconds. Data collection is deliberately split:

| Data | Interval | Source |
| --- | --- | --- |
| CPU, memory, battery, brightness | every tick (2 s) | sysfs/procfs |
| Audio | 4 s | `wpctl get-volume` |
| Wi-Fi and local IP | 15 s | `nmcli ... --rescan no`, `ip -4 route get` |
| Global IP | 300 s | `curl https://api.ipify.org` |
| Power profile | 30 s | `busctl` property read |
| UPower fallback | 30 s, only when no sysfs battery | `upower -e` / `upower -i` |

CPU utilisation is always a delta between the previous and current
`/proc/stat` samples. The first sample therefore shows `—`, not a misleading
percentage.

The Wi-Fi refresh uses NetworkManager's cached list (`--rescan no`), so it does
not force a radio scan every 15 seconds. The local IP lookup is cheap and rides
along with that refresh. The global IP lookup is deliberately much slower (5
minutes) because it is an external HTTPS request. The profile is normally read
through `busctl` because `powerprofilesctl` is a Python program that takes about
half a second to start. `powerprofilesctl` remains the tool used for profile
changes, as recommended by power-profiles-daemon.

Rendering is immediate-mode, but it is not unconditional: each refresh produces
a typed `Metrics` snapshot, and the loop only redraws when that snapshot,
the focus, the status message, or the terminal size changes. A 2-second tick
does not automatically mean 2-second redraws.

## Architecture

```text
src/
├── main.rs          terminal setup/restore, event/tick loop
├── app.rs           application state, focus, key/mouse handling, actions
├── config.rs        TOML loading, colours, startup layout, step sizes
├── event.rs         crossterm event polling
├── actions.rs       profile/brightness/audio/nmtui command wrappers
├── ui.rs            Ratatui layout, widgets, hit areas, colours
└── system/
    ├── mod.rs       Metrics, Monitor, refresh scheduling, identity
    ├── command.rs   argument-based and interactive process execution
    ├── sysfs.rs     small sysfs/procfs helpers
    ├── battery.rs   sysfs battery + UPower fallback
    ├── cpu.rs       /proc/stat delta sampler, temperature, frequency
    ├── memory.rs    /proc/meminfo
    ├── network.rs   nmcli parsing, local/global IP lookup
    ├── audio.rs     wpctl parsing
    ├── power_profile.rs
    └── backlight.rs
```

### Ratatui concepts used

- **`Terminal` and `CrosstermBackend`**: `niraboard` owns setup and teardown
  instead of using `ratatui::init`, because it also enables mouse capture. A
  `TerminalSession` guard restores raw mode, mouse reporting, the cursor, and
  the alternate screen on normal exit, on errors, and during unwinding. The same
  guard can temporarily suspend niraboard while `nmtui` runs, then resume and
  clear the screen. A panic hook additionally restores the terminal before the
  panic message is printed.
- **Immediate-mode rendering**: every draw describes the whole screen from
  `App` state. Widgets are cheap values constructed per frame; there is no
  retained widget tree to synchronise.
- **`Frame` and `Rect`**: the frame exposes the terminal area. The UI splits it
  with nested `Layout` calls. Rects are the single source of truth for both
  drawing and mouse hit-testing.
- **`Layout` constraints**: `Length(1)` reserves the header and footer;
  `Fill(1)` divides the remaining body between the cards. `Vertical` is one
  card per row; the grid layouts use a small `equal_spans` helper so every
  column has the same width.
- **`Block`, `Gauge`, `Paragraph`**: rounded bordered cards, one-line gauges,
  and styled `Line`/`Span` content. The layout uses only built-in widgets, which
  makes the rendering easy to follow.
- **Application state instead of `StatefulWidget`**: the selectable profile rows
  and focus state are part of `App`. A `StatefulWidget`/`ListState` would add
  indirection without saving anything at this size.
- **Hit areas**: `ui::draw` returns `HitAreas` (card rects, profile rows,
  button rects, and the network card). `main` stores them in `App`; mouse events
  call `HitAreas::action_at`, which maps coordinates to a small `ClickAction`
  enum. Rendering and input therefore share one layout definition.
- **Dirty redraw**: `App::tick` and `App::handle_event` return whether anything
  visible changed. `main` draws only when that flag is set.
- **Typed snapshots**: `Metrics` contains `Option`s for unavailable sources and
  derives `PartialEq`. That is both the UI model and the change detector.

### Why no async runtime?

The only potentially slow work is spawning a few system utilities. The normal
refresh path is cheap and staggered, so a plain `crossterm::event::poll`
timeout plus a tick is sufficient and much easier to read than an async
executor. `nmtui` intentionally takes over the terminal until it exits, so it
is synchronous by design. The one other deliberate trade-off is
`powerprofilesctl set`, which can block for roughly half a second when changing
profiles. Making that action non-blocking is listed as a future improvement.

## Known limitations

- Linux-only; the sysfs/procfs paths are intentionally not portable.
- The interface is tuned for at least 72×22 and looks best at 80×30 or larger
  in the default `Vertical` layout; the grid layouts fit shorter terminals.
  Cards clip gracefully below that; at very small sizes the screen shows a
  one-line size hint.
- Battery power draw depends on firmware. Many laptops report `0` while on AC,
  not charging, or under a charge threshold, in which case `niraboard` shows
  `—`.
- UPower is only used as a fallback when no sysfs battery is found. It is not
  polled for additional statistics while sysfs works.
- `powerprofilesctl set` briefly blocks the event loop (about 0.5 s on this
  machine). Profile *reads* use the cheap D-Bus path when `busctl` exists.
- Wi-Fi signal is NetworkManager's percentage and the link rate is the cached
  NM value; raw dBm and channel information are not shown. Connection changes
  are delegated to `nmtui`, which temporarily leaves the niraboard screen.
- Local IP is the source address for the default route. Secondary addresses,
  IPv6 local addresses, and per-interface lists are not shown.
- Global IP comes from `api.ipify.org` over IPv4, so an offline machine or an
  IPv6-only connection shows `—`. The lookup may delay the first refresh by up
  to 3 seconds and is repeated every 5 minutes.
- Only the default audio sink is controlled. Sources, per-application streams,
  and profiles are not exposed.
- Brightness changes require `brightnessctl`. There is no direct sysfs write
  fallback because that would usually require root.
- CPU temperature covers hwmon `coretemp`/`k10temp`/`zenpower` and common
  thermal zones. There is no `sensors` fallback yet.
- No history graphs, battery time estimate, or low-battery notification.
- Terminal mouse capture can interfere with text selection unless `Shift` is
  held.

## Possible future improvements

- Move slow actions to a small worker thread or use the D-Bus API directly, so
  profile changes never pause input.
- Extend the configuration file with refresh intervals and key bindings.
- Cache the last global IP across runs so a temporarily offline machine still
  shows the previous value.
- Sparklines for CPU, memory, and battery power history.
- Battery time-to-empty and charge-threshold controls (with explicit policy
  checks).
- Per-core CPU view and a compact process list.
- Native UPower/NetworkManager D-Bus integrations to avoid process spawning.
- Accessibility/theme options and remembering the selected layout between runs.
- Package into a proper Arch release with a PKGBUILD that points at a tagged
  tarball.

## License

MIT. See `LICENSE`.
