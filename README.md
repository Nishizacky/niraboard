# niraboard

A compact, keyboard-first control panel for the terminal — built for **niri**
as an on-demand replacement for an always-visible bar like Waybar.

`niraboard` is a small Ratatui/Crossterm TUI for Linux laptops, developed on
CachyOS/Arch. It shows battery, CPU, memory, root filesystem, Wi-Fi, local and
public IP addresses, power profile, brightness, and PipeWire volume on one
screen, and lets you change the power profile, brightness, volume, and mute
state without leaving the terminal. Pressing `n` opens a mako notification
browser with `Active` and `History` tabs, where you can invoke, dismiss, and
restore mako's notifications. Clicking the network card (or pressing `w`)
opens NetworkManager's `nmtui` for connection changes; the network data is
refreshed when it exits.

It is an on-demand utility: there is no daemon, no background service, and no
polling when it is not running. The header clock and date, colours, the startup
layout, card order, and the brightness/volume step sizes come from an optional
TOML configuration file.

## The idea: a bar you call, not a bar you keep

niraboard was designed around niri and its scrollable tiling workflow, where a
permanently visible status bar mostly costs screen space. A tiling desktop does
not need a strip of pixels that is always there; it needs a control panel that
appears the moment you ask for it and disappears when you are done.

Bind niraboard to a key. One keystroke gives you a single screen with the whole
machine on it — battery, CPU, memory, disk, network, IP addresses, power
profile, brightness, and audio. Change what you need in place, press `q`, and
the workspace is yours again. Between those two moments niraboard uses no
memory, no CPU, and no battery.

That trade-off is the point:

- **On demand, not always on.** No daemon, no background service, no idle
  polling, no process to babysit. Start it, use it, quit it.
- **A control panel, not a readout.** A bar tells you the volume; niraboard
  lets you change it, mute it, switch the power profile, adjust the backlight,
  and open `nmtui` from the same screen.
- **One screen, whole machine.** Everything relevant is visible at once, so you
  look, act, and leave instead of hovering over individual modules.
- **A normal window.** It is a plain terminal application, so niri decides
  where it goes. Make it floating in your niri window rules and it behaves like
  a quick utility overlay; tile it and it is just another workspace window.

In niri, a single bind is enough:

```kdl
binds {
    Mod+Shift+N { spawn "niraboard"; }
}
```

niraboard opens a terminal for itself when it is started from a graphical
session without one, so the bind above works as-is. Pass
`--spawn-terminal` to force that behaviour. See
[Desktop entry](#desktop-entry) for the launcher setup.

```text
 2026-10-31 14:05  NIRABOARD  ThinkPad L580  ·  laptop power      Vertical · 2s tick
╭ Battery ─────────────────────────────────────────────────────────────────────╮
│████████████79% ██████▏                                                       │
│ 79%  Not charging                                                            │
│AC connected · 30.0 Wh                                                        │
╰──────────────────────────────────────────────────────────────────────────────╯
╭ CPU ─────────────────────────────────────────────────────────────────────────╮
│██▉                   6%                                                      │
│Usage   6%                                                                    │
│Temp   42.0 °C   Freq  0.75 GHz                                               │
│Threads 8                                                                     │
│Model   Intel Core i7-8550U                                                   │
╰──────────────────────────────────────────────────────────────────────────────╯
╭ Memory ──────────────────────────────────────────────────────────────────────╮
│██▌       11%                                                                 │
│Used  1.6 / 15.4 GiB                                                          │
│13.8 GiB available                                                            │
╰──────────────────────────────────────────────────────────────────────────────╯
╭ Disk ────────────────────────────────────────────────────────────────────────╮
│██████████████▊                         48%                                   │
│Used  111.8 / 233.7 GiB                                                       │
│FS    ext4 · /                                                                │
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
 tab focus  ←/→ adjust  m mute  v layout  w nmtui  1-3 profile  r refresh  q quit  [n] notifications
 ready · click cards or buttons
```

## Requirements

- Linux; the sysfs/procfs paths are Linux-specific (developed on CachyOS/Arch).
- Target environment: niri on Wayland. Any compositor works — it is just a
  terminal application — but the on-demand, bar-free workflow is designed
  around a scrollable tiling compositor.
- Rust 1.88 or newer (edition 2024; the CachyOS `rust` package is current).
- `mold`, selected as the linker on Linux by `.cargo/config.toml`.
- A true terminal for the dashboard. `--help`, `--version`, and
  `--print-default-config` work without one. In a graphical session niraboard
  opens a terminal itself when it is started without one; see
  [Desktop entry](#desktop-entry).

Runtime integrations are optional. Missing tools are shown as unavailable and
never crash the application:

| Integration | Used for |
| --- | --- |
| `/proc/stat`, `/proc/meminfo` | CPU utilisation and memory |
| `statvfs` (via rustix), `/proc/self/mounts` | Root filesystem usage and filesystem type |
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
| `makoctl` (mako) | Reading and acting on mako's notification stack |

On Arch/CachyOS, the usual package set is:

```sh
sudo pacman -S --needed rust cargo mold brightnessctl networkmanager \
  power-profiles-daemon pipewire wireplumber upower iproute2 curl mako
```

`powerprofilesctl` and `networkmanager` may already be installed and running.

## Build and install

### From source

```sh
git clone <your-repository> niraboard
cd niraboard
cargo build --release --locked
./target/release/niraboard
```

`cargo run --release` works too. The release profile enables thin LTO and strips
symbols; the resulting binary is about 1.3 MB on x86_64.

### Cargo

```sh
cargo install --path . --locked
```

This places `niraboard` in `~/.cargo/bin`. Make sure that directory is in
`PATH`.

### System-wide

```sh
sudo install -Dm755 target/release/niraboard /usr/local/bin/niraboard
```

### Desktop entry

`packaging/niraboard.desktop` lets `fuzzel` and other XDG application launchers
find niraboard. Install it for the current user:

```sh
install -Dm644 packaging/niraboard.desktop \
  ~/.local/share/applications/niraboard.desktop

# optional, but refreshes desktop caches immediately
update-desktop-database ~/.local/share/applications
```

Then search for `niraboard`. The entry uses `Terminal=true`, so the launcher
opens it in the configured terminal (for example `terminal=foot -e`). If
`niraboard` is not in the launcher's `PATH`, change `Exec=niraboard` in the
installed file to an absolute path, such as
`Exec=/home/you/.cargo/bin/niraboard`.

Some launchers, window-manager key bindings, and scripts ignore `Terminal=true`
and start the binary without a terminal. In a graphical session niraboard then
opens one itself and runs there. It tries `$TERMINAL` (which may carry options
such as `foot -e`), then `xdg-terminal-exec`, then the first installed emulator
from `foot`, `kitty`, `ghostty`, `alacritty`, `wezterm`, `gnome-terminal`,
`konsole`, `xfce4-terminal`, `tilix`, `terminator`, `x-terminal-emulator`,
`xterm`, `urxvt`, `st`, or `qterminal`. The relaunched process keeps the
`--config` path and carries `NIRABOARD_TERMINAL_CHILD=1`, so a terminal that
fails to provide a TTY cannot cause a relaunch loop. Without
`$WAYLAND_DISPLAY` or `$DISPLAY` there is nowhere to open a terminal, and the
dashboard exits with an error instead of guessing.

## Usage

### Command-line options

```text
Usage: niraboard [OPTION]

Options:
  -h, --help                  Print this help and exit
  -V, --version               Print the version and exit
  -c, --config <FILE>         Read configuration from FILE
      --spawn-terminal        Open a terminal emulator for the dashboard when
                              no terminal is attached
      --print-default-config  Print the default configuration as TOML and exit
```

`--config FILE` (or `-c FILE`, `--config=FILE`) reads settings from `FILE`
instead of the default location. A missing file still means defaults.

`--spawn-terminal` forces the terminal relaunch described under
[Desktop entry](#desktop-entry), even when only one of stdin/stdout is
redirected. Without the flag that case is an error, so `niraboard > log` does
not unexpectedly open a new window.

`--print-default-config` writes the built-in defaults to stdout in the same
TOML format the configuration file uses. It does not read an existing file and
does not need a terminal, so it can seed a configuration. The document is
serialized from the same schema that loads the file, so every supported key is
emitted with its current default value:

```sh
mkdir -p ~/.config/niraboard
niraboard --print-default-config > ~/.config/niraboard/config.toml
```

### Controls

| Input | Action |
| --- | --- |
| `tab` / `shift+tab` | Move focus between the power, brightness, and audio cards |
| `←` / `→` or `h` / `l` | Adjust the focused card: cycle profile, brightness, or volume by the configured step |
| `m` | Toggle mute |
| `n` | Open or close the mako notification browser |
| `v` | Cycle the card layout (see [Layouts](#layouts)) |
| `w` | Open `nmtui` to manage Wi-Fi connections |
| `1` / `2` / `3` | Set `power-saver`, `balanced`, or `performance` directly |
| `r` | Refresh all data immediately |
| `q`, `Esc`, `Ctrl-C` | Ask to quit; press `y` to confirm |
| Mouse | Click a profile row, `[-]`, `[+]`, `[MUTE]`, a card to focus it, the network card, the layout name in the header, or the notifications button |

Clicking `[-]` or `[+]` changes brightness or volume by the configured step
(5% by default). Clicking a profile row selects it directly. Clicking anywhere
else in an interactive card gives that card keyboard focus. Clicking the
network card (or pressing `w`) temporarily leaves niraboard and opens `nmtui`;
the network data is refreshed when it exits. Clicking the layout name in the
top-right corner (or pressing `v`) changes the arrangement. Keyboard operation
never depends on the mouse.

While the mako window is open, `tab` / `shift+tab` or `h` / `l` switch between
`Active` and `History`, `j` / `k` move the selection, `PageUp` / `PageDown`
move it five rows, and `Home` / `End` jump to the ends. `Enter` invokes the
default action of the selected active notification, `d` dismisses it, `D`
dismisses every active notification, `o` restores the most recently dismissed
one, and `r` refreshes the list immediately. `Esc` or `n` closes the window.
Dashboard shortcuts are ignored behind the window. Clicking a tab selects it,
clicking a row selects that notification, and clicking outside closes the
window.

### Notifications

`n` or the button at the right end of the footer opens a browser for the
running [mako](https://github.com/emersion/mako) notification daemon. niraboard
does not receive notifications itself and does not keep a second copy of the
stack: mako remains the owner, and niraboard is only a client for it.

| Tab | Contents |
| --- | --- |
| `Active` | Notifications mako is currently showing |
| `History` | Notifications mako has already dismissed |

The window is fetched lazily. Nothing is read at startup or while the window is
closed. Opening the window runs `makoctl list -j` and `makoctl history -j`; it
is refreshed again every 10 seconds while open and immediately after an action
or an explicit `r`.

Actions map directly to mako's own IPC: `makoctl invoke -n ID`,
`makoctl dismiss -n ID`, `makoctl dismiss -a`, and `makoctl restore`. Invoking
and dismissing by id is only available on the `Active` tab, because mako's
history entries are no longer actionable.

Notification text is external input. Control characters, including terminal
escape sequences, are removed and newlines/tabs are flattened before rendering,
so a notification cannot disturb the layout. If mako is not running or
`makoctl` is missing, the window shows the error instead of failing the
dashboard. Install and run mako separately; niraboard never starts a
notification daemon.

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

Cards always appear in the order of the `cards` array in the configuration
file. The grid layouts wrap that order row by row.

Each layout has its own minimum terminal size. With the eight default cards:

| Layout | Minimum | Bottleneck |
| --- | --- | --- |
| `Vertical` | 72×43 | Height for eight card rows |
| `Horizontal` | 191×8 | Width for eight card columns |
| `2 columns` | 72×23 | Height for four card rows |
| `3 columns` | 72×18 | Height for three card rows |

Each extra card row adds five terminal rows. Beyond the 72-column base, every
extra card column adds 23 terminal columns plus a one-cell gap. Configuring
fewer cards lowers the requirement. The minimum is checked again as soon as `v`
changes the layout, and a terminal below it shows a centered hint with the
required width and height instead of squeezing or clipping the cards.

In the two- and three-column layouts, every card in a column has exactly the
same width. Leftover terminal cells are absorbed by the gaps rather than making
one column wider than the others. The layout is not persisted between runs; set
`layout` in the configuration file to choose the startup layout.

Because the application enables terminal mouse reporting, normal terminal text
selection may need `Shift` while `niraboard` is open. This is a terminal
convention, not a setting in `niraboard`.

## Configuration

niraboard reads an optional TOML file. The first path that applies is used:

1. `--config FILE` (or `-c FILE`, `--config=FILE`)
2. `$NIRABOARD_CONFIG` (ignored when empty)
3. `$XDG_CONFIG_HOME/niraboard/config.toml`
4. `$HOME/.config/niraboard/config.toml`

A missing file simply means defaults. `examples/config.toml` is a commented
starting point:

```sh
mkdir -p ~/.config/niraboard
cp examples/config.toml ~/.config/niraboard/config.toml
```

```toml
# Startup layout: "vertical", "horizontal", "2-columns", or "3-columns".
layout = "vertical"

# Cards in display order. Remove a name to hide that card. Cards that do not
# fit on screen are omitted automatically.
cards = [
    "battery",
    "cpu",
    "memory",
    "disk",
    "network",
    "power",
    "brightness",
    "audio",
]

[clock]
# Show the clock and date in the header, left of the NIRABOARD title.
enabled = true

# strftime-style format for the current local time.
format = "%Y-%m-%d %H:%M"

[colors]
# Accent for the header, focused borders, buttons, and key hints.
main = "cyan"

# Normal gauge fill. Warning and critical states use warn / bad below.
bar = "green"

# Regular values shown in cards.
text = "gray"

# Labels, hints, and secondary information.
muted = "dark-gray"

# State colours: connected/healthy, warning, and critical/error.
ok = "green"
warn = "yellow"
bad = "red"

[steps]
# Percentage changed by one brightness / volume adjustment (1-100).
brightness = 5
volume = 5
```

Colours accept names (`cyan`, `light-blue`, `red`, ...), an ANSI index
(`0`-`255`), or a hexadecimal value (`#4fc3f7`). Unknown keys, invalid colours,
unknown or duplicate card names, and steps outside 1-100 are printed to stderr
before the dashboard starts; niraboard then runs with defaults instead of
exiting.

The `[clock]` section controls the date and time shown to the left of the
header title. Set `enabled = false` to hide it, or change `format` to any
strftime-style pattern. Supported specifiers are `%Y` year, `%y` two-digit
year, `%m` month, `%d` day, `%e` space-padded day, `%H` 24-hour, `%I`
12-hour, `%M` minute, `%S` second, `%p` AM/PM, `%a` / `%A` short / full
weekday, `%b` / `%B` short / full month, `%j` day of year, `%F` date,
`%T` time, `%R` hour and minute, `%z` UTC offset, and `%%` for a literal
percent sign. Unknown specifiers are shown unchanged, which makes typos easy to
spot. The default `"%Y-%m-%d %H:%M"` refreshes once per minute; add `%S` for a
live seconds display. On terminals too narrow to fit both, the clock is hidden
so the title stays visible.

`--print-default-config` prints the same defaults without the explanatory
comments. Use `examples/config.toml` when you want a documented starting file.

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
| CPU, memory, disk, battery, brightness | every tick (2 s) | sysfs/procfs, `statvfs` via rustix |
| Audio | 4 s | `wpctl get-volume` |
| Wi-Fi and local IP | 15 s | `nmcli ... --rescan no`, `ip -4 route get` |
| Global IP | 300 s | `curl https://api.ipify.org` |
| Power profile | 30 s | `busctl` property read |
| UPower fallback | 30 s, only when no sysfs battery | `upower -e` / `upower -i` |
| mako notifications | on open, then every 10 s while open | `makoctl list -j` / `history -j` |

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
a typed `Metrics` snapshot, and the loop only redraws when that snapshot, the
focus, the status message, the clock text, or the terminal size changes. A
2-second tick does not automatically mean 2-second redraws.

## Architecture

```text
src/
├── main.rs          terminal setup/restore, event/tick loop, CLI options
├── app.rs           application state, focus, key/mouse handling, actions
├── clock.rs         strftime-style header clock formatting
├── config.rs        TOML loading, colours, clock, layout, card order, steps
├── event.rs         crossterm event polling
├── actions.rs       profile/brightness/audio/nmtui command wrappers
├── notifications.rs mako stack client, tabs, selection, invoke/dismiss actions
├── spawn.rs         relaunches the dashboard in a terminal when needed
├── ui.rs            Ratatui layout, widgets, hit areas, colours
└── system/
    ├── mod.rs       Metrics, Monitor, refresh scheduling, identity
    ├── command.rs   argument-based, interactive, and detached process execution
    ├── sysfs.rs     small sysfs/procfs helpers
    ├── battery.rs   sysfs battery + UPower fallback
    ├── cpu.rs       /proc/stat delta sampler, model, threads, temperature, frequency
    ├── memory.rs    /proc/meminfo
    ├── disk.rs      filesystem usage via rustix and mount table filesystem type
    ├── network.rs   nmcli parsing, local/global IP lookup
    ├── audio.rs     wpctl parsing
    ├── power_profile.rs
    └── backlight.rs
```

### Implementation notes

- **Terminal ownership**: niraboard sets up and tears down the terminal itself
  instead of using `ratatui::init`, because it also enables mouse capture. A
  `TerminalSession` guard restores raw mode, mouse reporting, the cursor, and
  the alternate screen on normal exit, on errors, and during unwinding. The same
  guard can suspend niraboard while `nmtui` runs, then resume and clear the
  screen. A panic hook restores the terminal before printing the panic message.
- **Launcher fallback**: when both streams are not terminals and a display
  server is available, `spawn` re-executes the current binary in a terminal
  emulator and exits; it never uses a shell. The child gets its own process
  group so it survives the caller closing its terminal, and the
  `NIRABOARD_TERMINAL_CHILD=1` environment variable prevents relaunch loops.
- **Immediate-mode rendering**: every draw describes the whole screen from
  `App` state. Widgets are cheap values constructed per frame; there is no
  retained widget tree to synchronise.
- **Rects and hit areas**: `ui::draw` returns `HitAreas` (card rects, profile
  rows, button rects, the network card, and notification targets). `main` stores
  them in `App`; mouse events call `HitAreas::action_at`, which maps coordinates
  to a small `ClickAction` enum. Rendering and input therefore share one layout
  definition.
- **Notifications**: `notifications::Center` is a client for mako, not a
  notification daemon or history store. It owns the open state, selected tab,
  selection, cached `makoctl` output, last refresh time, and any error. `App`
  keeps only a short-lived footer status for control actions. The window
  refreshes the stack when opened, every 10 seconds while visible, and on
  demand, so no work happens while it is closed.
- **Dirty redraw**: `App::tick` and `App::handle_event` return whether anything
  visible changed. `main` draws only when that flag is set. `Metrics` holds
  `Option`s for unavailable sources and derives `PartialEq`, so the same typed
  snapshot is both the UI model and the change detector.

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
- The minimum terminal size depends on the layout and the number of configured
  cards. With all eight default cards it is 72×43 for `Vertical`, 191×8 for
  `Horizontal`, 72×23 for two columns, and 72×18 for three columns. Below that,
  the screen shows a centered hint with the required dimensions. `Vertical`
  needs the most height, while `Horizontal` trades height for width; the grid
  layouts fit shorter terminals.
- Battery power draw depends on firmware. Many laptops report `0` while on AC,
  not charging, or under a charge threshold, in which case `niraboard` shows
  `—`.
- UPower is only used as a fallback when no sysfs battery is found. It is not
  polled for additional statistics while sysfs works.
- The disk card reports `/` only. Other mounted filesystems, per-partition
  details, and I/O throughput are not shown.
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
- No history graphs or battery time estimate.
- Notifications are mako-only. A running mako daemon and `makoctl` are required
  for that window; dunst and swaync are not supported. The stack is read only
  while the window is open and is never persisted by niraboard.
- Terminal mouse capture can interfere with text selection unless `Shift` is
  held.

## Possible future improvements

- Move slow actions to a small worker thread or use the D-Bus API directly, so
  profile changes never pause input.
- Extend the configuration file with refresh intervals and key bindings.
- Cache the last global IP across runs so a temporarily offline machine still
  shows the previous value.
- Sparklines for CPU, memory, and battery power history.
- Choose among a notification's named actions instead of only its default.
- Battery time-to-empty and charge-threshold controls (with explicit policy
  checks).
- Per-core CPU view and a compact process list.
- Native UPower/NetworkManager D-Bus integrations to avoid process spawning.
- Accessibility/theme options and remembering the selected layout between runs.
- Package into a proper Arch release with a PKGBUILD that points at a tagged
  tarball.

## License

MIT. See [`LICENSE`](LICENSE).
