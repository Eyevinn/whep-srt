# WHEP to SRT Bridge

A GStreamer-based application that bridges WebRTC streams from WHEP (WebRTC HTTP Egress Protocol) endpoints to SRT (Secure Reliable Transport) output streams.

## Overview

This tool consumes WebRTC media from a WHEP endpoint and re-streams it as SRT, enabling integration between WebRTC and SRT-based workflows. This is particularly useful for:

- Converting WebRTC streams to professional broadcast formats
- Integrating WebRTC sources into SRT-based production pipelines
- Low-latency streaming to SRT consumers
- Building bridges between web-based and broadcast infrastructure

## Features

- **WHEP Input**: Consumes WebRTC streams via the WHEP protocol
- **SRT Output**: Outputs to SRT with configurable parameters
- **Audio Processing**: Automatically handles audio decoding, conversion, and AAC encoding
- **Multi-track Support**: Handles multiple audio tracks via audio mixing (liveadder)
- **Continuous Output**: Silent audio source ensures continuous stream even without input
- **Docker Support**: Ready-to-use Docker image with all dependencies included
- **Flexible Configuration**: Supports both `whepsrc` and `whepclientsrc` implementations

## Prerequisites

### Native Build Requirements

- **Rust** (1.83+ recommended, using 2024 edition)
- **GStreamer 1.24+** with the following plugins:
  - gstreamer-plugins-base
  - gstreamer-plugins-good
  - gstreamer-plugins-bad
  - gstreamer-plugins-ugly
  - gstreamer-libav
  - gstreamer-nice
- **Development libraries**:
  - libssl-dev
  - libgstreamer1.0-dev
  - libgstreamer-plugins-base1.0-dev
  - libgstreamer-plugins-bad1.0-dev

### GStreamer Rust Plugins

This project requires GStreamer Rust plugins from [gst-plugins-rs](https://gitlab.freedesktop.org/gstreamer/gst-plugins-rs):
- `gst-plugin-webrtc` (provides `whepclientsrc` with WHEP feature)

**Note**: The `Cargo.toml` currently uses a git dependency pinned to a specific commit SHA (`e136005b108ec85bdc8bc533c551f56ef978e950`) because the WHEP signaller feature is not yet available in the official crate release. This will be updated to use the published crate once the feature is available in the next official release.

## Building

### From Source

```bash
# Clone the repository
git clone <repository-url>
cd whep-srt

# Build the project
cargo build --release

# The binary will be at target/release/whep-srt
```

### Using Docker

```bash
# Build the Docker image
docker build -t whep-srt .

# Run the container
docker run -it whep-srt -i <WHEP_ENDPOINT_URL> -o <SRT_OUTPUT_URL>
```

## Usage

### Basic Usage

```bash
./whep-srt -i <WHEP_INPUT_URL> -o <SRT_OUTPUT_URL>
```

### Command Line Options

| Option | Description | Default |
|--------|-------------|---------|
| `-i, --input-url` | WHEP source URL (required) | - |
| `-o, --output-url` | SRT output stream URL | `srt://0.0.0.0:1234?mode=listener` |
| `--dot-debug` | Output debug .dot files of the pipeline | `false` |

### Examples

**Listen for SRT connections on port 1234 (default):**
```bash
./whep-srt -i http://localhost:8889/mystream/whep
```

**Push to a specific SRT destination:**
```bash
./whep-srt -i http://localhost:8889/mystream/whep \
  -o "srt://192.168.1.100:5000?mode=caller"
```

**Using Docker with port mapping:**
```bash
docker run -p 1234:1234/udp whep-srt \
  -i http://host.docker.internal:8889/mystream/whep \
  -o "srt://0.0.0.0:1234?mode=listener"
```

**Running the included debug script:**
```bash
# Edit run.sh to configure your WHEP endpoint
./run.sh
```

## Pipeline Architecture

The application dynamically constructs a GStreamer pipeline that:

1. **WHEP Source**: Connects to the WHEP endpoint using `whepsrc` or `whepclientsrc` (configurable)
2. **Dynamic Pad Handling**: Detects and handles audio/video tracks as they become available
3. **Audio Processing Chain**:
   - Decodes incoming audio tracks using `decodebin`
   - Converts audio to F32LE format at 48kHz
   - Mixes multiple audio tracks using `liveadder`
   - Adds a silent audio test source to ensure continuous output
   - Encodes to AAC using `avenc_aac`
4. **Output Chain**:
   - Muxes audio into MPEG-TS using `mpegtsmux`
   - Sends to SRT destination via `srtsink`

**Pipeline String (when using whepsrc):**
```
whepsrc → [dynamic audio pads] → decodebin → audioconvert → audioresample →
capsfilter → liveadder ← audiotestsrc (silence) → avenc_aac → aacparse →
mpegtsmux → queue → srtsink
```

**Pipeline String (when using whepclientsrc):**
```
whepclientsrc → [dynamic audio pads] → decodebin → audioconvert → audioresample →
capsfilter → liveadder ← audiotestsrc (silence) → avenc_aac → aacparse →
mpegtsmux → queue → srtsink
```

## Configuration

### SRT Parameters

The SRT output URL supports standard SRT URI parameters:

- `mode=listener` - Wait for incoming connections (default)
- `mode=caller` - Connect to a remote SRT receiver
- `latency=<ms>` - Set SRT latency buffer (default: 100ms)
- Additional parameters supported by GStreamer's [srtsink element](https://gstreamer.freedesktop.org/documentation/srt/srtsink.html)

### WHEP Source Selection

The application supports two WHEP source implementations (configurable in [src/main.rs:56](src/main.rs#L56)):

- **whepclientsrc** (currently enabled) - From `gst-plugin-webrtc` - Newer implementation using signaller interface (will eventually replace whepsrc)
- **whepsrc** - From `gst-plugin-webrtchttp` - Original WebRTC implementation based on webrtcbin

Toggle between them by changing the `whepsrc` boolean variable in the code. Note: `whepclientsrc` requires the plugin to be registered via `gstrswebrtc::plugin_register_static()` as shown in [src/main.rs:65](src/main.rs#L65).

### Supported Codecs

**Audio Input (via RTP):**
- OPUS (default, 48kHz)

**Video Input (via RTP):**
- VP8, VP9 (default)
- H.264, H.265
- AV1

*Note: Video tracks are currently sent to `fakesink` and not included in SRT output.*

## Development

### Debug Logging

Enable GStreamer debug output using environment variables:

```bash
# Show all debug output
GST_DEBUG=*:DEBUG ./whep-srt -i <WHEP_URL>

# Show WHEP-specific debug output
GST_DEBUG=*whep*:DEBUG ./whep-srt -i <WHEP_URL>

# Save debug log to file
GST_DEBUG_FILE=debug.log GST_DEBUG=*:DEBUG ./whep-srt -i <WHEP_URL>

# Generate pipeline visualization (DOT files) using the --dot-debug flag
./whep-srt -i <WHEP_URL> --dot-debug

# Or set the environment variable directly
GST_DEBUG_DUMP_DOT_DIR=./ ./whep-srt -i <WHEP_URL>
```

### Pipeline Visualization

The application automatically generates GraphViz DOT files of the pipeline on state changes and errors when the `--dot-debug` flag is used. The files are timestamped with the format `<epoch>-<state>.dot` (e.g., `1729000000-Playing.dot`, `1729000000-error.dot`). Convert them to SVG for visualization:

```bash
# Convert a DOT file to SVG
dot -Tsvg 1729000000-error.dot -o pipeline.svg

# Or use xdot for interactive viewing
xdot 1729000000-error.dot
```

### Code Structure

- [src/main.rs](src/main.rs) - Main application logic
  - Command-line argument parsing ([Args struct](src/main.rs#L10-L24))
  - Pipeline construction and management
  - Dynamic pad handling for audio/video tracks
  - Event loop and error handling
  - Debug pipeline visualization ([debug_pipeline function](src/main.rs#L351-L368))

## Known Issues & Limitations

- **Video handling**: Video tracks are currently discarded (sent to `fakesink`)
- **Brittle pad matching**: Hardcoded check for `src_5` pad name (see [src/main.rs:157](src/main.rs#L157))
- **Git dependency**: Uses pinned git commit for `gst-plugin-webrtc` until WHEP feature is available in published crate
- **Audio-only output**: Only audio is currently muxed to SRT output

## Troubleshooting

**Missing GStreamer elements:**
If you get errors about missing elements, ensure all required GStreamer plugins are installed:
```bash
gst-inspect-1.0 whepclientsrc
gst-inspect-1.0 srtsink
gst-inspect-1.0 avenc_aac
```

**SRT connection issues:**
Check your firewall settings and ensure the SRT port (default 1234/udp) is accessible.

**No audio output:**
Enable debug logging to see if audio pads are being created and linked correctly.

## Future Improvements

- [ ] Add video support to SRT output
- [ ] More robust pad detection and handling
- [ ] Configuration file support
- [ ] Metrics and monitoring
- [ ] Reconnection handling
- [ ] Support for published crates.io versions of gst-plugins-rs

## License

See the LICENSE file for details.

## Author

Per Enstedt <<per.enstedt@eyevinn.se>>

Developed at [Eyevinn Technology](https://www.eyevinn.se/)

## Contributing

Contributions are welcome! Please feel free to submit issues or pull requests.

## Related Projects

- [GStreamer](https://gstreamer.freedesktop.org/) - Multimedia framework
- [gst-plugins-rs](https://gitlab.freedesktop.org/gstreamer/gst-plugins-rs) - GStreamer plugins written in Rust
- [WHEP Specification](https://www.ietf.org/archive/id/draft-murillo-whep-00.html) - WebRTC HTTP Egress Protocol
- [SRT Alliance](https://www.srtalliance.org/) - Secure Reliable Transport protocol
