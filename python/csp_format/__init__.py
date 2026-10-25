"""Read CLIP STUDIO PAINT files (.clip, .cmc) with the Python standard library only.

    from csp_format import read_clip
    with read_clip("page.clip") as clip:
        for layer in clip.iter_layers():
            found = clip.layer_pixels(layer)   # (Image, (x, y)) or None
            ...

Images are plain RGBA (``Image.width``, ``height``, ``rgba``); with Pillow,
``PIL.Image.frombytes("RGBA", (image.width, image.height), bytes(image.rgba))``.
Animation (timelines, cels, keyframes, 2D camera): :mod:`csp_format.animation`.
The format is described in ``docs/spec``; the Rust crate in ``crates/``
also writes and renders files.
"""
from .animation import Animation, FrameState, Timeline, Track, read_animation
from .clip import Canvas, ClipFile, Layer, read_clip
from .cmc import CanvasNode, CmcFile, CmcProject, PageSlot, read_cmc
from .container import Container
from .errors import CspFormatError
from .image import Image, decode_png
from .text import Attributes, TextBlock
from .vector import Point, VectorObject, parse_vector_data

__all__ = [
    "Animation", "Attributes", "Canvas", "CanvasNode", "ClipFile", "CmcFile", "CmcProject", "Container",
    "CspFormatError", "FrameState", "Image", "Layer", "PageSlot", "Point", "TextBlock", "Timeline", "Track", "VectorObject",
    "decode_png", "parse_vector_data", "read_animation", "read_clip", "read_cmc",
]
