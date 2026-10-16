#!/usr/bin/env bash
# Run inside an isolated Linux desktop or container. The tests overwrite its clipboard.
set -euo pipefail
cd "$(dirname "$0")/.."

for command in cargo Xvfb sway xclip wl-copy timeout; do
	command -v "$command" >/dev/null
done

run_dir="$PWD/target/watcher-native-$$"
mkdir -p "$run_dir/runtime"
chmod 700 "$run_dir/runtime"
server=
client=
cleanup() {
	if [ -n "$client" ]; then
		kill "$client" 2>/dev/null || true
		wait "$client" 2>/dev/null || true
	fi
	if [ -n "$server" ]; then
		kill "$server" 2>/dev/null || true
		wait "$server" 2>/dev/null || true
	fi
}
trap cleanup EXIT

start_x11() {
	: >"$run_dir/display"
	Xvfb -displayfd 3 -screen 0 1280x1024x24 -ac "$@" \
		3>>"$run_dir/display" >"$run_dir/xvfb.log" 2>&1 &
	server=$!
	for _ in {1..100}; do
		kill -0 "$server"
		[ ! -s "$run_dir/display" ] || break
		sleep 0.1
	done
	test -s "$run_dir/display"
	DISPLAY=":$(cat "$run_dir/display")"
	export DISPLAY
}

check_disconnect() {
	local backend="$1"
	: >"$run_dir/$backend-disconnect.log"
	timeout 5s cargo run --quiet --example watch --all-features \
		>>"$run_dir/$backend-disconnect.log" 2>&1 &
	client=$!
	for _ in {1..40}; do
		kill -0 "$client"
		! grep -q "Initial clipboard event" "$run_dir/$backend-disconnect.log" || break
		sleep 0.1
	done
	grep -q "Initial clipboard event" "$run_dir/$backend-disconnect.log"
	kill "$server"
	wait "$server" || true
	server=
	local status=0
	wait "$client" || status=$?
	client=
	test "$status" -ne 0
	test "$status" -ne 124
	grep -q "Error:" "$run_dir/$backend-disconnect.log"
	echo "$backend disconnect: native error delivered"
}

unset WAYLAND_DISPLAY WAYLAND_SOCKET
cargo test --lib watcher --all-features
cargo build --example watch --all-features
start_x11
cargo test --test watcher --all-features -- --ignored
cargo run --quiet --example watch --all-features -- --self-test
check_disconnect X11

start_x11 -extension XFIXES
if cargo run --quiet --example watch --all-features >"$run_dir/unsupported.log" 2>&1; then
	echo "The watcher accepted a server without XFixes." >&2
	exit 1
fi
grep -q ClipboardNotSupported "$run_dir/unsupported.log"
echo "X11 without XFixes: explicit unsupported error"
kill "$server"
wait "$server" || true
server=

export XDG_RUNTIME_DIR="$run_dir/runtime"
export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
unset DISPLAY
sway -c /dev/null >"$run_dir/sway.log" 2>&1 &
server=$!
socket=
for _ in {1..100}; do
	kill -0 "$server"
	socket=$(find "$XDG_RUNTIME_DIR" -maxdepth 1 -type s -name 'wayland-*' -print -quit)
	[ -z "$socket" ] || break
	sleep 0.1
done
test -n "$socket"
WAYLAND_DISPLAY=$(basename "$socket")
export WAYLAND_DISPLAY
cargo test --test watcher --all-features -- --ignored
cargo run --quiet --example watch --all-features -- --self-test
check_disconnect Wayland
echo "Native watcher validation passed. Logs: $run_dir"
