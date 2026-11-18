#!/usr/bin/env python3
"""Create a valid ICO file with a simple solid color image."""

from PIL import Image
import os

# Create a simple 256x256 image (solid color with alpha)
img = Image.new('RGBA', (256, 256), (0, 100, 200, 255))  # Blue background

# Save as ICO with multiple sizes
img.save(
    os.path.join(os.path.dirname(__file__), 'icon.ico'),
    format='ICO',
    sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
)
print("Created valid icon.ico")