# syntax=docker/dockerfile:1
FROM lukemathwalker/cargo-chef:latest-rust-1.92-bookworm AS chef
WORKDIR /app
RUN apt update && apt install lld clang -y

FROM chef AS planner
COPY . .
# Compute a lock-like file for our project
RUN cargo chef prepare  --recipe-path recipe.json

FROM chef AS builder
ENV DEBIAN_FRONTEND=noninteractive

# Update default packages
RUN apt-get update

# Get Ubuntu packages
RUN apt-get install -y \
    build-essential \
    curl \
    pkg-config

# Get GStreamer-related packages
RUN apt-get install -y libssl-dev \
    libunwind-dev \
    libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev \
    libgstreamer-plugins-bad1.0-dev \
    gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    gstreamer1.0-libav \
    gstreamer1.0-tools \
    gstreamer1.0-x \
    gstreamer1.0-alsa \
    gstreamer1.0-gl \
    gstreamer1.0-gtk3 \
    gstreamer1.0-qt5 \
    gstreamer1.0-pulseaudio \
    gstreamer1.0-nice

WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
# Build our project dependencies, not our application!
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
# Build our application
RUN cargo build --release

FROM debian:trixie-slim AS runtime
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y \
    libgstreamer1.0-0 \
    gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    gstreamer1.0-libav \
    gstreamer1.0-tools \
    gstreamer1.0-nice \
    && rm -rf /var/lib/apt/lists/*

# Assert that the transitively-installed libsrt meets the patched revision for
# CVE-2026-55869 / CVE-2026-55868. libsrt is pulled in via
# gstreamer1.0-plugins-bad; we check a MINIMUM version (not an exact pin, which
# would break on every future security bump) and fail the build loudly if the
# base image ever resolves an older, unpatched libsrt.
RUN set -eux; \
    installed="$(dpkg-query -W -f='${Package}\t${Version}\n' 'libsrt*' 2>/dev/null | awk 'NR==1{print $2}')"; \
    if [ -z "$installed" ]; then \
      echo "ERROR: libsrt not present in runtime image; expected it via gstreamer1.0-plugins-bad" >&2; \
      exit 1; \
    fi; \
    required="1.5.4-1+deb13u1"; \
    if ! dpkg --compare-versions "$installed" ge "$required"; then \
      echo "ERROR: installed libsrt $installed is older than required $required (CVE-2026-55869/CVE-2026-55868)" >&2; \
      exit 1; \
    fi; \
    echo "libsrt $installed satisfies >= $required"

WORKDIR /app
COPY --from=builder /app/target/release/whep-srt ./whep-srt

ENV GST_DEBUG=1
ENTRYPOINT [ "./whep-srt" ]
