#!/usr/bin/env python3
"""Generate disposable moving pictures and distinct tones; no operator media."""

from pathlib import Path
import subprocess


def main():
    for name, color, frequency in (("a", "red", 440), ("b", "blue", 660)):
        path = Path(f"/fixtures/{name}.mp4")
        temporary = path.with_name(f".{name}.tmp.mp4")
        subprocess.run([
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
            "-f", "lavfi", "-i",
            f"testsrc2=size=640x360:rate=25,drawbox=x=0:y=0:w=96:h=96:color={color}:t=fill",
            "-f", "lavfi", "-i", f"sine=frequency={frequency}:sample_rate=48000",
            "-t", "180", "-c:v", "libx264", "-preset", "ultrafast",
            "-crf", "32", "-g", "25", "-pix_fmt", "yuv420p",
            "-c:a", "aac", "-b:a", "96k", "-movflags", "+faststart", str(temporary),
        ], check=True)
        temporary.replace(path)
        print(f"Generated {path.name}: 180 s, {color} marker, {frequency} Hz")


if __name__ == "__main__":
    main()
