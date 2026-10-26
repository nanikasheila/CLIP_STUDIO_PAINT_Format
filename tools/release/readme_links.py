"""Check, or make absolute, the relative links of README files that leave the repository.

A README inside a package (crates.io, PyPI) or a release archive is read
away from the repository, where links like ``docs/spec/`` lead nowhere.

    python tools/release/readme_links.py FILE...
        fail if any FILE has a relative link
    python tools/release/readme_links.py --base URL [--keep NAME]... FILE...
        rewrite each relative link to URL + link in place, except links to
        the files NAME (shipped next to FILE), then check as above
"""

import argparse
import re
import sys
from pathlib import Path

LINK = re.compile(r"\]\(([^)\s]+)\)")
FENCE = re.compile(r"^\s*(```|~~~)")


def is_relative(target: str) -> bool:
    return not re.match(r"^([a-z][a-z0-9+.-]*:|#)", target, re.IGNORECASE)


def process(text: str, base: str | None, keep: set[str]) -> tuple[str, list[str]]:
    out, bad, fenced = [], [], False
    for line in text.splitlines(keepends=True):
        if FENCE.match(line):
            fenced = not fenced
        if not fenced:

            def replace(match: re.Match) -> str:
                target = match.group(1)
                if not is_relative(target) or target.split("#")[0] in keep:
                    return match.group(0)
                if base is not None:
                    return f"]({base}{target})"
                bad.append(target)
                return match.group(0)

            line = LINK.sub(replace, line)
        out.append(line)
    return "".join(out), bad


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--base", help="URL the relative links are resolved against (ends with /)")
    parser.add_argument("--keep", action="append", default=[], help="file shipped next to the README")
    parser.add_argument("files", nargs="+", type=Path)
    args = parser.parse_args()
    failed = False
    for path in args.files:
        text, _ = process(path.read_text(encoding="utf-8"), args.base, set(args.keep))
        if args.base is not None:
            path.write_text(text, encoding="utf-8")
        _, bad = process(text, None, set(args.keep))
        for target in bad:
            print(f"{path}: relative link {target}", file=sys.stderr)
        failed |= bool(bad)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
