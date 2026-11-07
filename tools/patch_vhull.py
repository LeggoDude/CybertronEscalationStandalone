"""Adds (or refreshes) the vhull lines of the classes' data files
(assets/models/<class>/<class>.txt), as build_class.py writes them, without
building the classes again: each vehicle form's rigid body's box
(class_data.physics_hull, at its form's MeshScale), after its vbody line.

usage: python patch_vhull.py <TransGame\\CookedPC> <foc_esc\\assets>
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import class_data as cd  # noqa: E402
from build_class import hull_line  # noqa: E402

if len(sys.argv) != 3:
    print(__doc__)
    sys.exit(1)
cooked, assets = sys.argv[1:3]
mp = cd.Mp(cooked)
for cls in ("infiltrator", "scientist", "titan", "destroyer"):
    path = os.path.join(assets, "models", cls, f"{cls}.txt")
    if not os.path.exists(path):
        continue
    out, n = [], 0
    for line in open(path, encoding="utf-8").read().splitlines():
        f = line.split("\t")
        if f[0] == "vhull":
            continue
        out.append(line)
        if f[0] != "vbody":
            continue
        form = f[1]
        idx = next((i for i in mp.by_name.get(form, []) if "FormBlueprint" in mp.pkg.class_name(mp.pkg.exports[i - 1])), 0)
        if not idx:
            print(f"  ! {cls}: {form} not found")
            continue
        pr = mp.props(idx)
        ms = cd._obj(pr.get("FormMeshSet"))
        v = {"hull": cd.physics_hull(mp, cd._obj(pr.get("PhysicsAsset"))),
             "scale": float(mp.props(ms).get("MeshScale", 1.0)) if ms else 1.0}
        hull = hull_line(form, v)
        if hull:
            out.append(hull)
            n += 1
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(out) + "\n")
    print(f"{cls}: {n} vhull lines")
