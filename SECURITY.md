# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's [private vulnerability reporting form](https://github.com/joaovitorpina/Navaja/security/advisories/new). Do not open a public issue.

Please include the affected version and OS, the steps to reproduce, and the impact you expect. You will get an acknowledgement within 7 days.

## What counts as a security issue

Navaja promises to work offline and to keep what you paste on your machine. These count as security bugs:

- **Any network connection** other than:
  - the opt-in update check to github.com and release-assets.githubusercontent.com;
  - webview runtime traffic disclosed in `docs/privacy.md`, once that file is published.

  Undisclosed traffic from the embedded webview counts.
- **Pasted input, tokens or tool output** reaching any of these:
  - logs or crash files;
  - web storage;
  - the OS clipboard history: Windows clipboard history and cloud clipboard, KDE Klipper, or macOS;
  - a clipboard manager that honours the standard exclusion markers;
  - any other place you did not ask for.
- The port inspector signalling a process that was not in the plan you confirmed. This includes PID reuse, ancestors and denylisted system processes.
- An update installing without a valid signature, or installing an older version.
- The Docker integration connecting to anything but a local socket or named pipe.

## Supported versions

Navaja has not been released yet. After v1, only the latest release receives fixes.
