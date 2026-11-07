"""Mirrored copies of Jazz's animations, for switching the gun to his left arm.

The game swaps hands (TnPlayerController.ChangeHandedness) by mirroring Jazz's whole
animation through a TnAnimNodeHandednessMirror node in his animation tree: every
clip plays left for right, so he holds and aims the gun with his left arm. Bevy
can't mirror a pose, so this writes every clip in robot.glb a second time, mirrored,
into robot_mirror.glb (same order, same skeleton, no meshes); main.rs blends
between the two sets.

How a clip is mirrored: a bone's pose in the model is reflected across the model's
middle (the exported skeleton is symmetric about its Z axis) and given to its
partner on the other side (L_* <-> R_*; C_* bones keep theirs). Bone frames on
the two sides aren't simply mirror images of each other, so each bone b gets a fixed
correction T_b = (S Ref_b)^-1 Ref_b', from the symmetric reference pose (the
exported bind pose, Ref, in model space; S the reflection), which makes the
reference pose mirror onto itself. In a bone's own (local) terms that works out to
    Local'_b' = T_parent(b)^-1  Local_b  T_b
so each clip is mirrored bone by bone without posing the whole skeleton. The
additive clips (gun recoil and parts, the aim offset turns) hold changes put in
front of a bone's rotation and added to its position; those mirror as
    Rot'_b' = A Rot_b A^-1,  Pos'_b' = A Pos_b       (A = T_parent(b)^-1's 3x3).

usage: python mirror_anims.py <robot.glb> <reference skeleton .gltf> <out .glb> [additive clip names, comma-separated]
(build_assets.py runs it itself and knows which of its clips are additive.)
"""
import sys

import numpy as np

from gltf_io import Doc, Writer, node_local, quat_to_mat

# Reflection across the skeleton's middle (its Z axis runs left to right).
S = np.diag([1.0, 1.0, -1.0, 1.0])


def partner(name):
    if name.startswith("L_"):
        return "R_" + name[2:]
    if name.startswith("R_"):
        return "L_" + name[2:]
    return name


def mat_to_quat(m):
    t = np.trace(m)
    if t > 0:
        s = np.sqrt(t + 1) * 2
        q = [(m[2, 1] - m[1, 2]) / s, (m[0, 2] - m[2, 0]) / s, (m[1, 0] - m[0, 1]) / s, s / 4]
    else:
        i = int(np.argmax(np.diag(m)))
        j, k = (i + 1) % 3, (i + 2) % 3
        s = np.sqrt(max(1 + m[i, i] - m[j, j] - m[k, k], 1e-12)) * 2
        q = [0.0, 0.0, 0.0, 0.0]
        q[i] = s / 4
        q[j] = (m[j, i] + m[i, j]) / s
        q[k] = (m[k, i] + m[i, k]) / s
        q[3] = (m[k, j] - m[j, k]) / s
    q = np.array(q)
    return q / np.linalg.norm(q)


def model_space(g, names):
    """Each named bone's matrix in the skeleton's space (from its top bone's parent)."""
    parent = {}
    for i, n in enumerate(g["nodes"]):
        for c in n.get("children", []):
            parent[c] = i
    out = {}
    for i, n in enumerate(g["nodes"]):
        if n.get("name") not in names:
            continue
        m = node_local(n)
        j = parent.get(i)
        while j is not None and g["nodes"][j].get("name") in names:
            m = node_local(g["nodes"][j]) @ m
            j = parent.get(j)
        out[n["name"]] = m
    return out


def sample(d, sampler, times):
    t = d.acc(sampler["input"]).ravel()
    v = d.acc(sampler["output"]).astype(np.float64)
    if len(t) == 1:
        return np.repeat(v[:1], len(times), axis=0)
    out = []
    for x in times:
        i = int(np.clip(np.searchsorted(t, x, side="right") - 1, 0, len(t) - 2))
        a = float(np.clip((x - t[i]) / max(t[i + 1] - t[i], 1e-9), 0.0, 1.0))
        q0, q1 = v[i], v[i + 1]
        if v.shape[1] == 4 and np.dot(q0, q1) < 0:
            q1 = -q1
        q = q0 * (1 - a) + q1 * a
        out.append(q / np.linalg.norm(q) if v.shape[1] == 4 else q)
    return np.array(out)


def mirror(robot_glb, ref_gltf, out_glb, additive, bones=None):
    """`additive(index, name)` says whether a clip is additive; `bones`, the
    skeleton's bones (by default the nodes any of the file's skins uses: a file of
    clips alone, without a skin, names them)."""
    d = Doc(robot_glb)
    g = d.g
    names = {n.get("name"): i for i, n in enumerate(g["nodes"])}
    root = next(i for i, n in enumerate(g["nodes"]) if n.get("name") == "Root")
    # The skeleton: bones are the nodes any skin uses.
    if bones is None:
        bones = {g["nodes"][j]["name"] for s in g["skins"] for j in s["joints"]}
    bones = set(bones)
    parent = {}
    for i, n in enumerate(g["nodes"]):
        for c in n.get("children", []):
            parent[c] = i
    bone_parent = {g["nodes"][i]["name"]: (g["nodes"][parent[i]]["name"] if g["nodes"][parent[i]].get("name") in bones else None)
                   for i in (names[b] for b in bones)}

    ref = Doc(ref_gltf).g
    ref_m = model_space(ref, bones)
    T = {}
    for b in bones:
        p = partner(b)
        if b not in ref_m or p not in ref_m:
            raise SystemExit(f"bone {b} or its partner missing from the reference skeleton")
        T[b] = np.linalg.inv(S @ ref_m[b]) @ ref_m[p]
    Tp_inv = {b: (np.linalg.inv(T[bone_parent[b]]) if bone_parent[b] else S) for b in bones}

    w = Writer(share=True)
    # The skeleton nodes again, with the same names and hierarchy as robot.glb, so
    # Bevy matches the mirrored clips to the model's bones by name.
    # (In the bones' order by name: the same file each time it is built.)
    keep = [root] + [names[b] for b in sorted(bones)]
    new = {}
    for i in keep:
        n = g["nodes"][i]
        new[i] = w.node({k: v for k, v in n.items() if k in ("name", "translation", "rotation", "scale")},
                        root=(i == root))
    for i in keep:
        kids = [new[c] for c in g["nodes"][i].get("children", []) if c in new]
        if kids:
            w.g["nodes"][new[i]]["children"] = kids

    def rest(b):
        n = g["nodes"][names[b]]
        return np.array(n.get("translation", [0, 0, 0]), float), np.array(n.get("rotation", [0, 0, 0, 1]), float)

    w.g["animations"] = []
    for ai, a in enumerate(g["animations"]):
        tracks = {}
        for c in a["channels"]:
            node = g["nodes"][c["target"]["node"]].get("name")
            if node in bones:
                tracks.setdefault(node, {})[c["target"]["path"]] = a["samplers"][c["sampler"]]
        samplers, channels = [], []

        def emit(bone, path, times, values):
            samplers.append({"input": w.accessor(times.astype(np.float32), minmax=True),
                             "output": w.accessor(values.astype(np.float32)), "interpolation": "LINEAR"})
            channels.append({"sampler": len(samplers) - 1, "target": {"node": new[names[bone]], "path": path}})

        is_add = additive(ai, a.get("name", ""))
        # The clip's length: every track spans it, so the mirrored clip lasts as long
        # as the original (Bevy loops a clip over its longest track; a longer one
        # would make a looping clip stand still at the end of each cycle).
        length = max((float(d.acc(s["input"]).max()) for s in a["samplers"]), default=0.0)
        if length <= 0.0:
            length = 1.0
        # Base clips cover every bone (a bone the clip leaves alone holds its rest pose,
        # which mirrored is its partner's pose too).
        sources = list(tracks) if is_add else sorted(bones)
        for b in sources:
            tr = tracks.get(b, {})
            times = np.unique(np.concatenate([d.acc(s["input"]).ravel() for s in tr.values()])) if tr else np.array([0.0, length])
            t0, r0 = rest(b)
            pos = sample(d, tr["translation"], times) if "translation" in tr else np.repeat([t0], len(times), 0)
            rot = sample(d, tr["rotation"], times) if "rotation" in tr else np.repeat([r0], len(times), 0)
            A = Tp_inv[b][:3, :3]
            out_p, out_r = [], []
            for p, q in zip(pos, rot):
                if is_add:
                    m = A @ quat_to_mat(q) @ np.linalg.inv(A)
                    out_r.append(mat_to_quat(m))
                    out_p.append(A @ p)
                else:
                    m = np.eye(4)
                    m[:3, :3] = quat_to_mat(q)
                    m[:3, 3] = p
                    m = Tp_inv[b] @ m @ T[b]
                    out_r.append(mat_to_quat(m[:3, :3]))
                    out_p.append(m[:3, 3])
            out_r = np.array(out_r)
            # (no flips between neighbouring keys)
            for k in range(1, len(out_r)):
                if np.dot(out_r[k], out_r[k - 1]) < 0:
                    out_r[k] = -out_r[k]
            if is_add:
                out_r = out_r * np.sign(out_r[:, 3:4] + 1e-12)
                if "rotation" in tr:
                    emit(partner(b), "rotation", times, out_r)
                if "translation" in tr:
                    emit(partner(b), "translation", times, np.array(out_p))
            else:
                emit(partner(b), "rotation", times, out_r)
                emit(partner(b), "translation", times, np.array(out_p))
        w.g["animations"].append({"name": a.get("name", "") + "_mirror", "samplers": samplers, "channels": channels})
    size = w.save(out_glb)
    return size, len(w.g["animations"])


if __name__ == "__main__":
    if len(sys.argv) not in (4, 5):
        sys.exit(__doc__)
    names = set(sys.argv[4].split(",")) if len(sys.argv) == 5 else set()
    size, n = mirror(sys.argv[1], sys.argv[2], sys.argv[3], lambda i, name: name in names)
    print(f"wrote {sys.argv[3]} ({n} clips, {size / 1e6:.1f} MB)")
