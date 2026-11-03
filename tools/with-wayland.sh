#!/usr/bin/env bash
# Run a command against a private headless Wayland compositor.
set -euo pipefail

runtime="$PWD/target/wayland-formats-$$"
mkdir -p "$runtime"
chmod 700 "$runtime"
export XDG_RUNTIME_DIR="$runtime"
export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
unset DISPLAY WAYLAND_DISPLAY SWAYSOCK

sway -c /dev/null >"$runtime/sway.log" 2>&1 &
compositor=$!
cleanup() {
	kill "$compositor" 2>/dev/null || true
	wait "$compositor" 2>/dev/null || true
	rm -rf "$runtime"
}
trap cleanup EXIT

for ((attempt = 0; attempt < 100; attempt++)); do
	if ! kill -0 "$compositor" 2>/dev/null; then
		cat "$runtime/sway.log" >&2
		exit 1
	fi
	for socket in "$runtime"/wayland-*; do
		if [[ -S "$socket" ]]; then
			export WAYLAND_DISPLAY="${socket##*/}"
			"$@"
			exit $?
		fi
	done
	sleep 0.1
done
cat "$runtime/sway.log" >&2
echo "The Wayland compositor did not create a socket." >&2
exit 1
