# ADR 0003: The webview's network: a dead proxy everywhere, macOS 14, and what is disclosed

- **Status:** Accepted
- **Date:** 2026-10-03
- **Decider:** João Vitor Pina

## Context

Spike S2.7 asks whether any process of Navaja's tree sends a packet off the machine, idle or in use. Its PASS is "a VM NIC capture shows no packets from the process tree, idle or in use". Its fallback is "browser args or policies; anything left over is disclosed verbatim in privacy.md". [spikes.md](../spikes.md) S2.7 has the method and every run.

The run of record before this decision, [37156112676](https://github.com/joaovitorpina/Navaja/actions/runs/37156112676) on `0866f1a`, failed on Windows and macOS. Linux passed: its webview already sent its web traffic to a dead proxy. [37158337262](https://github.com/joaovitorpina/Navaja/actions/runs/37158337262), on `90bbf7f`, found the same.

- **Windows, from WebView2's own processes:**
  - its network service reached `config.edge.skype.com` and `edge.microsoft.com`, with its own DNS queries, and looked up `wpad` through the Windows DNS client;
  - its component updater had BITS download 8 files from `msedge.b.tlu.dl.delivery.mp.microsoft.com`, 4.7 MB in all;
  - in use, it looked up the egress canary's navigation host before the guard denied the navigation.
- **macOS, from WebKit:** in use, WebKit looked up the same canary host through `mDNSResponder`.
- **Outside the tree, woken by the webview:**
  - macOS: `webprivacyd` started with the app and fetched from `wps.apple.com`, and `trustd` checked that server's certificate with `ocsp2.apple.com` for it;
  - macOS: the system's Safe Browsing service talked to Apple in each run checked; it started 4 s after the app in one of them ([37151706483](https://github.com/joaovitorpina/Navaja/actions/runs/37151706483)), but the VM also starts it by itself;
  - Windows: Windows' Microsoft-account sign-in service (`wlidsvc`) reached `login.live.com` within seconds of the app's first start, in every run checked. No log names it as acting for the app.

## Decision

### 1. A dead proxy on every OS

Outside dev mode, the webview's web traffic goes to `http://127.0.0.1:9`, a closed privileged loopback port (`DEAD_PROXY` in `app/src-tauri/src/window.rs`). An engine hands a proxied request's host name to the proxy instead of resolving it, so nothing is looked up and nothing leaves the machine.

- **Linux:** as before, wry sets it on WebKitGTK's `WebsiteDataManager`.
- **Windows:** WebView2's `--proxy-server`, which Microsoft's list of [WebView2 browser flags](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/webview-features-flags) describes as a proxy that overrides the system's settings, for HTTP and HTTPS.
- **macOS:** Tauri's `macos-proxy` feature (wry's `mac-proxy`) sets [`WKWebsiteDataStore.proxyConfigurations`](https://developer.apple.com/documentation/webkit/wkwebsitedatastore/proxyconfigurations-cdc1) to an HTTP CONNECT proxy.

The app's own pages and IPC never reach the proxy: custom schemes on Linux and macOS, and `http://tauri.localhost` and `http://ipc.localhost` on Windows, which wry serves from the app. The end-to-end suite passes with the proxy on all three OSes.

### 2. macOS 14 or later

`proxyConfigurations` is a macOS 14 API, and wry's `mac-proxy` links `nw_proxy_config_create_http_connect` from Network.framework, which macOS 14 introduced. The maintainer decided that Navaja requires macOS 14 or later, rather than ship macOS without the proxy.

- `tauri.conf.json` sets `bundle.macOS.minimumSystemVersion` to `"14.0"`. Tauri's bundler writes it to the bundle's `Info.plist` as `LSMinimumSystemVersion`, as its config reference says (tauri-utils 2.10.1). No CI job builds a bundle yet (`ci.yml` and `spikes.yml` build with `--no-bundle`), so that rests on Tauri's documentation until `bundle.yml` checks the `Info.plist` (roadmap M2b, item 4). What CI does show: tauri-build 2.7.1 passes the setting on as `MACOSX_DEPLOYMENT_TARGET` outside dev (`src/lib.rs`, lines 780 to 783), and the built files declare 14.0 (below).
- `.cargo/config.toml` sets `MACOSX_DEPLOYMENT_TARGET` to `14.0` for every cargo build. In a `tauri build`, tauri-build sets it from the bundle setting, but only for the app's own crate; its dependencies, clippy, nextest and dev builds would use rustc's defaults, 11.0 on arm64 and 10.12 on Intel.
- `scripts/check-macos-target.sh` reads the version back from what each macOS build in CI produced, and refuses a `minimumSystemVersion` below 14.0. In ci.yml it covers clippy's build scripts and proc macros, nextest's test binaries, the Intel step's C objects and the app; in spikes.yml, the app S2.7 runs.
- macOS caches in CI are keyed apart. Cargo rebuilds a cached crate for a new deployment target only if the crate reads it, so main's cache kept 359 proc macros and build scripts built for 11.0 ([37160662985](https://github.com/joaovitorpina/Navaja/actions/runs/37160662985)).

### 3. Browser arguments and preferences

- **Windows: no browser arguments of Navaja's own.** wry 0.57.0 adds `--proxy-server` after its defaults (`--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection`, in `create_environment` of its `src/webview2/mod.rs`), and with that nothing left the process tree ([37160662987](https://github.com/joaovitorpina/Navaja/actions/runs/37160662987)). The component updater's check stopped at the proxy, so BITS had nothing to download, and nothing looked `wpad` up. So these were not needed:
  - `--disable-component-update` and `--disable-background-networking`, Chromium switches ([`chrome_switches.h`](https://github.com/chromium/chromium/blob/af1f5c41f0cb8eb44adb549f17e176cc0df1aa54/chrome/common/chrome_switches.h#L234-L245)) that Microsoft's flags list does not name.
  - Tried and dropped: turning off WebView2's single sign-on with the Windows account (`msSingleSignOnOSForPrimaryAccountIsShared`, `msSingleSignOnForInPrivateWebView2` and `msAllowAmbientAuthInPrivateWebView2`, all on Microsoft's list), to stop `wlidsvc`'s contact (section 4). With them off it still came, at +7.3 s ([37162299733](https://github.com/joaovitorpina/Navaja/actions/runs/37162299733)), so Navaja does not pass them.

  Microsoft's flags page says apps "in production shouldn't use WebView2 browser flags"; Navaja relies on one, `--proxy-server`, through wry. An argument Navaja passes later replaces all of wry's, so it must repeat wry's defaults and `--proxy-server`, in one place in `window.rs` (architecture §5).

- **macOS:** `WKPreferences.fraudulentWebsiteWarningEnabled` is off ([public API](https://developer.apple.com/documentation/webkit/wkpreferences/isfraudulentwebsitewarningenabled), macOS 10.15 and later, on by default), next to WebRTC (`platform/macos.rs`). Apple documents it as whether the web view warns about suspected fraudulent content; the webview only shows Navaja's own pages, so it needs no such warnings. The aim is that WebKit stops asking the system's Safe Browsing service about pages. **CI has not shown that it does.**
  - In every run since this change, the VM had started the service by itself before the app first ran: as the baseline began in [37162299733](https://github.com/joaovitorpina/Navaja/actions/runs/37162299733), 8.5 min before it in [37163502511](https://github.com/joaovitorpina/Navaja/actions/runs/37163502511), 38 s into it in [37164320626](https://github.com/joaovitorpina/Navaja/actions/runs/37164320626), 4 min before it in [37165352245](https://github.com/joaovitorpina/Navaja/actions/runs/37165352245), and 1.5 min before it in [37167561422](https://github.com/joaovitorpina/Navaja/actions/runs/37167561422).
  - S2.7 counts a WebKit daemon as the app's only if it starts within 15 s after the app. It cannot tell the traffic of a daemon that was already running apart from the app's, so it left the service out, and the service's packets in the app's phases went unchecked: 35 in idle and 6 in use in 37162299733, 44 in idle in 37164320626, 39 in idle in 37165352245, 40 in idle in 37167561422, none in 37163502511.
  - Before this change, the service started 4 s after the app in one run ([37151706483](https://github.com/joaovitorpina/Navaja/actions/runs/37151706483)); no run since has shown whether the app still wakes it.

### 4. What is disclosed

The fallback's "disclosed verbatim" part: what system services that the webview wakes still send, which no setting reaches. S2.7 reports it as disclosed, in a summary row of its own, instead of failing on it; anything else, or the same traffic in another OS or phase, still fails. Nothing from the process tree itself is on the list. The entries are in `scripts/spikes/s2-7-disclosed.tsv`, one per (OS, phases, service, hosts), and each names only the phases its traffic was seen in:

| OS | Phases | Service | Hosts | Purpose |
|---|---|---|---|---|
| macOS | idle | `webprivacyd` | `wps.apple.com` | what `webprivacyd` fetches when WebKit asks it for its storage-access quirk lists |
| macOS | idle | `webprivacyd`, through `trustd` | `ocsp2.apple.com` (CNAME `ocsp2.g.aaplimg.com`) | the certificate check of that connection |
| Windows | idle | `svchost.exe [wlidsvc]` | `login.live.com` | Windows' Microsoft-account sign-in |

How S2.7 matches a packet to an entry: the packet's service is the entry's, and every name the packet is tied to is one of the entry's hosts.

- A DNS packet is tied to the name it asks for.
- Any other packet is tied, through its connection, to the lookups that its own process made before the connection began and that gave its remote address. On macOS those are the lookups mDNSResponder made for the same process (pktap's eproc, name and PID); on Windows, the process's own lookups in the DNS client's log (event 3008).
- So a shared address counts only for what that process looked up. Apple's edge servers answer for many names: in 37164320626, `17.253.5.143` answered for `ocsp2.apple.com`, `valid.apple.com` and `updates.cdn-apple.com`, among others. Until `6ab9fc2`, any name a captured answer gave for the address counted, so the `ocsp2` entry would have covered a connection of `webprivacyd`'s to another Apple service there.
- A connection with no such lookup is tied to nothing, and fails.
- The limit: on macOS, a name looked up stands for the names whose CNAMEs lead to it, as far as the job's captures show, because mDNSResponder may ask only for a CNAME's target. That is why the `ocsp2` entry lists `ocsp2.g.aaplimg.com` too. A lookup of that name made for `webprivacyd` would count as the entry's even if some other name, whose CNAME no capture saw, had led to it.

What was tried first:

- **`webprivacyd`.** WebKit asks it for its storage-access quirk lists unconditionally, in `WebProcessPool`'s constructor ([`WebProcessPool.cpp`](https://github.com/WebKit/WebKit/blob/10740b3a1fa50537a0fbea349eb9130ac609d852/Source/WebKit/UIProcess/WebProcessPool.cpp#L345-L372), under `ENABLE(ADVANCED_PRIVACY_PROTECTIONS)`), and the request ([`WebPrivacyHelpers.mm`](https://github.com/WebKit/WebKit/blob/62300d55e70a29e7555d266eca4c8ff4d4e5cf1e/Source/WebKit/Platform/cocoa/WebPrivacyHelpers.mm#L270-L342)) checks only that the WebPrivacy framework is there. No `WKWebView`, `WKWebsiteDataStore` or `WKPreferences` setting, public or private, gates it, so there is nothing to try in the app. Whether the daemon then downloads depends on its own copy: on these fresh VMs, it did every time.
- **`trustd`** checks the certificate of `webprivacyd`'s connection; it goes with the entry above.
- **`wlidsvc`**, Windows' Microsoft-account sign-in service, reached `login.live.com` within seconds of the app's first start in every Windows run: 1 to 3 s after WebView2 started before the proxy, and 3.1 to 7.3 s into the idle phase with it. WAM's account provider started next to it. No log names the app or WebView2 as asking, and the service also talks without the app:
  - in the baselines of 37154768748 (+239 s), [37160662987](https://github.com/joaovitorpina/Navaja/actions/runs/37160662987) (+124.5 s), [37162299733](https://github.com/joaovitorpina/Navaja/actions/runs/37162299733) (+206.8 s) and [37164320626](https://github.com/joaovitorpina/Navaja/actions/runs/37164320626) (+203.5 s). In the last three the service started by itself and looked `login.live.com` up 0.1 to 0.2 s later;
  - later in idle phases, next to Windows Update's medic service.

  wry leaves WebView2's `AllowSingleSignOnUsingOSPrimaryAccount` at its default, off ([Microsoft's reference](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2environmentoptions)), and turning off the single-sign-on features changed nothing (section 3). No other WebView2 option or switch is known to reach it. S2.7 counts its connections, and those of the account provider, in the 15 s after an app start as the app's, and discloses this one.

**The text for `privacy.md`**, word for word (roadmap M6, item 6):

> **macOS: WebKit's fixes for particular websites.** When Navaja starts, macOS's WebKit asks the system service `webprivacyd` for its storage-access quirk lists: fixes for particular websites, for how they reach their cookies across sites. The service may then fetch data from Apple (`wps.apple.com`), and macOS's `trustd` checks that server's certificate with Apple (`ocsp2.apple.com`). These are macOS services, not part of Navaja, and Navaja cannot turn them off. Other apps that use WebKit, Safari among them, ask the same service.
>
> **Windows: the Microsoft account sign-in service.** Within seconds of Navaja starting, Windows' Microsoft account sign-in service (`wlidsvc`) can contact Microsoft's sign-in server, `login.live.com`. In our tests it did so at the first start of Navaja in each session. Windows also runs this service when Navaja is not running, and no Windows log names Navaja or its webview (Microsoft Edge WebView2) as the cause. It is part of Windows, and Navaja cannot turn it off. Navaja itself signs in to nothing.

## Evidence

- **The dead proxy alone**, [37160662987](https://github.com/joaovitorpina/Navaja/actions/runs/37160662987) on `a5c0f31`:
  - Windows passed idle and in use. The tree's only connections were to loopback, 3 (idle) and 6 (in use) of them to `127.0.0.1:9`; no DNS query and no BITS job. The `wlidsvc` contact with `login.live.com` stayed, at +3.2 s, outside the tree.
  - Linux passed, with 1 connection to the dead proxy in use.
  - macOS in use was clean: the canary lookup was gone, and WebKit's Networking process, for `navaja`, reached the proxy on lo0. Idle still had `webprivacyd`: 104 packets with `wps.apple.com`, and 33 of `trustd`'s certificate check for it. The baseline failed: the VM started the Safe Browsing service 8 s into it, with no app, and the check of that time counted it, which is why a daemon now counts only when it starts within 15 s after the app.
- **ci.yml**, [37162299734](https://github.com/joaovitorpina/Navaja/actions/runs/37162299734) on `a969a4b`: green on all three OSes, the end-to-end suite included, with the proxy (and the single-sign-on switches below, since dropped). On macOS, with a fresh cache, all 2,390 Mach-O files and archive members under `target/debug`, and the 2 under `target/x86_64-apple-darwin`, declared macOS 14.0, while the runner's environment set no `MACOSX_DEPLOYMENT_TARGET`: the plain cargo builds took it from `.cargo/config.toml`. The run before, [37160662985](https://github.com/joaovitorpina/Navaja/actions/runs/37160662985), still restored main's cache and found 359 files built for 11.0.
- **The single-sign-on switches**, [37162299733](https://github.com/joaovitorpina/Navaja/actions/runs/37162299733) on `a969a4b`: with them, Windows' tree again sent nothing, but `wlidsvc` still reached `login.live.com`, at +7.3 s; the switches were dropped (`1fac01a`). The same run showed two flaws in the new check, both fixed in the next run: on macOS an empty list of app starts swallowed the daemons' list (`1b8598f`), and a lookup of a CNAME's target, `a1845.dscg2.akamai.net` for `ocsp2.apple.com`, was not tied to its name (`531e307`).
- **The disclosed list, matched by address**: [37163502511](https://github.com/joaovitorpina/Navaja/actions/runs/37163502511) on `ad6580a`, [37164320626](https://github.com/joaovitorpina/Navaja/actions/runs/37164320626) on `ccdea07` (the run of record until `6ab9fc2`) and [37165352245](https://github.com/joaovitorpina/Navaja/actions/runs/37165352245) on `c809ddb` passed on all three OSes, idle and in use, with the same entries (the macOS ones then also named in use), matched through any name the job's DNS answers gave for an address (section 4). `ci.yml` was green on all three OSes on those commits: [37163502497](https://github.com/joaovitorpina/Navaja/actions/runs/37163502497), [37164320631](https://github.com/joaovitorpina/Navaja/actions/runs/37164320631) and [37165352187](https://github.com/joaovitorpina/Navaja/actions/runs/37165352187).
- **The run of record**, [37167561422](https://github.com/joaovitorpina/Navaja/actions/runs/37167561422) on `6ab9fc2`, with each connection tied to its own process's lookups: S2.7 passed on all three OSes, idle and in use, with every control and baseline passing and the suite green. [37167561420](https://github.com/joaovitorpina/Navaja/actions/runs/37167561420), `ci.yml` on the same commit, was green on all three OSes.
  - No packet from the process tree on any OS. On Windows the tree connected only to loopback, 3 (idle) and 6 (in use) times to `127.0.0.1:9`; on Linux 1 connection stopped at the proxy in use; on macOS WebKit's Networking process connected to it on lo0.
  - Disclosed: on macOS in idle, `webprivacyd` (started 1 s after the app) with 105 packets for `wps.apple.com` and 35 for `trustd`'s check with `ocsp2.apple.com`, each connection at an address that a lookup made for `webprivacyd` just before had given; on Windows in idle, 1 connection event of `wlidsvc` to `login.live.com`, at +3.6 s, to the address its own lookup of that name had given. Nothing else, and nothing in use.
  - The Safe Browsing service, started by the VM 1.5 min before the baseline, was left out, and its 40 packets in idle went unchecked (section 3).

## Consequences

- Navaja requires macOS 14 or later. Its binaries declare 14.0, which CI checks, and wry links a function that macOS 14 introduced; no CI job runs it on macOS 13. The bundle's `LSMinimumSystemVersion`, which macOS reads before it opens an app, should say 14.0 too; `bundle.yml` checks that when it lands (roadmap M2b, item 4).
- Navaja passes WebView2 no arguments of its own. One added later replaces all of wry's, so it must repeat wry's defaults (`create_environment` in `src/webview2/mod.rs`) and `--proxy-server`, and a wry or Tauri upgrade must check them again.
- Microsoft may change or remove WebView2's browser flags, `--proxy-server` included. S2.7 runs again in M6 (roadmap, item 7), and `spikes.yml` can be run by hand after a WebView2 or macOS update.
- `privacy.md` (roadmap M6, item 6) carries the disclosed list above word for word. A new entry needs a run that shows the traffic, an attempt to stop it, and a change to this ADR.
