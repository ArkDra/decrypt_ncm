# File safety fixtures

These fixed NCM fixtures contain 20 milliseconds of generated mono silence in
FLAC or MP3 format and synthetic metadata (`Song`, `Artist`, `Album`). The normal cover
is a 1×1 PNG. They contain no downloaded music.

The silence was generated with FFmpeg, and the key and metadata blocks were
encrypted with OpenSSL AES-128-ECB. The tests consume the fixed fixtures without
reimplementing the NCM cipher or requiring either executable.

- `song.ncm`: valid FLAC audio, metadata and cover.
- `song-mp3.ncm`: valid MP3 audio with the same metadata and cover.
- `unsupported-format.ncm`: the same audio, but metadata declares `ncm`.
- `mismatched-format.ncm`: FLAC audio with metadata declaring `mp3`.
- `bad-cover.ncm`: valid FLAC audio and metadata with an invalid, nonempty cover,
  provoking a tagging error after the temporary audio file has been written.
