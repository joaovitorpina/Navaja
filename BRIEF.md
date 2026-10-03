# Navaja: project brief for Claude Code

Oct 2, 2026 · @Joao Vitor Pina

## Goal

An open-source, cross-platform desktop toolbox for developers, with all logic in Rust and everything running offline. It is called Navaja, Spanish for pocket knife.

It holds two kinds of tools:

- **Text tools** replace the websites developers paste tokens and payloads into, so a production JWT never leaves the machine.
- **System tools** inspect the local machine, which a website cannot do. This is what sets it apart from existing toolboxes such as DevToys and DevTools-X, which are mostly text utilities.

The space is crowded, so v1 aims for a small, solid and easily extended base, not feature parity.

## v1 scope

Seven tools and a minimal shell.

| Tool | Kind | What it does |
| --- | --- | --- |
| JWT decoder | Text | Decodes header and payload and shows expiry. |
| JSON formatter | Text | Formats, minifies and validates. |
| Base64 | Text | Encodes and decodes. |
| URL encoder | Text | Encodes and decodes. |
| Hash generator | Text | MD5, SHA-1, SHA-256 and SHA-512. |
| UUID generator | Text | Generates one or many. |
| Port inspector | System | Shows what holds a port, explains why, and frees it. |

The shell provides navigation by category, search across tools, and a tray icon.

The icon and visual style follow the name: a folding pocket knife with its tools fanned out. The design must be original and must not copy Swiss Army knife branding, such as the cross-and-shield emblem.

## Port inspector: behaviour

The port inspector has three actions. They were first specced as CLI commands, shown below with `pd` as a placeholder; the app exposes the same actions in its UI.

| Command | What it does |
| --- | --- |
| `pd why <port>` | Explains what holds the port: process, parent chain, working directory, uptime, container or Windows service, and a suggested fix. |
| `pd ls` | Lists all listening ports with process names, common dev ports highlighted. |
| `pd kill <port>` | Kills the whole process tree on the port, after confirmation. `--force` skips the prompt, `--dry-run` only prints. |

Each action returns serializable data, so the UI and a possible later CLI share the same results.

Target output for `pd why 5000`:

```
Port 5000 (TCP) is held by dotnet.exe (PID 18244), running 2h 14m
  started from  C:\projects\expenses\api
  launched by   Code.exe > pwsh.exe > dotnet.exe
  command       dotnet run --project Api

To free it:  pd kill 5000
```

When nothing is listening but the bind still fails:

```
Port 5000 (TCP) has no listener, but it is inside a range reserved by Windows (4950-5049).
This usually comes from Hyper-V, WSL or Docker Desktop.
```

## Not in v1

- Feature parity with DevToys or DevTools-X.
- Third-party extensions loaded at runtime. v1 tools live in the repository.
- A background service or port history.
- Scanning project files for declared ports.
- Remote hosts. Local machine only.
- Changing system configuration, such as removing reserved port ranges. v1 explains the fix and leaves it to the user.
- Accounts or telemetry.

## Architecture

All tool logic lives in Rust libraries with no UI code, so any front end can sit on top.

- Tool logic never prints and never prompts. It returns data and errors.
- Every result is serializable.
- Platform-specific code sits behind a shared trait, one module per OS.
- No tool makes a network request.

## Extensibility

Adding a tool must not require changes to the shell. This is a core requirement, because tools will be added constantly.

- Each tool is a self-contained module that registers its id, name, category, search keywords, logic and UI view.
- The shell builds navigation and search from that registry.
- Target: a new text tool is one new folder plus one registration line.

Third-party extensions loaded at runtime come after v1, with DevToys's extension SDK as the reference. The v1 tool interface must not block them, so the stack research should say how they could be added later.

## Stack: research task

Before writing code, Claude Code researches the options below and writes a short comparison with a recommendation for approval.

Fixed:

- Rust for all tool logic.
- Windows, Linux and macOS from the first commit, with CI on all three.
- Fully offline. MIT license.

Open:

- UI approach: a web front end in Tauri, or a native Rust GUI toolkit.
- The front-end framework, if the UI is web-based.
- Tray icon and global shortcut support on all three platforms.
- Packaging and updates: installers, `winget`, `scoop`, `brew` and Linux packages.
- Crates for the port inspector: socket-to-PID mapping, process details and Win32 access.
- A path to runtime extensions later.

Criteria, in order: runtime performance and memory use, maturity and maintenance of the dependencies, tooling and developer experience, how little code a new tool needs, startup time and installer size.

## Updates

The update check is the only network call in the app, and it is opt-in.

- Off by default. First launch asks once, and settings has a "Check now" button that works either way.
- The check reads the latest version from GitHub Releases and sends no data about the user or the machine.
- The app shows the new version and its changelog, and installs only after a click.
- Downloads are verified against a signature before installing.
- Users who install through `winget`, `scoop` or `brew` update there.

The stack research should cover the updater mechanism and the cost of code signing on Windows and macOS.

## Port inspector: platform notes

All three platforms are built together from the first milestone, and every feature lands on Windows, Linux and macOS before it counts as done. Windows has the most special cases. These are starting points from memory and each needs verifying.

| Case | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Port to PID | `GetExtendedTcpTable` and `GetExtendedUdpTable` | `/proc/net` or netlink `sock_diag` | `libproc` file-descriptor info |
| Command line and parent chain | `sysinfo`, or Toolhelp snapshot | `/proc/<pid>` | `libproc`, `sysctl` |
| Working directory | Read from the process's memory; may need elevation | `/proc/<pid>/cwd` | `proc_pidinfo` |
| Kill the tree | Job objects or walk children, then `TerminateProcess` | Signal the process group | Signal the process group |

Windows-only cases that make `why` worth having:

- **Service behind `svchost.exe`:** map the PID to service names.
- **PID 4 ("System"):** the port belongs to `http.sys`, usually IIS or a URL reservation.
- **Reserved ranges:** no listener, but the port is in an excluded range (`netsh interface ipv4 show excludedportrange protocol=tcp`).
- **Other users' processes:** details need an elevated shell. Say so instead of showing blanks.

On every platform, detect Docker-published ports and name the container and compose project.

## Milestones

1. **Stack decision.** Research write-up and recommendation, approved before any code.
2. **Shell and tool interface.** Window, navigation, search, tray and the tool registry, with the UUID generator working end to end and CI on all three platforms.
3. **Text tools.** The other five.
4. **Port inspector.** List, explain and kill on all three platforms.
5. **Port special cases.** Windows services, PID 4 and reserved ranges, plus Docker detection everywhere.
6. **Release.** Installers for the three platforms and package managers. This is v1.
7. **After v1.** More system tools (environment and PATH, hosts and DNS, certificates) and runtime extensions.

## Open questions

- [x] App name: Navaja (decided, free on crates.io). Checked 2026-10-02:
  - free on winget, Homebrew, Scoop, Flathub, AUR and npm;
  - the GitHub login `navaja` is taken, so the repo stays at `joaovitorpina/Navaja`.

  See [ADR 0001](docs/adr/0001-stack.md#frozen-identifiers).
- [x] CLI front end: not in v1, possible later (decided). It must not be called `pd` ([ADR 0001](docs/adr/0001-stack.md#frozen-identifiers)).
- [x] Updates: opt-in update checks are acceptable (decided). The mechanism is `tauri-plugin-updater` with minisign signatures and a static `latest.json` on GitHub Releases ([ADR 0001](docs/adr/0001-stack.md#decisions-per-open-item)).
- [x] Port inspector: TCP by default, with UDP as an option in v1 (decided; [ADR 0001](docs/adr/0001-stack.md#decisions-per-open-item)).
- [ ] Deferred to implementation: can the working directory be read reliably on Windows without elevation?
  - Research says yes for the user's own non-elevated processes, through a PEB read, and "needs elevation" otherwise.
  - Spike S4.1 confirms this in M4.
- [x] Deferred to implementation: Docker detection through the Docker CLI or the engine socket? Decided: through the engine socket or named pipe via bollard, local endpoints only ([ADR 0001](docs/adr/0001-stack.md#decisions-per-open-item)).
- [x] License: MIT (decided).
