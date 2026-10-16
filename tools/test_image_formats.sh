#!/usr/bin/env bash
# Run only with a disposable clipboard server. This test replaces its contents.
set -euo pipefail

backend=${1:?usage: test_image_formats.sh x11-or-wayland --replace-clipboard}
[[ ${2:-} == --replace-clipboard ]] || {
	echo "This test replaces the clipboard. Pass --replace-clipboard to continue." >&2
	exit 1
}
case "$backend" in
	x11) unset WAYLAND_DISPLAY ;;
	wayland) unset DISPLAY ;;
	*) echo "Expected x11 or wayland" >&2; exit 1 ;;
esac

cargo build --quiet --examples --features wayland-data-control
examples="${CARGO_TARGET_DIR:-target}/debug/examples"
owner=
cleanup() {
	if [[ -n "$owner" ]]; then
		kill "$owner" 2>/dev/null || true
		wait "$owner" 2>/dev/null || true
		owner=
	fi
}
trap cleanup EXIT

start_owner() {
	local mime=$1 file=$2
	cleanup
	if [[ "$backend" == x11 ]]; then
		xclip -quiet -selection clipboard -target "$mime" "$file" >/dev/null 2>&1 &
	else
		wl-copy --foreground --type "$mime" <"$file" >/dev/null 2>&1 &
	fi
	owner=$!
	local ready=0
	for ((attempt=0; attempt<100; attempt++)); do
		kill -0 "$owner"
		if [[ "$backend" == x11 ]]; then
			if xclip -selection clipboard -out -target TARGETS 2>/dev/null | grep -Fxq "$mime"; then ready=1; break; fi
		else
			if wl-paste --list-types 2>/dev/null | grep -Fxq "$mime"; then ready=1; break; fi
		fi
		sleep 0.05
	done
	[[ "$ready" == 1 ]] || { echo "Clipboard owner did not become ready" >&2; exit 1; }
}

check_fixture() {
	local mime=$1 file=$2
	start_owner "$mime" "$file"
	local size output
	size=$(wc -c <"$file")
	output=$("$examples/get_image")
	[[ "$output" == "Image: 3 x 2, 24 RGBA bytes" ]]
	output=$("$examples/image_limits" "$size" 3 2 24)
	[[ "$output" == "Image: 3 x 2, 24 RGBA bytes" ]]
	for bounds in "$((size - 1)) 3 2 24" "$size 2 2 24" "$size 3 1 24" "$size 3 2 23"; do
		# An aborted INCR transfer can leave xclip waiting on the old requestor.
		# Give each rejection case an independent owner.
		start_owner "$mime" "$file"
		read -r -a values <<<"$bounds"
		if output=$("$examples/image_limits" "${values[@]}" 2>&1); then
			echo "Expected an image limit error for $mime ($bounds)" >&2
			exit 1
		fi
		[[ "$output" == *ImageTooLarge* ]] || {
			echo "Unexpected error for $mime ($bounds): $output" >&2
			exit 1
		}
	done
	echo "$backend $mime: $output; exact bounds passed"
}

for format in png jpeg webp bmp gif; do
	check_fixture "image/$format" "tests/fixtures/red.$format"
done
check_fixture image/gif tests/fixtures/animated.gif
check_fixture image/webp tests/fixtures/animated.webp
check_fixture image/jpg tests/fixtures/red.jpeg
check_fixture image/x-ms-bmp tests/fixtures/red.bmp
check_fixture image/x-bmp tests/fixtures/red.bmp
check_fixture application/x-qt-image tests/fixtures/qt-image.bin
check_fixture application/x-qt-image tests/fixtures/red.png

# The large payload forces X11 INCR rather than a single property transfer.
mkdir -p target/image-fixtures
cat tests/fixtures/red.png >target/image-fixtures/large.png
dd if=/dev/zero bs=1048576 count=1 >>target/image-fixtures/large.png 2>/dev/null
check_fixture image/png target/image-fixtures/large.png
rm target/image-fixtures/large.png
rmdir target/image-fixtures
