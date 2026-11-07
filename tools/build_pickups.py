"""Builds the weapon pickups and the purchase points' goods alone, as
build_assets.py does in a full build: models/weapons/<weapon>.glb with
models/weapons/sockets.txt (build_weapon_pickups: needs models/robot.glb, built
already), props/ammo_crate_heavy.glb and the equipment pickups
(build_station_props: the meshes from umodel glTF exports of ESC_Ark_BASE_m and
mp_shared, their texture sets from ENV_MP_PropsShared_p, WEP_KamikazeMine_p and
PROP_NEU_Pickups_p, in folders named after them next to the package export or in
FOC_MORE_EXPORTS; the weapons' texture sets as build_assets.py finds them), and the
support items' models in use (build_equipment_models: the mines and the Decimator's
ball, from glTF and .psa exports of all_shared.xxx and an Escalation map package).

usage: python build_pickups.py <umodel package export dir> <foc_esc/assets dir>
"""
import os
import sys

if len(sys.argv) != 3:
    print(__doc__)
    sys.exit(1)
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import build_assets  # noqa: E402  (reads its folders from sys.argv)

build_assets.build_weapon_pickups()
build_assets.build_station_props()
build_assets.build_equipment_models()
