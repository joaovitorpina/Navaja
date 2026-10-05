# Manual QA

What a person checks at the end of each milestone, on the desktops [roadmap.md](roadmap.md) §2 "Manual QA" names: Windows 11, Ubuntu GNOME Wayland with AppIndicator, Ubuntu X11, Fedora GNOME (no tray), KDE Plasma 6 Wayland and macOS 26. CI covers what a runner can. These checks need what no runner has: Windows 11 rather than Server, a Wayland session, a tray host, Klipper, a real elevated start, and a person watching the screen.

This page holds the M2a checks. Later milestones add theirs.

## What M2a needs from a person

| Check | For | Where |
|---|---|---|
| [1. Starts hidden, shows without a flash](#1-starts-hidden-shows-without-a-flash) | M2a Exit | Windows 11, Ubuntu Wayland, Ubuntu X11, macOS 26 (recorded); Fedora, KDE (watched) |
| [2. A second launch: `--tool` and `--toggle`](#2-a-second-launch---tool-and---toggle) | M2a Exit; §2 Shell | all six |
| [3. The tray](#3-the-tray) | M2a Exit; §2 Shell | all six (Fedora: no tray) |
| [4. Close and quit](#4-close-and-quit) | §2 Shell | all six |
| [5. Keyboard only](#5-keyboard-only) | §2 Shell | all six |
| [6. About's link and the logs folder](#6-abouts-link-and-the-logs-folder) | §2 Shell | all six |
| [7. Elevated or root start](#7-elevated-or-root-start) | M2a item 8 | Windows 11 (banner); Ubuntu, Fedora, KDE, macOS (refusal) |
| [8. Klipper](#8-klipper) | §2, M2a and M6 | KDE Plasma 6 Wayland |
| Clipboard history (Win+V) | M2a Exit | Windows 11: [spikes.md](spikes.md), "M2a exit check", by a person |
| S2.1, the RustRover half | M2a item 4 | any OS: [spikes.md](spikes.md), S2.1, "RustRover, by a person" |
| S2.3, the Windows scales | M2a item 9 | Windows 11: [spikes.md](spikes.md), S2.3, "Windows scales, by a person" |

Record each result in [the results table](#m2a-results) at the end. Attach recordings and screenshots to the PR that records them; don't commit them ([spikes.md](spikes.md), Rules).

## Before you start

- **The commit.** Clone `https://github.com/joaovitorpina/Navaja` (on Windows to a short path, below). Use `main` after M2a's last merge, and record `git rev-parse --short HEAD`.
- **A release build.** Every check below uses the release build, as it ships. A debug build does not count: on Windows it also opens a console window, it logs to stderr, and it honours `NAVAJA_APP_DIR`.
- **One Navaja at a time.** Quit any other Navaja first, a `pnpm dev` one included. Navaja runs as a single instance, so a second one hands its arguments over and exits.
- **Its files.** A release build keeps its settings and logs in the real app folder: `%APPDATA%\Navaja` on Windows, `~/Library/Application Support/Navaja` on macOS, `~/.config/navaja` on Linux. The logs are `logs/navaja.<date>.log`, one file per UTC day.
- **After signing in again,** for check 1's cold start, a switch between Ubuntu's Wayland and Xorg sessions, or KDE's optional X11 pass, the terminal that held `$exe` or `$NAVAJA` is gone. Open a new one at the repository root and run the last line of your OS's build block again: `$exe = (Resolve-Path .\target\release\navaja.exe).Path` on Windows (and its first line too, before any `pnpm`), the `ls` and `export NAVAJA=...` lines on macOS, `export NAVAJA="$PWD/target/release/navaja"` on Linux. macOS's `open target/release/...` commands need that folder too.
- **Home** is the Navaja wordmark at the top of the sidebar. Its page's heading is "Navaja" too.
- **A check fails** when a step does not behave as written. Record what happened, with the frames or a screenshot, and go on with the next check.

### What M2a does today

These are known, and are not failures:
- **Closing the window quits the app,** on every OS: nothing handles a close request yet (close behaviour per OS is roadmap M2b, item 1). Only the tray's Quit, and the start-failure view's Quit button, write a `quit` line to the log.
- **`--toggle` never quits.** It hides the window if it is shown and focused, and otherwise shows it.
- **On Wayland, a second launch can't focus the window.** The launcher's activation token is not forwarded yet, so GNOME and KDE won't raise, restore or focus it, whether it is visible but unfocused, minimized or hidden (roadmap M2b, item 8). Tray-menu actions can't take focus or restore a minimized window there either. In M2a, on Wayland, a second launch or the tray's Open only has to open what was asked for and show a hidden window again.
- **On macOS, `open -a Navaja --args --tool uuid` does not reach a running Navaja.** LaunchServices activates the running app and drops the arguments, and the app handles no Dock reopen yet. For a second launch, run the bundle's binary, `"$NAVAJA"` below.
- **The tray menu is in English** whatever the language: its labels are literals in `tray.rs` (an exception until roadmap M2b, item 1). No shipped tool sets `tray: true`, so the menu has no tool entries.
- **Clicks on the tray icon:** on Windows a left click toggles the window and a right click opens the menu. On macOS and Linux any click opens the menu; Linux delivers no click events.

## Setup

One block per OS. Each ends with a release build, and with the path of its binary.

### Windows 11

- **Machine:** Windows 11, not Windows Server. Record what `winver` shows: edition, version, OS build, and whether it is an Insider build. Record the GPU (`Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion`) and the display's scale (Settings > System > Display).
- **Install:**
  - Git for Windows.
  - Visual Studio Build Tools 2022 or later, with the "Desktop development with C++" workload (MSVC and a Windows SDK).
  - rustup, from rustup.rs, with its default host (`x86_64-pc-windows-msvc`).
  - Node.js 24 LTS, from nodejs.org.
  - The WebView2 Runtime ships with Windows 11.
  - For check 1: OBS Studio and VLC.
- **Clone to a short path,** such as `C:\src\Navaja` (`git clone https://github.com/joaovitorpina/Navaja C:\src\Navaja`). With a long one, MSVC's linker can fail with LNK1104 on a build script's path over 260 characters, even with long paths turned on.
- **Build,** in PowerShell at the repository root:

  ```powershell
  Set-ExecutionPolicy -Scope Process RemoteSigned -Force   # this window only; see below
  git switch main; git pull --ff-only; git rev-parse --short HEAD
  rustup toolchain install       # the toolchain rust-toolchain.toml pins
  corepack enable pnpm           # in an administrator PowerShell if Node is installed for all users
  pnpm install --frozen-lockfile
  pnpm tauri build --no-bundle   # writes target\release\navaja.exe
  $exe = (Resolve-Path .\target\release\navaja.exe).Path
  ```

  corepack's `pnpm` comes with a PowerShell script, `pnpm.ps1`, which PowerShell runs in place of `pnpm.cmd`. A fresh Windows 11 runs no PowerShell scripts, so without the first line `pnpm install` stops with "running scripts is disabled on this system". `-Scope Process` lasts until this window closes and changes no lasting setting. `-Force` skips the prompt that asks to confirm the change: its default answer is No, which keeps scripts blocked. Calling `pnpm.cmd` in place of `pnpm` works too.

  Keep this PowerShell window open: the checks use `$exe`. After signing out, set it again ("After signing in again", above).
- **The newest log:** `Get-ChildItem "$env:APPDATA\Navaja\logs\navaja.*.log" | Sort-Object LastWriteTime | Select-Object -Last 1`.
- **Promote the tray icon now,** before any check hides the window. Run `Start-Process $exe`. Windows 11 puts a new icon behind the ^ overflow first. Turn Navaja on under Settings > Personalization > Taskbar > Other system tray icons (it may be listed as `navaja.exe`), check that the icon now shows on the taskbar, then quit Navaja from its menu.
- **Recording:** in OBS, set Settings > Video > Common FPS Values to 60 and add a Display Capture source for the screen Navaja opens on. VLC steps through a recording frame by frame with the E key.

### macOS 26

- **Machine:** a Mac on macOS 26. Record `sw_vers`, `sysctl -n hw.model`, and whether the display is Retina (System Settings > Displays).
- **Install:** the Xcode Command Line Tools (`xcode-select --install`), rustup from rustup.rs, and Node.js 24 LTS from nodejs.org.
- **Build,** in Terminal at the repository root:

  ```sh
  git switch main && git pull --ff-only && git rev-parse --short HEAD
  rustup toolchain install       # the toolchain rust-toolchain.toml pins
  corepack enable pnpm           # with sudo if Node is installed for all users
  pnpm install --frozen-lockfile
  pnpm tauri build --bundles app # writes target/release/bundle/macos/Navaja.app
  ls target/release/bundle/macos/Navaja.app/Contents/MacOS/
  export NAVAJA="$PWD/target/release/bundle/macos/Navaja.app/Contents/MacOS/<the name ls printed>"
  ```

  Keep this Terminal window open. After signing out, set `NAVAJA` again ("After signing in again", above). `pgrep -x "$(basename "$NAVAJA")"` lists the running Navaja.
- **Keyboard navigation:** turn on System Settings > Keyboard > Keyboard navigation. Without it, Tab in a web view skips links and buttons, and check 5 can't pass.
- **Logs:** `~/Library/Application Support/Navaja/logs/`.
- **Recording:** Cmd+Shift+5 > Record Entire Screen. QuickTime Player steps through a paused recording frame by frame with the arrow keys.

### Ubuntu 24.04 GNOME (Wayland and X11)

- **Machine:** Ubuntu 24.04 desktop, which ships GNOME with the Ubuntu AppIndicators extension on. Record `lsb_release -d`, `gnome-shell --version`, and the GPU and its driver (`lspci -k | grep -A3 -E 'VGA|3D'`).
- **Install:**

  ```sh
  sudo apt-get update
  sudo apt-get install -y build-essential curl file git libwebkit2gtk-4.1-dev \
    libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev \
    x11-utils obs-studio mpv
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

  The build packages are those `ci.yml` installs, plus curl and git. `x11-utils` gives `xwininfo`; OBS records, and mpv steps through a recording frame by frame with the `.` key. Open a new terminal after rustup's installer, so that `cargo` is on the path. Install Node.js 24 LTS from nodejs.org, or with nvm (`nvm install 24`).
- **Build,** in a terminal at the repository root:

  ```sh
  git switch main && git pull --ff-only && git rev-parse --short HEAD
  rustup toolchain install       # the toolchain rust-toolchain.toml pins
  corepack enable pnpm           # with sudo if Node is installed for all users
  pnpm install --frozen-lockfile
  pnpm tauri build --no-bundle   # writes target/release/navaja
  export NAVAJA="$PWD/target/release/navaja"
  ```

- **The tray host:** `gnome-extensions list --enabled | grep ubuntu-appindicators` must print `ubuntu-appindicators@ubuntu.com`.
- **Sessions:** at the login screen, pick the user, then the gear button: "Ubuntu" is the Wayland session, "Ubuntu on Xorg" the X11 one. After signing in, `echo $XDG_SESSION_TYPE` prints `wayland` or `x11`. In the Wayland session, `echo $GDK_BACKEND` must print nothing. Do every Ubuntu check once in each session. After each switch, set `NAVAJA` again in a new terminal ("After signing in again", above).
- **Logs:** `~/.config/navaja/logs/`.
- **Recording:** in OBS, set 60 fps and add a "Screen Capture (PipeWire)" source in the Wayland session, or "Screen Capture (XSHM)" in the X11 one.

### Fedora GNOME (no tray)

- **Machine:** Fedora Workstation, whose GNOME has no tray host: it ships no AppIndicator extension. Use the Wayland session. Record `cat /etc/fedora-release`, `gnome-shell --version` and the GPU, as on Ubuntu.
- **Install,** as Tauri documents for Fedora, then rustup and Node as on Ubuntu:

  ```sh
  sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
    libappindicator-gtk3-devel librsvg2-devel libxdo-devel git
  sudo dnf group install c-development
  ```

- **Build** as on Ubuntu; the binary is `target/release/navaja`, and `export NAVAJA="$PWD/target/release/navaja"`.
- **No tray host:** `gnome-extensions list --enabled` must list no appindicator extension. The appindicator library is installed, so Navaja creates its tray icon, but no host shows it.
- **Logs:** `~/.config/navaja/logs/`.

### KDE Plasma 6 Wayland

- **Machine:** Plasma 6, in its Wayland session ("Plasma (Wayland)" at the login screen). KDE neon User Edition (based on Ubuntu 24.04) or Fedora KDE Plasma Desktop both ship it; Kubuntu 24.04 ships Plasma 5.27 and does not count. `plasmashell --version` must print 6.x, and `echo $XDG_SESSION_TYPE` must print `wayland`.
- **Install and build:** on KDE neon as on Ubuntu, on Fedora KDE as on Fedora. `export NAVAJA="$PWD/target/release/navaja"`.
- **The tray:** Navaja's icon may land in the System Tray's ^ popup. If it does, set System Tray Settings > Entries > Navaja to "Always shown".
- **Klipper** is the Clipboard entry in the System Tray. Meta+V opens its history at the pointer.
- **Logs:** `~/.config/navaja/logs/`.

## The checks

In the steps, "start Navaja" means: on Windows `Start-Process $exe`, on macOS `open target/release/bundle/macos/Navaja.app`, on Linux `"$NAVAJA" &` in the terminal. "Quit from the tray" means the tray menu's Quit Navaja; on Fedora, which has no tray, close the window instead.

### 1. Starts hidden, shows without a flash

Recorded on Windows 11, Ubuntu Wayland, Ubuntu X11 and macOS 26, which the M2a Exit names. On Fedora and KDE, watch steps 2 and 3 without recording, and record what you saw.

1. Set the OS to dark mode: Windows, Settings > Personalization > Colors > Choose your mode; macOS, System Settings > Appearance; Ubuntu, Settings > Appearance > Style. Navaja's Settings > Theme stays at "Same as the system", the default.
2. **Cold start:** right after signing in, before Navaja has run in this session, set `$exe` or `NAVAJA` again ("After signing in again", above), then start recording. Start Navaja the way a person would: on Windows, double-click `target\release\navaja.exe` in Explorer; on macOS, `open target/release/bundle/macos/Navaja.app`; on Linux, `"$NAVAJA"` in a terminal. Stop recording once the shell shows.
3. **Warm start:** quit from the tray, then record another start the same way.
4. **Theme override:** set the OS to light mode. In Navaja, set Settings > Theme to Dark, quit from the tray, and record a start. Then set Theme back to "Same as the system" and the OS back to dark mode.
5. **A first launch with a tool:** with Navaja quit, record a start with `--tool uuid`: on Windows, Win+R and `"<the path in $exe>" --tool uuid`; on macOS, `open target/release/bundle/macos/Navaja.app --args --tool uuid`, which works because Navaja is not running; on Linux, `"$NAVAJA" --tool uuid`.
6. On Ubuntu Wayland, while the window shows, run `xwininfo -root -tree | grep -c '"Navaja"'`. It must print 0, which shows Navaja runs on Wayland itself; a match means it runs under XWayland, and the Wayland run does not count. On X11 the same command lists the window.
7. Step through each recording frame by frame, from the last frame without a Navaja window to the first frames with one.
8. Read the log. Each start wrote a `starting` line. Search it for `did not report ready`: on Windows, `Select-String -Path "$env:APPDATA\Navaja\logs\navaja.*.log" -Pattern 'starting','did not report ready'`; on macOS and Linux, `grep -h -e starting -e 'did not report ready'` on the log files.
9. On Linux with an NVIDIA GPU, if the content stays blank, record that, then try `WEBKIT_DISABLE_DMABUF_RENDERER=1 "$NAVAJA"` and record that as a separate finding.

**Pass,** in all four recordings:
- Until the window appears there is no Navaja window and no console window, and on Windows no taskbar button. On macOS a Dock icon that bounces at launch is expected.
- The first frame that shows any part of the window already shows the rendered shell: the Tools sidebar and the page's heading ("Navaja", or "UUID generator" in step 5), in the expected theme (dark in steps 2 to 4).
- No frame shows a blank white, grey or black content area, a title bar around an empty area, or unstyled text. The window appears once, at one size and place.
- The log has a `starting` line for each start and no "front end did not report ready; showing the window anyway".

Attach the frames around the window's first appearance.

### 2. A second launch: `--tool` and `--toggle`

On all six. Start Navaja, wait for the window, and click Home. Place the terminal so that it overlaps part of Navaja's window.

**The probe** follows each case below. It launches Navaja a second time, with `--tool uuid`, then checks five things without a click:
- Run the second launch: on Windows, `$p = Start-Process $exe -ArgumentList '--tool','uuid' -PassThru -Wait; $p.ExitCode`; on macOS and Linux, `"$NAVAJA" --tool uuid; echo $?`. It must print 0.
- The UUID generator shows.
- Navaja's window is in front. On macOS, the menu bar's application menu reads Navaja, not Terminal, and the window's traffic lights are coloured, not grey.
- Press Ctrl+K (Cmd+K on macOS): the command palette must open in Navaja. Press Esc.
- One Navaja is left: on Windows, `(Get-Process navaja).Count` prints 1; on macOS, `pgrep -x "$(basename "$NAVAJA")" | wc -l` prints 1; on Linux, `pgrep -x navaja | wc -l` prints 1.

Then click Home before the next case.

1. **Hidden.** Hide the window, then check that it is gone.
   - On Windows, click Navaja's title bar, then left-click its tray icon.
   - Elsewhere, run `sleep 3; "$NAVAJA" --toggle` and click inside Navaja within the 3 s: `--toggle` hides only a window that is shown and focused.
   - If the window is still shown, record that. On Windows, hide it with `Start-Sleep 3; Start-Process $exe -ArgumentList '--toggle'` instead, clicking into Navaja within the 3 s. Elsewhere, try again, clicking into Navaja just before the 3 s are up. Run the probe only once the window is gone.
   - Run the probe. Also note whether Home shows for a frame before the UUID generator.
2. **Minimized.** Minimize Navaja: on Windows its title-bar button, on macOS Cmd+M, on GNOME Super+H, on KDE its title-bar button. Click the terminal, and run the probe.
3. **Covered.** Click the terminal, so that it is in front and Navaja is visible behind it but not focused, and run the probe.
4. **Early.** Quit Navaja from the tray. Start recording the screen with the terminal in view, as in check 1 (where OBS is not installed, the desktop's own screen recorder will do). Then start Navaja and hand it a tool before its front end is ready:
   - Windows: `Start-Process $exe; Start-Sleep -Milliseconds 300; $p = Start-Process $exe -ArgumentList '--tool','uuid' -PassThru -Wait; $p.ExitCode`
   - macOS: `open target/release/bundle/macos/Navaja.app; sleep 0.3; "$NAVAJA" --tool uuid; echo $?`
   - Linux: `"$NAVAJA" & sleep 0.3; "$NAVAJA" --tool uuid; echo $?`

   Nothing in the log says whether the hand-off came before the front end was ready, so the recording decides whether the try counts. Step through it: the try counts only when the window's first frame comes after the terminal shows the second launch's exit code. The window stays hidden until the front end is ready, so the hand-off was then sent before that, and the running Navaja kept the tool until then.
   - If the window was already showing when the exit code appeared, the try does not count: retry with a shorter pause, such as 0.1 s.
   - If the second launch does not return, it started before the first one and became the running Navaja: the try does not count. Quit it, and retry with a 0.5 s pause.

   In a try that counts, the window appears once and opens the UUID generator without another launch, and the second launch prints 0. Note whether Home shows for a frame first: Navaja shows the window, then opens the tool, as in case 1. Then check the focus and the process count as in the probe.
5. **`--toggle` on a window that is not focused.** Click the terminal, as in case 3, and run `"$NAVAJA" --toggle` (on Windows, `Start-Process $exe -ArgumentList '--toggle'`). Navaja must come to the front, not hide. Then hide it as in case 1, and run the same command again: the window must show.
6. **Windows only, the way a shortcut starts it.** Cover Navaja with another window, press Win+R, and enter `"<the path in $exe>" --tool uuid`. Check the probe's points, apart from the exit code.
7. Read the log: it must have no `could not handle a second launch` line.

For each case, record whether the tool opened, whether the window came to the front, whether it took focus, the exit code and the process count.

**Pass:**
- **Windows 11, Ubuntu X11, macOS 26:** every case passes in full. Signs of a failure: the taskbar button flashes instead of the window coming forward (Windows); GNOME shows "“Navaja” is ready" instead of raising the window (X11); Ctrl+K or Cmd+K reaches the terminal.
- **Ubuntu Wayland, Fedora, KDE:** in every case the tool opens, the second launch exits with 0 and one Navaja is left, and in cases 1, 4 and 5 a hidden window shows again. Whether the window came to the front or took focus, and whether a minimized one came back, is recorded only: on Wayland that is roadmap M2b, item 8. GNOME is expected to show "“Navaja” is ready", and KDE to highlight Navaja's taskbar entry.

### 3. The tray

On Windows 11, Ubuntu Wayland, Ubuntu X11, KDE and macOS 26. Fedora has its own steps below.

1. Start Navaja. The icon appears within a few seconds: on Windows in the taskbar's notification area, where hovering it shows the tooltip "Navaja"; on macOS in the menu bar, left of the system's items; on GNOME at the right of the top bar; on KDE in the System Tray.
   - On macOS, if it is missing, quit other menu-bar apps (the notch hides items that don't fit), and check that System Settings > Menu Bar does not hide it.
2. Open the menu: right-click on Windows, any click elsewhere. It must read, in order: Open Navaja, Search tools…, a separator, Quit Navaja. Take a screenshot of the open menu.
3. Choose Search tools…: the window shows with the command palette open and its search box focused. Type `uid`: the UUID generator is listed first. Press Esc.
4. Open a page other than Home, such as Settings. Hide the window: on Windows, click Navaja's title bar, then left-click the icon; elsewhere, `sleep 3; "$NAVAJA" --toggle`, clicking inside Navaja within the 3 s. Check that the window is gone, then choose Open Navaja: the window shows, on Settings.
5. Minimize the window, then choose Open Navaja: the window is restored. On Ubuntu Wayland and KDE, whether it comes back is recorded only (roadmap M2b, item 8): xdg-shell has no request that restores a minimized window, and the tray gets no activation token.
6. **Windows only, the left click.** With Navaja focused, a left click on the icon hides the window. Another left click shows it, and Ctrl+K then opens Navaja's palette without a click. With Navaja covered by another window for more than half a second, a left click brings it to the front instead of hiding it. With Navaja minimized, a left click restores it.
7. **The icon on light and dark.** On Windows, repeat steps 1, 2 and 6 with the taskbar light and dark (Settings > Personalization > Colors > Choose your mode). The placeholder's light blades barely show on a light taskbar; the final art is roadmap M2b, item 3. On macOS, in both Light and Dark (System Settings > Appearance), the icon is grey like the clock's text.
8. Hide the window as in step 4, then choose Quit Navaja. The process ends (`Get-Process navaja` fails on Windows; `pgrep` prints nothing elsewhere), and the icon goes. The log's last line is `quit` with `from="tray"`, and the log has no `no tray icon` warning.

On Ubuntu Wayland and KDE, Open Navaja and Search tools… may show the window without focus, and may leave a minimized window minimized (step 5). A hidden window must show, and the action must happen; focus is recorded only. If the window has no keyboard focus there, click the palette's search box before typing in step 3. On Ubuntu X11, the window must also take focus: Ctrl+K reaches Navaja without a click.

**Pass:** every step behaves as written, on Windows on both taskbar colours, and on macOS in both appearances. On Ubuntu Wayland and KDE, focus and step 5 are recorded only, as above.

**Fedora GNOME (no tray):**
1. Start Navaja. No icon shows in the top bar, and the window shows as usual. Record any `no tray icon` line in the log.
2. Hide the window with `sleep 3; "$NAVAJA" --toggle`, clicking inside Navaja within the 3 s, and check that it is gone. Run `"$NAVAJA"`: the window shows again.
3. Close the window: the app quits, and `pgrep -x navaja` prints nothing.

**Pass (Fedora):** the app works without a tray, and a second launch brings a hidden window back.

Optional, on Ubuntu: `gnome-extensions disable ubuntu-appindicators@ubuntu.com`, then start Navaja. There must be no icon, and the window must show. Quit by closing the window, or with Ctrl+C in the terminal; `pgrep -x navaja` must print nothing. Then `gnome-extensions enable ubuntu-appindicators@ubuntu.com`.

### 4. Close and quit

On all six. In M2a every one of these ends the app.

1. Start Navaja, and click the window's close button (on macOS the red one). The process ends: on Windows `Get-Process navaja` fails; elsewhere `pgrep` prints nothing. The tray icon goes. The log gets no `quit` line for this.
2. Start Navaja, and close the window from the keyboard: Alt+F4 on Windows and Linux, Cmd+W on macOS. The same as step 1.
3. macOS only: start Navaja, and press Cmd+Q. The same as step 1.
4. Start Navaja, and choose Quit Navaja from the tray while the window shows. The process ends, the icon goes, and the log's last line is `quit` with `from="tray"`. Check 3, step 8 covers the same with the window hidden.

**Pass:** each path ends the process and removes the tray icon, and only the tray's Quit writes `quit`.

### 5. Keyboard only

On all six. Start Navaja, and from then on don't touch the mouse.

1. Press Tab repeatedly. Focus moves through the sidebar: Home, the "Filter tools" box, the shortcut button next to it, the tool links, Settings and About. You can always see where focus is.
2. Tab back to "Filter tools" and type `uid`: the list shows the UUID generator. Tab to it and press Enter: the tool opens.
3. Tab through the tool: Version, How many, Format, Upper case, Generate, the output, and Copy. Change Version from the keyboard: the arrow keys, or Space to open the list first. Press Enter on Generate: a new UUID shows. Press Enter on Copy: its label changes to "Copied".
4. Press Ctrl+K (Cmd+K on macOS): the palette opens with its search box focused. Type `uid`, move through the results with the arrow keys, and press Enter: the tool opens. Press Ctrl+K again, then Esc: the palette closes, and focus goes back to where it was.
5. Tab to Settings and press Enter. Tab to Theme and pick Light, then Dark, then "Same as the system" with the keyboard: the window follows each at once.

**Pass:** every step works without the mouse, focus is visible at every step, and Tab never gets stuck.

### 6. About's link and the logs folder

On all six. No automated test opens these, because the offline checks must never start a browser or a file manager.

1. Open About. The version shows. Click `github.com/joaovitorpina/Navaja`: the default browser opens the repository's page. Navaja shows no error.
2. Open Settings, and click "Open logs folder": the file manager opens the logs folder, `%APPDATA%\Navaja\logs` in Explorer, `~/Library/Application Support/Navaja/logs` in Finder, or `~/.config/navaja/logs` in Files or Dolphin.

**Pass:** both open outside Navaja, and Navaja shows no "could not open" message.

### 7. Elevated or root start

- **Windows 11:** quit Navaja. Right-click `target\release\navaja.exe` in Explorer, choose Run as administrator, and accept the prompt. The banner "Navaja is running with administrator or root rights. It doesn’t need them." shows above the page. Quit from the tray, then start Navaja normally: no banner.
- **Ubuntu (either session), Fedora, KDE and macOS:** quit Navaja, then run `sudo "$NAVAJA"; echo $?`. It must print "Error: Navaja does not need root and will not run as root. Start it as your own user, or pass --allow-root.", then a non-zero status, and no window opens. Don't try `--allow-root` on macOS: single instance there uses a socket in `/tmp` that every user shares (roadmap M2b, item 9).
- **Optional, Ubuntu X11, record only:** `sudo env HOME=/tmp/navaja-root DISPLAY="$DISPLAY" XAUTHORITY="$XAUTHORITY" "$NAVAJA" --allow-root`. Record whether it starts and shows the banner. `HOME` points at a throwaway folder, so no root-owned file lands in yours. Close the window, then `sudo rm -rf /tmp/navaja-root`.

**Pass:** the banner shows only for the elevated start on Windows, and every root start without `--allow-root` is refused with that message.

### 8. Klipper

On KDE Plasma 6, in the Wayland session. KWin offers Wayland's data-control protocol, so Navaja copies over Wayland there, which no automated check reads. Each step uses a new UUID.

1. Control: in Kate or KWrite, type a word nobody else would copy, select it and press Ctrl+C. Press Meta+V: the word must be listed. If it is not, Klipper is not keeping history, and the check can't be answered here.
2. Run `"$NAVAJA" --tool uuid`. Press the output's Copy button: its label must change to "Copied". Paste in Kate: that UUID must appear. Press Meta+V: it must not be listed. Keep Navaja running.
3. Press Generate, triple-click the new UUID in the output, and press Ctrl+C. Paste in Kate: the new UUID must appear. Press Meta+V: it must not be listed.
4. Press Generate again and right-click the new UUID. No menu is expected: Navaja's guard turns the native context menu off outside text fields. Record what happened. If a menu opens and offers Copy, choose it, paste in Kate, and check that Meta+V does not list it.
5. Quit Navaja from the tray, then press Meta+V: no UUID copied through Navaja may be listed.
6. Lookup control: in Kate, select the UUID pasted in step 3 and press Ctrl+C. Press Meta+V: it must now be listed.

**Pass:** steps 1 and 6 are listed, steps 2 and 3 paste the right UUID, and no UUID copied through Navaja is ever listed. **Fail:** Klipper lists a UUID copied through Navaja. **Inconclusive,** so redo it: step 1 or 6 is not listed, or a paste does not give back the UUID.

Optional, record only: the same steps in the "Plasma (X11)" session, if the login screen offers one. There Navaja copies over X11, and step 5 matters most: at exit, the copied text is handed to Klipper, which must keep honouring the marker.

## M2a results

Fill in one row per check and desktop. Result is pass, fail or inconclusive, or "recorded" for what the Wayland carve-out only records. Session means the session and anything that matters for the check, such as Wayland or X11, the GPU, the display scale, or cold or warm.

| Desktop | Check | Date | Commit | OS build | Session | Result | Notes |
|---|---|---|---|---|---|---|---|
| Windows 11 | 1. Starts hidden | | | | | | |
| Windows 11 | 2. Second launch | | | | | | |
| Windows 11 | 3. Tray | | | | | | |
| Windows 11 | 4. Close and quit | | | | | | |
| Windows 11 | 5. Keyboard only | | | | | | |
| Windows 11 | 6. Link and logs folder | | | | | | |
| Windows 11 | 7. Elevated start | | | | | | |
| Windows 11 | Clipboard history (spikes.md) | | | | | | |
| Windows 11 | S2.3 scales (spikes.md) | | | | | | |
| Ubuntu Wayland | 1. Starts hidden | | | | | | |
| Ubuntu Wayland | 2. Second launch | | | | | | |
| Ubuntu Wayland | 3. Tray | | | | | | |
| Ubuntu Wayland | 4. Close and quit | | | | | | |
| Ubuntu Wayland | 5. Keyboard only | | | | | | |
| Ubuntu Wayland | 6. Link and logs folder | | | | | | |
| Ubuntu Wayland | 7. Root start | | | | | | |
| Ubuntu X11 | 1. Starts hidden | | | | | | |
| Ubuntu X11 | 2. Second launch | | | | | | |
| Ubuntu X11 | 3. Tray | | | | | | |
| Ubuntu X11 | 4. Close and quit | | | | | | |
| Ubuntu X11 | 5. Keyboard only | | | | | | |
| Ubuntu X11 | 6. Link and logs folder | | | | | | |
| Fedora GNOME | 1. Starts hidden (watched) | | | | | | |
| Fedora GNOME | 2. Second launch | | | | | | |
| Fedora GNOME | 3. No tray | | | | | | |
| Fedora GNOME | 4. Close and quit | | | | | | |
| Fedora GNOME | 5. Keyboard only | | | | | | |
| Fedora GNOME | 6. Link and logs folder | | | | | | |
| Fedora GNOME | 7. Root start | | | | | | |
| KDE Plasma 6 | 1. Starts hidden (watched) | | | | | | |
| KDE Plasma 6 | 2. Second launch | | | | | | |
| KDE Plasma 6 | 3. Tray | | | | | | |
| KDE Plasma 6 | 4. Close and quit | | | | | | |
| KDE Plasma 6 | 5. Keyboard only | | | | | | |
| KDE Plasma 6 | 6. Link and logs folder | | | | | | |
| KDE Plasma 6 | 7. Root start | | | | | | |
| KDE Plasma 6 | 8. Klipper | | | | | | |
| macOS 26 | 1. Starts hidden | | | | | | |
| macOS 26 | 2. Second launch | | | | | | |
| macOS 26 | 3. Tray | | | | | | |
| macOS 26 | 4. Close and quit | | | | | | |
| macOS 26 | 5. Keyboard only | | | | | | |
| macOS 26 | 6. Link and logs folder | | | | | | |
| macOS 26 | 7. Root start | | | | | | |
| any | S2.1, RustRover (spikes.md) | | | | | | |
