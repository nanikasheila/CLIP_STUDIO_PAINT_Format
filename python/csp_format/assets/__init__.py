"""CSP's files other than canvases, with the standard library only (A14).

    from csp_format.assets import subtool, serial, colorset
    db = subtool.SubToolDb.open("EditImageTool.todb")
    for index, depth, path in db.sub_tools():
        node = db.nodes[index]
        print("/".join(path + [node.name]), db.settings(node.variant_id)["BrushSize"])

Mirrors ``crates/csp-assets`` (reading; colour sets, serialised documents
and tars are also written). Encrypted material data is never decrypted.
"""
from . import c2f, colorset, material, serial, subtool, tar

__all__ = ["c2f", "colorset", "material", "serial", "subtool", "tar"]
