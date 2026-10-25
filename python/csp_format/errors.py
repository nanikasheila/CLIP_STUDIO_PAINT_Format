"""Errors raised by the reader."""


class CspFormatError(Exception):
    """The file is not a valid CLIP STUDIO PAINT file, or uses a layout this reader does not know."""
