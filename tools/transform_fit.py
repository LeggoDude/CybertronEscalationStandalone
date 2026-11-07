"""Where each vehicle's car is drawn against its robot while they change form: the
lift written to <class>.txt's vtransform lines (build_class.py's --transforms pass;
src/car.rs and src/customize.rs draw the car that much higher, see there).

The demo draws both halves of a transformation from the player's feet: the robot's
clip on his robot (clips.glb, at his class's size) and the car's on the car model
(vehicle_<name>.glb at main.rs's 1 / 2.2). The game doesn't: its forms hang their
meshes on the pawn by their own offsets (the robot's MeshTranslation under its
collision cylinder's middle, the vehicle's under the middle of its mesh's bounds),
and the vehicle comes into the world high up where the robot's middle was and drops
to the ground (folding up), or the pawn moves up to where the robot stands and both
meshes are carried up to it from where the car was (unfolding: TnPawn.
MoveToSafeTransformationLocation's mesh offset, blended out over half a second). So
where the demo shows them, the car appears and goes well below the robot's pieces
it is made of (Bumblebee's more than a metre down).

The lift that puts it back is measured from the clips themselves, as they play when
the two are both seen: folding up, from the car's appearing to the robot's going
(its car_show to robot_hide); unfolding, from the robot's appearing to the car's
going (robot_show to car_hide). At a few moments between, every bone of the car is
matched to the nearest of the robot's (not the robot's bones left at his feet), and
the lift is the one that brings the car's bones closest to the robot's (the median
distance, the mean over those moments). Never lower than the clip has it: where
the robot's pieces are lower still (crouched as he unfolds, Onslaught's), the car
would sink into the ground.

usage: python transform_fit.py <foc_esc/assets> <class> [<plot dir>]
       prints each vehicle's lifts (and with a plot dir, draws each vehicle's
       halves side on, as the demo places them with its lifts)
(build_class.py writes them: its main build and its --transforms pass.)
"""
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from gltf_io import Doc, quat_to_mat  # noqa: E402

# The car model's size in the demo (main.rs's VEHICLE_SCALE).
VEHICLE_SCALE = 1.0 / 2.2
# Robot bones this close to the ground are the ones his clips leave at his feet
# (the root's, the feet's IK targets), wherever his pieces are.
FEET = 0.15
# The moments measured across each window, and the lifts tried (metres).
SAMPLES = 7
LIFTS = np.arange(-1.5, 3.0001, 0.01)


class Clips:
    """A glTF's skeleton posed by its clips: each joint's place at any time of any of
    them, the model drawn at `scale` (and `lift` metres up) from the feet."""

    def __init__(self, path, scale, lift=0.0):
        self.doc = Doc(path)
        g = self.doc.g
        self.nodes = g["nodes"]
        self.roots = g["scenes"][g.get("scene", 0)]["nodes"]
        self.joints = sorted(g["skins"][0]["joints"])
        self.place = np.diag([scale, scale, scale, 1.0])
        self.place[1, 3] = lift
        self.anims = {}
        for i, a in enumerate(g["animations"]):
            tracks = {}
            for c in a["channels"]:
                s = a["samplers"][c["sampler"]]
                tracks[(c["target"]["node"], c["target"]["path"])] = (
                    self.doc.acc(s["input"]).ravel().astype(np.float64),
                    self.doc.acc(s["output"]).astype(np.float64))
            self.anims[i] = tracks
            self.anims[a["name"]] = tracks

    def length(self, clip):
        return max(ts[-1] for ts, _ in self.anims[clip].values())

    def _value(self, tracks, node, path, t):
        track = tracks.get((node, path))
        if track is None:
            return None
        ts, vs = track
        if t <= ts[0] or len(ts) == 1:
            return vs[0]
        if t >= ts[-1]:
            return vs[-1]
        i = int(np.searchsorted(ts, t, side="right")) - 1
        f = (t - ts[i]) / (ts[i + 1] - ts[i])
        a, b = vs[i], vs[i + 1]
        if path == "rotation":
            if np.dot(a, b) < 0:
                b = -b
            q = a * (1 - f) + b * f
            return q / np.linalg.norm(q)
        return a * (1 - f) + b * f

    def joint_places(self, clip, t):
        """Where each joint is at `t` seconds into `clip` (metres, x right, y up)."""
        tracks = self.anims[clip]
        places = {}

        def walk(i, parent):
            n = self.nodes[i]
            m = np.eye(4)
            q = self._value(tracks, i, "rotation", t)
            tr = self._value(tracks, i, "translation", t)
            sc = self._value(tracks, i, "scale", t)
            q = n.get("rotation", [0, 0, 0, 1]) if q is None else q
            tr = n.get("translation", [0, 0, 0]) if tr is None else tr
            sc = n.get("scale", [1, 1, 1]) if sc is None else sc
            m[:3, :3] = quat_to_mat(q) * np.asarray(sc)
            m[:3, 3] = tr
            here = parent @ m
            places[i] = here[:3, 3]
            for c in n.get("children", []):
                walk(c, here)

        for r in self.roots:
            walk(r, self.place)
        return np.array([places[j] for j in self.joints])


def best_lift(pairs):
    """The lift that brings the car's bones closest to the robot's over these
    moments' (robot bones, car bones): its score (metres) too."""
    scores = np.zeros(len(LIFTS))
    for robot, car in pairs:
        robot = robot[robot[:, 1] > FEET]
        if len(robot) == 0 or len(car) == 0:
            continue
        for k, lift in enumerate(LIFTS):
            moved = car + np.array([0.0, lift, 0.0])
            d = np.sqrt(((moved[:, None, :] - robot[None, :, :]) ** 2).sum(-1)).min(1)
            scores[k] += np.median(d)
    scores /= max(len(pairs), 1)
    k = int(np.argmin(scores))
    lift = LIFTS[k]
    # (Between the steps: the parabola through the best and its neighbours.)
    if 0 < k < len(LIFTS) - 1:
        a, b, c = scores[k - 1], scores[k], scores[k + 1]
        den = a - 2 * b + c
        if den > 1e-9:
            lift += 0.5 * (a - c) / den * (LIFTS[1] - LIFTS[0])
    return float(lift), float(scores[k])


def window(start, end):
    """The moments measured between the one's appearing and the other's going, each
    the robot's and the car's (when they don't overlap, the one as it comes and the
    other as it goes)."""
    if end <= start:
        return [(end, start)]
    return [(t, t) for t in np.linspace(start, end, SAMPLES)]


def read_class(assets, cls):
    """The class line's robot scale and each vehicle line's fields."""
    path = os.path.join(assets, "models", cls, f"{cls}.txt")
    scale, vehicles = 1.0, []
    for line in open(path, encoding="utf-8").read().splitlines():
        f = line.split("\t")
        if f[0] == "class":
            scale = float(f[2])
        elif f[0] == "vehicle" and len(f) >= 17:
            vehicles.append(f)
    return scale, vehicles


def fit_vehicle(assets, robot, f):
    """One vehicle line's (folding up, unfolding) lifts, their scores, and the
    posed models (for the plots)."""
    to_vehicle, to_robot = int(f[3]), int(f[4])
    robot_hide, robot_show = float(f[5]), float(f[7])
    car_show, car_hide = float(f[9]), float(f[11])
    car = Clips(os.path.join(assets, *f[2].split("/")), VEHICLE_SCALE, float(f[16]))
    out = []
    for robot_clip, car_clip, (start, end) in ((to_vehicle, "to_vehicle", (car_show, robot_hide)),
                                                (to_robot, "to_robot", (robot_show, car_hide))):
        # (Folding up the car comes as the robot goes; unfolding the robot comes as
        # the car goes.)
        moments = window(start, end)
        if car_clip == "to_robot":
            moments = [(c, r) for r, c in moments]
        pairs = [(robot.joint_places(robot_clip, rt), car.joint_places(car_clip, ct)) for rt, ct in moments]
        out.append(best_lift(pairs))
    return out, car


def fit(assets, cls):
    """Each of the class's vehicles' (form, folding-up lift, unfolding lift, their
    scores)."""
    scale, vehicles = read_class(assets, cls)
    robot = Clips(os.path.join(assets, "models", cls, "clips.glb"), scale)
    out = []
    for f in vehicles:
        ((up, up_score), (down, down_score)), _ = fit_vehicle(assets, robot, f)
        out.append((f[1], max(up, 0.0), max(down, 0.0), up_score, down_score))
    return out


def line(assets, robot, f):
    """The vtransform line for vehicle line `f` (its fields), `robot` the class's
    clips.glb as Clips."""
    ((up, _), (down, _)), _ = fit_vehicle(assets, robot, f)
    return "\t".join(["vtransform", f[1], f"{max(up, 0.0):.3f}", f"{max(down, 0.0):.3f}"])


def plot(assets, cls, plot_dir):
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    scale, vehicles = read_class(assets, cls)
    robot = Clips(os.path.join(assets, "models", cls, "clips.glb"), scale)
    os.makedirs(plot_dir, exist_ok=True)
    for f in vehicles:
        ((up, _), (down, _)), car = fit_vehicle(assets, robot, f)
        up, down = max(up, 0.0), max(down, 0.0)
        to_vehicle, to_robot = int(f[3]), int(f[4])
        robot_hide, robot_show = float(f[5]), float(f[7])
        car_show, car_hide = float(f[9]), float(f[11])
        rows = (("to_vehicle", to_vehicle, up, (car_show, robot_hide)),
                ("to_robot", to_robot, down, (robot_show, car_hide)))
        fig, axes = plt.subplots(4, 4, figsize=(13, 14))
        for r, (car_clip, robot_clip, lift, (start, end)) in enumerate(rows):
            times = [t for t, _ in window(start, end)]
            times = [times[0], times[len(times) // 2], times[-1], max(start, end) + 0.15]
            for c, t in enumerate(times):
                for row, shown_lift in ((2 * r, 0.0), (2 * r + 1, lift)):
                    ax = axes[row][c]
                    rp = robot.joint_places(robot_clip, t)
                    cp = car.joint_places(car_clip, t) + np.array([0, shown_lift, 0])
                    ax.scatter(-rp[:, 2], rp[:, 1], s=3, c="tab:blue")
                    ax.scatter(-cp[:, 2], cp[:, 1], s=3, c="tab:red")
                    ax.axhline(0, color="k", lw=0.5)
                    ax.set_xlim(-3.5, 3.5)
                    ax.set_ylim(-1, 5)
                    ax.set_aspect("equal")
                    ax.set_title(f"{car_clip} {t:.2f}s lift {shown_lift:+.2f}", fontsize=8)
        name = os.path.basename(f[2]).replace(".glb", "")
        fig.suptitle(f"{f[1]}: robot blue, car red (rows: as before, lifted)")
        fig.tight_layout()
        fig.savefig(os.path.join(plot_dir, f"{cls}_{name}.png"), dpi=60)
        plt.close(fig)


if __name__ == "__main__":
    if len(sys.argv) not in (3, 4):
        raise SystemExit(__doc__)
    for form, up, down, us, ds in fit(sys.argv[1], sys.argv[2]):
        print(f"{form:32s} folding up {up:+.3f} ({us:.3f})  unfolding {down:+.3f} ({ds:.3f})")
    if len(sys.argv) == 4:
        plot(sys.argv[1], sys.argv[2], sys.argv[3])
