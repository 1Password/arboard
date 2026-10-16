import subprocess
import time

payload = bytes(range(256)) * 8192
name = "application/x-arboard-large"
writer = subprocess.Popen(
    ["xclip", "-quiet", "-selection", "clipboard", "-target", name],
    stdin=subprocess.PIPE,
    stdout=subprocess.DEVNULL,
    stderr=subprocess.PIPE,
)
try:
    writer.stdin.write(payload)
    writer.stdin.close()
    deadline = time.monotonic() + 5
    while True:
        assert writer.poll() is None, writer.stderr.read()
        result = subprocess.run(
            ["xclip", "-out", "-selection", "clipboard", "-target", "TARGETS"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        if result.returncode == 0 and name.encode() in result.stdout:
            break
        assert time.monotonic() < deadline, result.stderr
        time.sleep(0.01)
    result = subprocess.run(
        ["cargo", "run", "--quiet", "--example", "get_raw", "--no-default-features", "--", name],
        stdout=subprocess.PIPE,
        check=True,
    )
    assert result.stdout == payload
    print(f"Read {len(payload)} native bytes through X11 INCR without changes.")
finally:
    writer.terminate()
    writer.wait(timeout=5)
