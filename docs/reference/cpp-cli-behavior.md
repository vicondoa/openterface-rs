# C++ Openterface CLI behavioral compatibility spec (W1.5)

This document captures the observable behavior of the C++ CLI at `/home/paydro/projects/openterface` so `openterface-rs` can match it without consulting the C++ source. Citations use `file:line` in the C++ repository unless explicitly marked `openterface-rs`.

## Scope and entry point

- The executable entry point constructs `openterface::CLI` and returns `cli.run(argc, argv)` (`openterface-cli.cpp:3-5`).
- The CLI11 application is constructed as `app("Openterface USB KVM CLI", "openterface")` (`src/cli.cpp:23`).
- The built-in version flag is exactly `--version`, using version string `1.0.0` (`src/cli.cpp:38-39`, `include/openterface/cli.hpp:23`). There is no C++ `-V` alias in source.
- Exactly one subcommand is required via `app.require_subcommand(1)` (`src/cli.cpp:391`).
- The current C++ build prints unconditional `DEBUG:` lines during CLI construction and `connect`; these are not gated by `--debug` or `--verbose` (`src/cli.cpp:24-44`, `src/cli.cpp:62`, `src/cli.cpp:153`, `src/cli.cpp:251-270`).

## Command surface

Quote these option strings and help strings exactly in the Rust clap surface unless intentionally documenting a Rust-only deviation.

| Scope | CLI11 declaration | Required? | Behavior |
|---|---|---:|---|
| Global | `-v,--verbose` — `Enable verbose output` (`src/cli.cpp:49-51`) | No | In every implemented callback, if set, prints `Verbose mode enabled\n` before the command body (`src/cli.cpp:64-65`, `src/cli.cpp:285-286`, `src/cli.cpp:326-327`, `src/cli.cpp:351-352`). |
| Global | `--version` (`src/cli.cpp:38-39`) | No | CLI11 version flag prints the version string `1.0.0` and exits through CLI11 parse handling (`include/openterface/cli.hpp:23`, `src/cli.cpp:394-400`). |
| Subcommand | `connect` — `Connect to KVM device (auto-discovers devices if none specified)` (`src/cli.cpp:53-54`) | Exactly one subcommand required | Starts the GUI-oriented KVM session. Accepts partial modes: video-only, serial/input-only, GUI-only, or dummy. |
| `connect` option | `--video` — `Video device path (optional - auto-detected if omitted)` (`src/cli.cpp:55`) | No | Overrides video auto-detect. If omitted and `--no-video` is absent, auto-detects and uses the first detected Openterface video device (`src/cli.cpp:75-85`). |
| `connect` option | `--serial` — `Serial device path (optional - auto-detected if omitted)` (`src/cli.cpp:56`) | No | Overrides serial auto-detect. If omitted and `--no-serial` is absent, auto-detects and uses the first detected serial device (`src/cli.cpp:87-96`). |
| `connect` flag | `--no-video` — `Disable video capture (even if device detected)` (`src/cli.cpp:57`) | No | Clears any video path and prints `- Video disabled by --no-video flag` (`src/cli.cpp:98-102`). |
| `connect` flag | `--no-serial` — `Disable input forwarding (even if device detected)` (`src/cli.cpp:58`) | No | Clears any serial path and prints `- Serial disabled by --no-serial flag` (`src/cli.cpp:103-106`). |
| `connect` flag | `--dummy` — `Run in dummy mode (no device connection, GUI only)` (`src/cli.cpp:59`) | No | Skips all device connections, starts GUI with test-pattern video display and simulated input messaging (`src/cli.cpp:69-72`, `src/cli.cpp:194-201`, `src/cli.cpp:227-232`). |
| `connect` flag | `--debug` — `Enable debug output for input events (mouse/keyboard)` (`src/cli.cpp:60`) | No | Calls `gui->setDebugMode(true)`, which sets callback debug state and logs `Debug mode enabled - input events will be logged` after GUI initialization (`src/cli.cpp:209-212`, `src/gui.cpp:561-566`). |
| Subcommand | `reset` — `Perform factory reset of CH9329 chip` (`src/cli.cpp:281-282`) | Exactly one subcommand required | Connects to the serial port, invokes CH9329 factory reset, prints success/failure messages, then disconnects (`src/cli.cpp:294-320`). |
| `reset` option | `--serial` — `Serial device path (required for reset)` (`src/cli.cpp:283`) | Required by callback, not CLI11 | If empty, prints `Error: --serial parameter is required for reset command` and `Usage: openterface reset --serial /dev/ttyUSB0`, then returns from the callback (`src/cli.cpp:288-292`). |
| Subcommand | `status` — `Show device status` (`src/cli.cpp:323-324`) | Exactly one subcommand required | Prints current in-process `SerialInfo` and `VideoInfo`; it does not scan or connect first (`src/cli.cpp:329-345`). |
| Subcommand | `scan` — `Scan for Openterface devices` (`src/cli.cpp:348-349`) | Exactly one subcommand required | Enumerates `/dev/video0..9` and serial nodes, prints found devices and a recommended command (`src/cli.cpp:354-388`). |

## Exit codes

The C++ callbacks do not throw or return status codes for runtime failures. `CLI::run` returns `0` after `app.parse`, and only CLI11 parse exceptions return `app.exit(e)` (`src/cli.cpp:394-400`). Therefore:

| Case | C++ exit behavior | Source |
|---|---:|---|
| Successful `connect`, `scan`, `status`, or `reset` callback completion | `0` | `src/cli.cpp:394-397` |
| `connect` video connection failure | Prints `✗ Video connection failed`, returns from callback, then `CLI::run` returns `0` | `src/cli.cpp:119-126`, `src/cli.cpp:394-397` |
| `connect` GUI initialization/window creation failure | Prints failure, returns from callback, then process exits `0` | `src/cli.cpp:155-159`, `src/cli.cpp:183-188`, `src/cli.cpp:394-397` |
| `connect` no devices found | Not an error: prints `No devices available - running in GUI-only mode`, then starts GUI and exits with GUI event loop lifecycle | `src/cli.cpp:111-113` |
| `connect` serial connection failure | Async callback prints `✗ Serial connection failed: <message>`; command already continues into GUI. Final exit remains `0` unless CLI11 parse failed | `src/cli.cpp:131-148`, `src/cli.cpp:394-397` |
| `scan` no devices found | Not an error: prints troubleshooting suggestions and exits `0` | `src/cli.cpp:380-388`, `src/cli.cpp:394-397` |
| `reset --serial` omitted | Prints error/usage, returns from callback, exits `0` | `src/cli.cpp:288-292`, `src/cli.cpp:394-397` |
| `reset` cannot connect or factory reset fails | Prints failure text, exits `0` | `src/cli.cpp:304-320`, `src/cli.cpp:394-397` |
| Bad CLI syntax / missing required subcommand / unknown option | `app.exit(e)` from CLI11 parse error. Match CLI11 default parse-error code (normally `1`), and help/version exceptions (normally `0`) | `src/cli.cpp:391`, `src/cli.cpp:394-400` |

Compatibility note: a Rust implementation that returns nonzero for runtime failures would be more conventional but not C++-compatible.

## Output format

### Global and construction output

The C++ implementation currently emits these unconditional constructor lines before parsing any command:

```text
DEBUG: CLI constructor - starting
DEBUG: Creating Serial
DEBUG: Creating Video
DEBUG: Creating Input
DEBUG: Creating GUI
DEBUG: Setting version flag
DEBUG: Setting up commands
DEBUG: CLI constructor - complete
```

These lines come from `src/cli.cpp:24-44`. `connect` also emits multiple unconditional `DEBUG:` lifecycle lines (`src/cli.cpp:62`, `src/cli.cpp:67`, `src/cli.cpp:73`, `src/cli.cpp:153`, `src/cli.cpp:162`, `src/cli.cpp:191`, `src/cli.cpp:251-270`, `src/cli.cpp:272-278`).

### `scan`

`scan` prints this structure (`src/cli.cpp:354-388`):

```text
Scanning for Openterface USB KVM devices...

=== Video Devices ===
Found: /dev/videoN (<card-name>)
# if verbose and not Openterface:
Found: /dev/videoN (<card-name>) - not Openterface

=== Serial Devices ===
Found: /dev/ttyUSBn (VID:PID 0x1A86:0x7523)
Found: /dev/ttyACMn (VID:PID 0x1A86:0x7523)

=== Recommended Connection ===
Try: openterface connect --video=/dev/videoN --serial=/dev/ttyX
```

If either video or serial list is empty, the recommendation block is instead (`src/cli.cpp:384-388`):

```text
No Openterface devices detected.
Ensure device is plugged in and recognized by the system.
Or use: openterface connect --dummy
```

Video scan only prints devices whose V4L2 `cap.card` string contains `Openterface`; non-matching video nodes print only under `-v/--verbose` (`src/cli.cpp:356-369`, `src/cli.cpp:403-417`). Serial scan prints every node returned by `findOpenterfaceSerialPorts`, with the literal VID/PID text `0x1A86:0x7523` regardless of which path matched (`src/cli.cpp:373-378`).

### `status`

`status` prints exactly this shape (`src/cli.cpp:329-345`):

```text
=== Openterface KVM Status ===
Serial: CONNECTED (/dev/ttyX @ 115200)
# or
Serial: DISCONNECTED
Video: CONNECTED (1920x1080 MJPG)
# or
Video: DISCONNECTED
Target: RESPONSIVE
# or
Target: NO RESPONSE
```

Because `status` reads fresh module objects created during this process and does not connect first, the normal standalone output is `Serial: DISCONNECTED`, `Video: DISCONNECTED`, `Target: NO RESPONSE` (`src/cli.cpp:26-36`, `src/cli.cpp:329-345`).

### `connect`

Key user-visible lines and ordering:

1. Verbose line, if set: `Verbose mode enabled` (`src/cli.cpp:64-65`).
2. Dummy mode:
   ```text
   Starting Openterface KVM in dummy mode...
   No device connections will be made.
   ```
   (`src/cli.cpp:69-72`).
3. Auto-detect video:
   ```text
   Auto-detecting video devices...
   ✓ Found video device: /dev/videoN
   # or
   - No Openterface video devices detected
   ```
   (`src/cli.cpp:75-85`).
4. Auto-detect serial:
   ```text
   Auto-detecting serial devices...
   ✓ Found serial device: /dev/ttyX
   # or
   - No Openterface serial devices detected
   ```
   (`src/cli.cpp:87-96`).
5. Disable flags, if present: `- Video disabled by --no-video flag`; `- Serial disabled by --no-serial flag` (`src/cli.cpp:98-106`).
6. If at least one endpoint remains:
   ```text
   Connecting to Openterface KVM...
   Video: /dev/videoN
   Serial: /dev/ttyX
   ```
   If neither remains: `No devices available - running in GUI-only mode` (`src/cli.cpp:111-117`).
7. Video: `✓ Video connected` or `✗ Video connection failed`; if no video, `- Video capture disabled (no --video specified)` (`src/cli.cpp:119-129`).
8. Serial: `Connecting to serial port...`, followed asynchronously by `✓ Serial connected` or `✗ Serial connection failed: <message>` (`src/cli.cpp:131-148`). If no serial: `- Input forwarding disabled (no --serial specified)` (`src/cli.cpp:148-150`).
9. GUI: `✓ GUI initialized`, then `✓ Window created` or failure lines (`src/cli.cpp:155-189`).
10. Video display: `✓ Video display started`, `✓ Video display started (dummy mode - test pattern)`, `✗ Failed to start video display`, or `- Video display disabled (no --video specified)` (`src/cli.cpp:193-207`).
11. Input capture: `✓ Input capture started (keyboard/mouse will be forwarded)`, `✗ Failed to start input capture`, or `- Input capture disabled (no --serial specified)` (`src/cli.cpp:214-225`).
12. Ready block:
   ```text
   
   === KVM Ready ===
   - Full KVM mode: Video display + Input forwarding
   - Video-only mode: Display feed, no input forwarding
   - Input-only mode: Forwarding keyboard/mouse, no video
   - GUI-only mode: Test window, no device connections
   - Video feed active
   - Input forwarding active
   - Close window or press Ctrl+C to exit
   ```
   Dummy mode uses `- Running in dummy mode (no device connections)`, `- Video will show test pattern`, `- Input will be simulated (not forwarded)` (`src/cli.cpp:227-249`).
13. On GUI event-loop return: blank line plus `GUI exited with code: <n>`, cleanup `DEBUG:` lines, and `✓ Cleanup complete` (`src/cli.cpp:253-278`).

### `reset`

`reset` prints (`src/cli.cpp:294-320`):

```text
=== CH9329 Factory Reset ===
Connecting to serial port: /dev/ttyX
✓ Connected to serial port
Performing factory reset (this will take ~5 seconds)...
✓ Factory reset completed successfully!
The CH9329 chip has been reset to factory defaults.
You can now try connecting normally.
✓ Disconnected from serial port
```

Failure variants are `✗ Factory reset failed!`, `Check that the device is properly connected.`, or `✗ Failed to connect to serial port: /dev/ttyX`, `Check that the device is plugged in and accessible.`, `Try: openterface scan` (`src/cli.cpp:308-320`).

### Verbose and `--debug`

- `-v/--verbose` only prints `Verbose mode enabled` at command start plus extra non-Openterface video devices during `scan` (`src/cli.cpp:64-65`, `src/cli.cpp:367-369`).
- `--debug` only exists on `connect`, and enables GUI input debug logging (`src/cli.cpp:60`, `src/cli.cpp:209-212`, `src/gui.cpp:561-566`). It does not control the unconditional `DEBUG:` prints in `src/cli.cpp`.
- Several input messages are logged even when `debug_mode=false`, because callback code checks only `log_func` for button/key receipt and forwarding (`src/gui_input.cpp:427-432`, `src/gui_input.cpp:523-529`, `src/gui_input.cpp:560-588`, `src/gui_input.cpp:748-760`, `src/gui_input.cpp:798-817`). Additional gated debug examples include mouse enter detail, throttled mouse position every 30 motion callbacks, mouse button state, GPU/CPU render logs, and forwarded motion logs (`src/gui_input.cpp:300-304`, `src/gui_input.cpp:361-377`, `src/gui_input.cpp:593-600`, `src/gui.cpp:1468-1480`, `src/gui.cpp:1651-1657`).

## Device discovery ordering and auto-select heuristics

There are two discovery implementations in the C++ tree: the active CLI helpers in `src/cli.cpp`, and a `KVMManager` scan path in `src/kvm.cpp`. `connect` and `scan` use the CLI helpers, not `KVMManager` (`src/cli.cpp:78-91`, `src/cli.cpp:356-375`).

### Active CLI video discovery

- Iterate integer indices `0..9` and build `/dev/video<i>` in ascending order (`src/cli.cpp:471-473`).
- Check existence with `access(path, F_OK)` (`src/cli.cpp:474`).
- Query V4L2 `VIDIOC_QUERYCAP`; return the `cap.card` string as the device name, or `Unknown` on open/ioctl failure (`src/cli.cpp:403-417`).
- Accept only if `device_name.find("Openterface") != std::string::npos` (`src/cli.cpp:475-479`).
- Auto-select the first accepted path (`video_devices[0]`) (`src/cli.cpp:78-81`).

Quote of active predicate:

```cpp
std::string device_name = getVideoDeviceName(device);
if (device_name.find("Openterface") != std::string::npos) {
    openterface_videos.push_back(device);
}
```

Source: `src/cli.cpp:475-479`.

### Active CLI serial discovery

- Open `/sys/class/tty`; if unavailable, return an empty list (`src/cli.cpp:426-431`).
- Iterate directory entries in filesystem order. Accept names starting with `ttyUSB` or `ttyACM` (`src/cli.cpp:433-435`).
- Read `/sys/class/tty/<name>/device/../uevent` (`src/cli.cpp:437-440`).
- Accept if any line begins with `PRODUCT=1a86/7523/` (`src/cli.cpp:444-448`).
- Add `/dev/<name>` only if it exists (`src/cli.cpp:452-456`).
- Auto-select the first accepted path (`serial_devices[0]`) (`src/cli.cpp:87-92`).

Quote of active predicate:

```cpp
if (strncmp(entry->d_name, "ttyUSB", 6) == 0 || strncmp(entry->d_name, "ttyACM", 6) == 0) {
    ...
    if (line.find("PRODUCT=1a86/7523/") == 0) {
        is_openterface = true;
        break;
    }
}
```

Source: `src/cli.cpp:433-448`.

Important parity nuance: the CLI source does **not** explicitly prefer `/dev/ttyACM*` over `/dev/ttyUSB*`; it returns the filesystem iteration order from `readdir`. The shipped diagnostic tool does prefer `/dev/ttyACM*` before `/dev/ttyUSB*` (`tools/kvm-debug.sh:46-50`), and `TROUBLESHOOTING.md` documents the input endpoint as `/dev/ttyACM0` (`TROUBLESHOOTING.md:15-18`). For Rust parity, decide whether to match the active CLI exactly or the shipped diagnostic/documented behavior; this is an ambiguity.

### Shipped diagnostic/video heuristic: uvcvideo + MJPG, skip virtio

`TROUBLESHOOTING.md` documents that in VMs, only the real capture is the `uvcvideo` device advertising `MJPG`; virtio decoder adapters are not the KVM (`TROUBLESHOOTING.md:136-145`). The shipped `tools/kvm-debug.sh` implements this by scanning `/dev/video*`, requiring `v4l2-ctl --info` driver name to contain `uvcvideo`, and requiring listed formats to contain `mjpg` (`tools/kvm-debug.sh:31-43`). This is stronger than the active CLI helper, which only searches the card name for `Openterface`.

### KVMManager discovery path

`KVMManager::scanUSBDevices` scans `/sys/bus/usb/devices`, reads `idVendor` and `idProduct`, and accepts `(6666,6666)` or `(534d,2109)` (`src/kvm.cpp:25-31`, `src/kvm.cpp:296-323`). It then guesses `/dev/ttyUSB0` and `/dev/video0` rather than mapping endpoints (`src/kvm.cpp:325-328`). If no devices are found, it pushes a `Default Openterface Device` with `vendor_id=unknown`, `/dev/ttyUSB0`, `/dev/video0` (`src/kvm.cpp:338-349`). This path is not used by `src/cli.cpp` commands.

## Connection lifecycle

### `connect` command lifecycle

`connect` is GUI-first and tolerant of missing endpoints:

1. Auto-detect video and serial unless paths or disable flags are provided (`src/cli.cpp:75-106`).
2. If video exists, synchronously call `video->connect(video_device)`. A video failure returns from the callback before GUI startup (`src/cli.cpp:119-126`).
3. If serial exists, print `Connecting to serial port...` and call `serial->connectAsync(serial_port, 115200, callback)`, then continue immediately into GUI startup without waiting (`src/cli.cpp:131-148`).
4. Initialize GUI, create a `1920x1080` window, title based on mode (`src/cli.cpp:155-189`). Titles: `Openterface KVM - Dummy Mode`, `Full Mode`, `Video Only`, `Input Only`, or `GUI Only` (`src/cli.cpp:164-180`).
5. If video or dummy mode, set video source and start display (`src/cli.cpp:193-207`).
6. If `--debug`, set GUI debug mode (`src/cli.cpp:209-212`).
7. If serial or dummy mode, set input target/serial forwarder and start input capture (`src/cli.cpp:214-225`).
8. Run blocking GUI event loop (`src/cli.cpp:253-255`).
9. Shutdown order after loop: stop input capture, stop video display, destroy window, shutdown GUI, then disconnect video and serial if not dummy (`src/cli.cpp:259-276`).

### Synchronous KVMManager lifecycle

The `KVMManager` path (not the CLI `connect` command) connects serial first, then video, then Wayland input; video failure disconnects serial (`src/kvm.cpp:89-119`). `startKVMSession` starts video capture, input forwarding, then GUI; GUI failure is non-fatal for CLI usage, but video failure marks overall `success=false` (`src/kvm.cpp:232-270`). Shutdown stops GUI, input forwarding, then video capture (`src/kvm.cpp:273-283`), and `disconnect` then disconnects Wayland input, video, serial (`src/kvm.cpp:122-143`).

### Serial connection sequence

`Serial::connect(port, baudrate)` tries the requested baud first, then adds `9600` as fallback if requested baud is not already `9600` (`src/serial.cpp:284-303`). The CLI passes `115200` (`src/cli.cpp:136`, `src/cli.cpp:298`).

For each baud, `connectAtBaudRate`:

1. Opens `port` as `O_RDWR | O_NOCTTY | O_NDELAY` (`src/serial.cpp:352-360`).
2. Configures POSIX serial: baud, `CLOCAL|CREAD`, no parity, one stop bit, 8 data bits, no hardware/software flow control, raw output, `VMIN=0`, `VTIME=1` (`src/serial.cpp:363-400`).
3. Marks `connected=true` before initialization (`src/serial.cpp:407`).
4. Sleeps 50 ms (`src/serial.cpp:409-413`).
5. Sends `CMD_GET_PARA_CFG` base bytes `57 AB 00 08 00`; checksum is appended by `sendCommandWithChecksum` (`src/serial.cpp:415-423`, `src/serial.cpp:55-60`).
6. Sleeps 100 ms, reads response, requires at least 6 bytes and byte index `5 == 0x82` for correct mode (`src/serial.cpp:425-437`).
7. If mode differs, runs software reset/reconfigure (`src/serial.cpp:438-451`).
8. If no response at 115200, closes the fd, clears `connected`, returns false so `connect()` tries 9600 (`src/serial.cpp:453-462`).
9. If no response at 9600, tries hardware factory reset, waits 1 second, then software reset; if hardware reset fails, still tries software reset (`src/serial.cpp:463-490`).
10. Sends `CMD_GET_INFO` base bytes `57 AB 00 01 00`, sleeps 50 ms, reads response (`src/serial.cpp:502-506`). If response non-empty, logs ready and sets `target_connected=true`; if empty, logs `Warning: No response from CH9329 to info command` and sets `target_connected=false`; this does **not** fail initialization (`src/serial.cpp:507-521`).

`TROUBLESHOOTING.md` explicitly states `[SERIAL] No response from CH9329 to info command` is harmless for this firmware revision: the chip still accepts mouse/keyboard commands, so do not treat it as a connection failure (`TROUBLESHOOTING.md:147-151`).

## Video capture behavior

- `VideoInfo` defaults: `1920x1080`, `fps=30`, `format="MJPG"`, disconnected/not capturing (`include/openterface/video.hpp:10-18`).
- `Video::connect` opens the V4L2 device, queries capabilities, requires `V4L2_CAP_VIDEO_CAPTURE`, then calls `setupV4L2` (`src/video.cpp:68-106`).
- `setupV4L2` first gets current format, then tries `1920x1080` `V4L2_PIX_FMT_MJPEG`, `V4L2_FIELD_NONE`; on failure tries `1280x720` MJPEG; on failure tries `1280x720` YUYV (`src/video.cpp:260-300`).
- It requests 30 fps using `VIDIOC_G_PARM`/`VIDIOC_S_PARM` with `timeperframe = 1/30`; failure logs a warning but does not fail setup (`src/video.cpp:305-326`).
- It logs final format as `Video format: <format> <width>x<height> @ <fps>fps` (`src/video.cpp:328-329`).
- `startCapture` allocates 4 MMAP buffers, starts streaming, launches capture thread (`src/video.cpp:129-158`, `src/video.cpp:339-382`).
- Capture loop uses `select` timeout 25 ms, dequeues frames, immediately invokes callback with buffer data/size/current dimensions/timestamp, then requeries buffer (`src/video.cpp:397-458`).
- Supported formats/resolutions methods are hard-coded to `{"MJPG", "YUYV"}` and `1920x1080`, `1280x720`, `640x480` (`src/video.cpp:471-475`).

## CH9329 protocol specifics

### Framing and checksum

Every C++ command method builds a base frame without checksum, then calls `sendData`, which calls `sendCommandWithChecksum` (`src/serial.cpp:544-547`). The checksum is `sum(all bytes before checksum) % 256`, appended as the final byte (`src/serial.cpp:46-60`). This matches `PROGRESS.md` and `TROUBLESHOOTING.md`, which describe `57 AB 00 <CMD> <LEN> <DATA…> <SUM>` and low-byte additive checksum (`PROGRESS.md:138-143`, `TROUBLESHOOTING.md:65-74`).

All physical writes are protected by `send_mutex`, write the complete frame, then `tcdrain`; commands are paced by an internal minimum 4 ms gap to avoid CH9329 drops (`src/serial.cpp:31-41`, `src/serial.cpp:68-110`).

### Keyboard report

Press frame before checksum (`src/serial.cpp:581-596`):

```text
57 AB 00 02 08 <mod> 00 <key1> 00 00 00 00 00
```

- `CMD=0x02`, `LEN=0x08`.
- Modifier byte mapping in this method comment: Ctrl `0x01`, Shift `0x02`, Alt `0x04`, Meta `0x08` (`src/serial.cpp:587-593`).
- Release frame is all zero report data:

```text
57 AB 00 02 08 00 00 00 00 00 00 00 00
```

Source: `src/serial.cpp:598-606`. `sendCtrlAltDel` sends key `0x4C` with modifiers `0x05` (Ctrl+Alt), then release (`src/serial.cpp:715-724`). `PROGRESS.md` and `TROUBLESHOOTING.md` corroborate the keyboard layout and all-zero release (`PROGRESS.md:65-84`, `TROUBLESHOOTING.md:65-74`).

### Absolute mouse report

Move/frame before checksum (`src/serial.cpp:609-641`, `src/serial.cpp:686-698`):

```text
57 AB 00 04 07 02 <button-mask> <x_lo> <x_hi> <y_lo> <y_hi> 00
```

- `CMD=0x04`, `LEN=0x07`, first data byte `0x02` selects absolute mode (`src/serial.cpp:618-625`).
- Coordinates are little-endian 16-bit values. GUI maps window coordinates to the CH9329 range `0..4095` and clamps (`src/gui.cpp:1633-1644`).
- Wheel byte is always `0x00` in the implemented send methods (`src/serial.cpp:625`, `src/serial.cpp:697`).
- Motion during drag carries the currently held button mask instead of releasing the button (`src/gui.cpp:1646-1651`).

Button mapping: left `0x01`, right `0x02`, middle `0x04`, no button `0x00` (`src/serial.cpp:660-668`, `PROGRESS.md:55-59`, `TROUBLESHOOTING.md:70-72`).

### Relative mouse report

Relative move/frame before checksum (`src/serial.cpp:626-638`, `src/serial.cpp:644-683`):

```text
57 AB 00 05 05 01 <button-mask> <dx> <dy> 00
```

- `CMD=0x05`, `LEN=0x05`, first data byte `0x01` selects relative mode.
- Relative `dx`/`dy` in `sendMouseMove` are clamped to signed 8-bit `[-127,127]`, then written as bytes (`src/serial.cpp:631-637`).
- Relative `sendMouseButton` writes `x & 0xFF`, `y & 0xFF` without the signed clamp (`src/serial.cpp:677-680`).

### Factory reset and software reconfiguration

- Software reset command base: `57 AB 00 0F 00`, plus checksum (`src/serial.cpp:180-191`, `src/serial.cpp:727-735`).
- Reconfiguration sequence: send reset, wait 100 ms; send `CMD_SET_PARA_CFG` base starting `57 AB 00 09 32`, mode `0x82`, baud bytes `00 01 C2 00`, VID/PID bytes `86 1A 29 E1`, plus remaining config bytes; wait 50 ms; send reset again; wait 200 ms (`src/serial.cpp:180-225`).
- Hardware factory reset requires an open fd, reads modem control signals, sets RTS high, sleeps 4 seconds, clears RTS, waits 500 ms (`src/serial.cpp:228-265`). The public `factoryReset` then waits 1 second and runs software reconfiguration (`src/serial.cpp:738-765`).
- `PROGRESS.md` describes 115200 default, 9600 fallback, mode `0x82`, `CMD_GET_PARA_CFG`, `CMD_GET_INFO`, and v1.9 factory reset guidance (`PROGRESS.md:5-29`, `PROGRESS.md:112-124`, `PROGRESS.md:150-161`).

## Runtime behaviors and environment tunables

### Mouse pacing and input forwarding

- Shipped behavior: mouse move forwarding is capped at 30 Hz by default; `OPENTERFACE_MOUSE_INTERVAL_MS=33` is the default/tuning example (`TROUBLESHOOTING.md:78-104`, `TROUBLESHOOTING.md:160-168`).
- Implementation default is `move_interval_ms = 33`, parsed from `OPENTERFACE_MOUSE_INTERVAL_MS` only if integer and `5 <= n <= 1000`; invalid values keep 33 (`src/gui.cpp:1599-1616`).
- The input thread coalesces position by sampling the latest `callback_data.last_mouse_x/y` once per interval, sends only if changed, and sleeps `move_interval_ms` every loop (`src/gui.cpp:1618-1670`).
- Serial itself also enforces a minimum 4 ms gap between any physical command writes (`src/serial.cpp:73-83`).
- Pointer leave/focus loss release held mouse buttons at the last CH9329 coordinate, preventing stuck drags; focus loss also sends an all-zero keyboard release (`src/gui_input.cpp:316-328`, `src/gui_input.cpp:721-735`).

### Idle MJPEG decode throttling

`TROUBLESHOOTING.md` documents the default tunables (`TROUBLESHOOTING.md:106-124`, `TROUBLESHOOTING.md:160-168`):

| Env var | Default | Behavior |
|---|---:|---|
| `OPENTERFACE_THROTTLE` | `1` | If exactly `0`, disables throttling and decodes every frame (`src/gui.cpp:1262-1264`, `src/gui.cpp:1294-1307`). |
| `OPENTERFACE_IDLE_DECODE_MS` | `100` | When idle and raw MJPEG is non-deterministic, cap decode attempts to this interval (`src/gui.cpp:1249-1267`, `src/gui.cpp:1340-1351`). |
| `OPENTERFACE_INPUT_WAKE_MS` | `250` | After input activity, keep full-rate decode for this many ms (`src/gui.cpp:1309-1315`). |
| `OPENTERFACE_IDLE_WATCHDOG_MS` | `1000` | Force a surface refresh of the cached frame at least this often; no decode required (`src/gui.cpp:1317-1326`). |

Parsing accepts non-empty integer values from `0..100000`; invalid values keep defaults (`src/gui.cpp:1249-1267`). The implementation skips decode/upload for byte-identical MJPEG frames, declares idle after 15 consecutive static frames (about 0.5 s at 30 fps), detects non-deterministic MJPEG by comparing decoded pixels, and then uses decoded-frame compare plus the idle decode gate (`src/gui.cpp:139-188`, `src/gui.cpp:1328-1395`).

### libdecor / CSD / fullscreen windowing

- `OPENTERFACE_USE_LIBDECOR` defaults to enabled. If the env var exists and starts with `0`, skip libdecor and use raw xdg-shell. If libdecor creation fails, fall back to raw xdg-shell (`src/gui.cpp:36-40`, `src/gui.cpp:680-701`).
- With libdecor active, the app decorates the surface, sets title/app id, minimum content size `640x480`, waits up to 100 dispatch iterations of 50 ms for initial configure, and disables the app's raw xdg resize/move path by setting `callback_data.xdg_toplevel = nullptr` so content-area clicks still forward to the KVM (`src/gui.cpp:901-948`).
- `OPENTERFACE_FULLSCREEN=1` calls `xdg_toplevel_set_fullscreen` in the raw xdg-shell path and logs `Fullscreen requested (OPENTERFACE_FULLSCREEN=1)`; unset means normal windowed/tiled behavior (`src/gui.cpp:1010-1023`, `TROUBLESHOOTING.md:169-170`).
- Raw xdg-shell windows set minimum size `640x480`, maximum `(0,0)` (no maximum), title from mode, app id `com.openterface.openterfaceQT` (`src/gui.cpp:1006-1030`).

### Resize off input thread

Resize work is deliberately not done on the Wayland/input event thread. The Wayland event thread only stores target dimensions and `resize_pending=true`; the render thread consumes the flag and resizes the GPU renderer, then forces repaint (`src/gui.cpp:171-180`, `src/gui.cpp:1440-1455`, `src/gui.cpp:1518-1533`). The rationale is to avoid blocking input dispatch, which can batch key-down/key-up or click press/release and cause dropped input at the target (`src/gui.cpp:171-179`, `src/gui.cpp:1518-1521`).

## Parity checklist for openterface-rs

| Behavior | Rust implementation area | Current/target location |
|---|---|---|
| CLI11-equivalent commands/options/help, `--version`, one required subcommand | cli | `openterface-rs/crates/openterface-cli/src/main.rs` |
| C++-compatible exit codes, including runtime failures returning 0 | cli | `openterface-rs/crates/openterface-cli/src/main.rs` |
| `scan`/`status` text formats and verbose behavior | cli + discovery + serial/video status | `openterface-rs/crates/openterface-cli/src/main.rs`, `crates/openterface-core/src/discovery/mod.rs` |
| Video discovery: active C++ `cap.card contains Openterface`; documented shipped heuristic `uvcvideo` + `MJPG`, skip virtio | discovery/video | `crates/openterface-core/src/discovery/mod.rs`, `crates/openterface-core/src/video/mod.rs` |
| Serial discovery: CH341 `PRODUCT=1a86/7523/`, `/dev/ttyUSB*` and `/dev/ttyACM*`; decide ordering ambiguity | discovery/serial | `crates/openterface-core/src/discovery/mod.rs`, `crates/openterface-core/src/serial/mod.rs` |
| Serial open 115200 then 9600 fallback; GET_PARA_CFG, mode `0x82`, reset/reconfigure, GET_INFO harmless no-response | serial/protocol | `crates/openterface-core/src/serial/mod.rs`, `crates/openterface-core/src/protocol/mod.rs` |
| CH9329 frame layouts, additive checksum, command write serialization, 4 ms physical gap | protocol/serial | `crates/openterface-core/src/protocol/mod.rs`, `crates/openterface-core/src/serial/mod.rs` |
| Absolute mouse coordinate scaling/clamping 0..4095, button mask preserved during motion | input/protocol | `crates/openterface-core/src/input/mod.rs`, `crates/openterface-core/src/protocol/mod.rs` |
| Mouse move pacing: default 33 ms, env validation 5..1000, coalescing latest position | pacing/input/gui | `crates/openterface-core/src/pacing/mod.rs`, `crates/openterface-core/src/input/mod.rs`, `crates/openterface-gui/src/lib.rs` |
| Idle MJPEG throttling env vars/defaults, raw dedup, decoded compare, watchdog, input wake | gui/video/decode | `crates/openterface-gui/src/lib.rs`, `crates/openterface-core/src/decode/mod.rs`, `crates/openterface-core/src/video/mod.rs` |
| libdecor default-on, raw xdg fallback, fullscreen env, title/app id/min size | gui | `crates/openterface-gui/src/lib.rs` |
| Resize work moved from input/Wayland event thread to render thread | gui | `crates/openterface-gui/src/lib.rs` |
| Focus/pointer-leave safety releases buttons and keys | input/gui/serial | `crates/openterface-core/src/input/mod.rs`, `crates/openterface-gui/src/lib.rs`, `crates/openterface-core/src/serial/mod.rs` |
| Reset command: RTS high 4 s, wait 500 ms, wait 1 s, software reconfigure | cli/serial/protocol | `crates/openterface-cli/src/main.rs`, `crates/openterface-core/src/serial/mod.rs`, `crates/openterface-core/src/protocol/mod.rs` |

## Ambiguities and known source conflicts

1. Active CLI video auto-detect accepts `cap.card` containing `Openterface`; shipped troubleshooting/debug tooling says the robust heuristic is `uvcvideo` + `MJPG` and skip virtio (`src/cli.cpp:475-479`, `tools/kvm-debug.sh:33-43`, `TROUBLESHOOTING.md:136-145`).
2. Active CLI serial order is `readdir` order over `ttyUSB*`/`ttyACM*`; debug tooling prefers `/dev/ttyACM*` before `/dev/ttyUSB*` (`src/cli.cpp:433-456`, `tools/kvm-debug.sh:46-50`).
3. `KVMManager` discovery recognizes VID/PID `6666:6666` and `534d:2109`, guesses `/dev/ttyUSB0` and `/dev/video0`, and adds a default fake device if none found; the CLI does not use this for command behavior (`src/kvm.cpp:25-31`, `src/kvm.cpp:325-349`).
4. C++ runtime failures generally exit `0`; if Rust chooses more conventional nonzero runtime exit codes, document it as intentional incompatibility (`src/cli.cpp:394-400`).
5. `PROGRESS.md` says factory reset uses RTS low for v1.9 hardware, but implemented C++ sets RTS high for 4 seconds then low (`PROGRESS.md:158-160`, `src/serial.cpp:245-263`).
