# Voxi

![Voxi icon](assets/voxi.svg)

Lightweight Windows text-to-speech tray app using SAPI, with an optional private offline Narrator voice backend.

## Features

- Read clipboard text aloud via hotkey
- Cycle between Microsoft Guy and Microsoft Eva
- Prewarmed Microsoft Guy natural voice at native SAPI rate 10
- Adjustable speech speed
- Built-in pronunciation dictionary for URLs, acronyms, emojis
- Read Markdown content & link labels without formatting noise
- Decode HTML entities & expand numeric multiplication and common math symbols
- Preserve punctuation & following text when shortening URLs
- Queue 2 seconds of silent audio after speech to help with Bluetooth end clipping
- Idle and speaking tray states with embedded white line icons
- Compact native menu with transparent colored line icons
- Single-instance protection and automatic tray recovery after Explorer restarts

## Hotkeys

- `Alt+1` - Toggle read/stop
- `Alt+2` - Cycle speed
- `Alt+3` - Cycle voice
- `Alt+4` - Exit

The menu shows Read while idle & Stop while speaking, including live updates
while open. Read uses a speech bubble & Stop uses a stop square. Selecting Stop
never starts another reading. The voice menu shows MS Eva or MS Guy; changes announce
only the voice's first name: Eva or Guy.

Changing speed while reading preserves your place. Eva changes speed directly;
Guy resumes at the current word because its embedded engine cannot adjust an
utterance already in progress. Guy may repeat that word. The speed presets are
Slow, Mid & Fast (SAPI rates 0, 5 & 10). Speed announcements replace earlier
announcements immediately; only clipboard readings are resumed. Voice & speed
still reset to the startup defaults after exit.

The silent tail uses PCM audio on the same speech output for both voices. Guy's
embedded engine ignores XML silence tags. This is an app-side workaround;
whether it resolves Bluetooth clipping depends on the audio device & driver.
During the silent tail, the tray icon is idle. `Alt+1` starts the next reading
immediately & `Alt+2` announces the new speed, replacing the remaining silence.

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
