# Image fixtures

These original fixtures contain a 3 × 2 red image. They use the repository's licenses.
The JPEG fixture is lossy. The WebP fixture is lossless.
The animated GIF and WebP fixtures have a red first frame and a green second frame.
The `red16.png` fixture has 16-bit RGB channels.
The Qt fixture contains a PNG image after the QDataStream non-null marker.

To regenerate these files, install Pillow in your Python environment.
Run `python3 tests/fixtures/generate.py` from the repository root.
The tests use the committed files and do not require Python or Pillow.
