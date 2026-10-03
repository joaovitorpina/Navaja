# Navaja

An offline, cross-platform developer toolbox for Windows, Linux and macOS. *Navaja* is Spanish for pocket knife.

> **Status: pre-alpha.** The stack and architecture are decided. There is no code or release yet.

Navaja holds two kinds of tools:

- **Text tools** replace the websites developers paste tokens and payloads into, so a production JWT never leaves your machine.
- **System tools** inspect your machine, which a website cannot do.

## Planned for v1

| Tool | What it does |
|---|---|
| JWT decoder | Decodes header and payload and shows expiry |
| JSON formatter | Formats, minifies and validates |
| Base64 | Encodes and decodes |
| URL encoder | Encodes and decodes |
| Hash generator | MD5, SHA-1, SHA-256 and SHA-512 |
| UUID generator | Generates one or many (v4, v7) |
| Port inspector | Shows what holds a port, explains why, and frees it |

## Principles

- **Offline.** No tool makes a network request. The only network call is an opt-in update check against GitHub Releases, and it is off until you agree.
- **Rust everywhere it matters.** All tool logic lives in Rust libraries with no UI code. The UI is Tauri 2 with Svelte 5.
- **Easy to extend.** A new text tool is one folder plus one registration line.

## Documentation

- [Stack decision (ADR 0001)](docs/adr/0001-stack.md)
- [Architecture](docs/architecture.md)
- [Roadmap: milestones, spikes and verification](docs/roadmap.md)

## License

[MIT](LICENSE)
