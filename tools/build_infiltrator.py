"""Build the Infiltrator's characters: build_class.py for the Infiltrator (see there).

usage: python build_infiltrator.py <umodel export dir> <CookedPC> <TransGame> <foc_esc/assets>
       python build_infiltrator.py --umodel <CookedPC> <TransGame> [<umodel export dir>]
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import build_class  # noqa: E402

if __name__ == "__main__":
    args = sys.argv[1:]
    if len(args) in (3, 4) and args[0] == "--umodel":
        print("\n".join(build_class.umodel_commands("infiltrator", *args[1:])))
    elif len(args) == 4:
        build_class.main("infiltrator", *args)
    else:
        raise SystemExit(__doc__)
