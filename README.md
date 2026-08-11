# Voxi

![Voxi icon](assets/voxi.svg)

Lightweight Windows text-to-speech tray app using SAPI, with an optional private offline Narrator voice backend.

## Features

- Read clipboard text aloud via hotkey
- Cycle between Microsoft Guy and Microsoft Eva
- Prewarmed Microsoft Guy natural voice at native SAPI rate 10
- Adjustable speech speed
- Built-in pronunciation dictionary for URLs, acronyms, emojis
- Idle and speaking tray states with embedded white line icons
- Compact native menu with transparent colored line icons
- Single-instance protection and automatic tray recovery after Explorer restarts

## Hotkeys

- `Alt+1` - Toggle read/stop
- `Alt+2` - Cycle speed
- `Alt+3` - Cycle voice
- `Alt+4` - Exit

## Build

Voxi requires Windows, Rust 1.80 or newer, and the Windows SDK resource compiler.

```powershell
cargo build --release
```

The executable is written to `target/release/Voxi.exe`.

Microsoft Guy is loaded when `runtime/natural` is installed beside `Voxi.exe`. Voxi uses registration-free COM activation and does not register the adapter system-wide. Guy is the default and first voice, with Eva second as the lightweight SAPI fallback. If Guy cannot initialize or later rejects a speech request, Voxi switches to Eva and keeps working.

To install Guy from the adapter's official release and Microsoft's official US English voice package:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/setup-guy.ps1
```

The script verifies both downloads by SHA-256 and extracts only the 64-bit runtime files Voxi needs. The downloaded Microsoft files remain outside version control. See `THIRD_PARTY_NOTICES.md` for source and license details.

## Development

```powershell
cargo fmt -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

## License

MIT
