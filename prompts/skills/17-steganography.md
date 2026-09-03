# 17. Steganography

When: Analyzing image (PNG, JPG, BMP, GIF), audio (WAV, MP3), video, or text files where hidden data or covert channels are suspected.

## Mental model
Steganography is embedding confidential data within redundant perceptual slack of a host medium: modifying least-significant bits, transform coefficients, or metadata in ways imperceptible to human senses but mathematically extractable.

## Attack arc
- Surface & Structural Inspection:
  - *Metadata & Comments:* Inspect EXIF data (`exiftool`), PNG chunks (`pngcheck -v`), GIF comment extensions.
  - *Trailing Appended Data (Overlay):* Check for data appended past the formal End-of-File marker (`IEND` for PNG, `\xFF\xD9` for JPEG, `%%EOF` for PDF). Extract using `binwalk` or `dd`.
  - *Polyglots / Embedded Archives:* Concatenated ZIP/RAR files inside images (extractable with `7z x` or `unzip`).
- Spatial Domain (Bit-Plane) Steganography:
  - *PNG / BMP LSB Analysis:* Inspect Least Significant Bits across Color channels (R, G, B, Alpha) and planes (0–7) using `zsteg -a <image.png>` or `stegsolve`.
  - *Custom LSB Extractors:* Write Python PIL / NumPy scripts to extract specific bit sequences (e.g. MSB-first, row-major vs. column-major, interlaced).
  - *Color Palette & Alpha Channel:* Check for anomalous palette indices or non-255 Alpha channel byte patterns.
- Frequency & Transform Domain (JPEG / Lossy):
  - Extract data embedded in Discrete Cosine Transform (DCT) coefficients using `steghide extract -sf <image.jpg>` (brute-force passphrase with `stegcracker`), `outguess`, `jphide`, `jsteg`, or `F5`.
- Audio & Acoustic Steganography:
  - *Spectrogram Analysis:* Convert audio to spectrograms using `sox <audio.wav> -n spectrogram -o spec.png` or Audacity to view visual text or QR codes painted in high-frequency bands.
  - *WAV LSB / DTMF:* Decode LSB in PCM samples or decode DTMF dialing tones.
- Text & Unicode Covert Channels:
  - *Zero-Width Characters:* Detect and decode zero-width spaces (`\u200B`), non-joiners (`\u200C`), and joiners (`\u200D`) hidden within standard text.
  - *Whitespace Steganography:* Decode trailing tabs and spaces (`stegsnow`).

## Key techniques & primitives
- Zsteg Comprehensive Scan: `zsteg -a image.png | grep -E "flag|CTF|text"` (checks b1,b2,b3,b4 in rgb, bgr, msb, lsb).
- Spectrogram Generation via Sox: `sox input.wav -n spectrogram -Y 300 -X 50 -o out.png`.

## Tells & signals
- `pngcheck` reports `additional data after IEND chunk` = appended secret or ZIP payload.
- File size significantly larger than typical image dimensions (e.g. 500x500 PNG is 5 MB).
- High-frequency visual hiss in audio spectrograms revealing ASCII text or shapes.
