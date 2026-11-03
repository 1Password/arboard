FROM rust:1-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc pkg-config libwayland-dev xvfb xauth xclip sway wl-clipboard \
    && rm -rf /var/lib/apt/lists
RUN useradd --create-home --uid 1000 tester && \
    mkdir -p /home/tester/cargo /tmp/.X11-unix && \
    chown tester:tester /home/tester/cargo && chmod 1777 /tmp/.X11-unix

ENV CARGO_HOME=/home/tester/cargo
USER tester
WORKDIR /workspace
