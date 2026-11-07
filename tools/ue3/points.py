"""Extract player start, Escalation spawn points, AI path nodes and health stations
from a level's BASE package. usage: python points.py out.json <map>_BASE_m.xxx

Health stations are the Escalation pickup factories that sell health
(TnPickupFactoryPurchasable with PresetType PPT_Health), listed as "HealthStation":
[x, y, z, yaw] (Unreal units; yaw in 1/65536 turns)."""
import collections, json, struct, sys
from ue3pkg import Package
from layout import props_any, vec

KINDS = ("TnSurvivalSpawnPoint", "PathNode", "TnTacticalPathNode", "TnSpawnClusterSurvival", "AirPathNode")

if __name__ == "__main__":
    pkg = Package(sys.argv[2])
    out = collections.defaultdict(list)
    for e in pkg.exports:
        c = pkg.class_name(e)
        if c == "TnPickupFactoryPurchasable":
            try:
                pr, _ = props_any(pkg, e, (8, 16, 4, 12, 0, 30))
            except ValueError:
                continue
            if pr.get("PresetType") == "PPT_Health" and "Location" in pr:
                yaw = struct.unpack("<iii", pr["Rotation"][2][:12])[1] if "Rotation" in pr else 0
                out["HealthStation"].append([round(v) for v in vec(pr["Location"][2])] + [yaw])
            continue
        if c in KINDS or "PlayerStart" in c:
            try:
                pr, _ = props_any(pkg, e, (4, 30))
            except ValueError:
                continue
            if "Location" in pr:
                out[c].append([round(v) for v in vec(pr["Location"][2])])
    for k, v in out.items():
        print(k, len(v))
    json.dump(out, open(sys.argv[1], "w"))
