#!/usr/bin/env bash
set -euo pipefail

backend="${1:?Specify x11 or wayland}"
shift

run_tests() {
    # The legacy aggregate assumes a desktop clipboard manager. Native-client
    # integration covers transfers without one; all other library tests run here.
    timeout 180s cargo test "$@" --lib -- --skip tests::all_tests
    timeout 180s cargo test "$@" --test linux_clipboard -- --nocapture
}

case "$backend" in
    x11)
        unset DISPLAY WAYLAND_DISPLAY
        display_file="$(mktemp)"
        Xvfb -displayfd 3 -screen 0 1280x1024x24 -nolisten tcp -ac 3>"$display_file" &
        server_pid=$!
        cleanup() {
            kill "$server_pid" 2>/dev/null || true
            wait "$server_pid" 2>/dev/null || true
            rm -f "$display_file"
        }
        trap cleanup EXIT
        for ((attempt = 0; attempt < 100; attempt++)); do
            kill -0 "$server_pid"
            if [[ -s "$display_file" ]]; then
                export DISPLAY=":$(cat "$display_file")"
                break
            fi
            sleep 0.05
        done
        : "${DISPLAY:?Xvfb did not allocate a display}"
        run_tests "$@"
        ;;
    wayland)
        unset DISPLAY WAYLAND_DISPLAY
        export XDG_RUNTIME_DIR
        XDG_RUNTIME_DIR="$(mktemp -d)"
        export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
        sway -c /dev/null >"$XDG_RUNTIME_DIR/sway.log" 2>&1 &
        compositor_pid=$!
        cleanup() {
            kill "$compositor_pid" 2>/dev/null || true
            wait "$compositor_pid" 2>/dev/null || true
            cat "$XDG_RUNTIME_DIR/sway.log"
            rm -f "$XDG_RUNTIME_DIR/sway.log"
            rmdir "$XDG_RUNTIME_DIR"
        }
        trap cleanup EXIT
        for ((attempt = 0; attempt < 100; attempt++)); do
            kill -0 "$compositor_pid"
            for socket in "$XDG_RUNTIME_DIR"/wayland-*; do
                if [[ -S "$socket" ]]; then
                    export WAYLAND_DISPLAY="${socket##*/}"
                    break 2
                fi
            done
            sleep 0.05
        done
        : "${WAYLAND_DISPLAY:?The compositor did not create a Wayland socket}"
        run_tests "$@"
        ;;
    *)
        echo "Unknown backend: $backend" >&2
        exit 2
        ;;
esac
