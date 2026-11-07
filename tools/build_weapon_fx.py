"""The effects the demo's weapons and abilities spawn as they are used, read from the
game for src/levelfx.rs (its spawned effects, `FxSystems`): the EMP grenade's
(the Infiltrator's FlashBangAbility, see src/emp.rs), the speed boost its Speed
Redirector upgrade gives, the Riot Cannon's (src/riot.rs): its muzzle flash, its
round's flight and its explosion, the Orbital Pistol's (src/orbital.rs): its
muzzle flash, its beam, its impact, the steam it lets off while it is out and the
marks it leaves on what it hits, the Titan's tanks' (src/car.rs, src/riot.rs):
their hover jets, boosters, jumps and landings, and their cannon's muzzle flash,
shells and blasts, the Scientist's jets' (src/car.rs, src/riot.rs): their
hover jets, afterburners and contrails, and their homing rockets' muzzle flash,
flight and blasts, the Infiltrator's cars' and the Destroyer's trucks' (src/car.rs):
their hover jets, boosters, jumps and landings, the trucks' rockets' (src/riot.rs):
their muzzle flash, flight and blasts, and the Titan's, Scientist's and Destroyer's guns'
(src/arsenal.rs, src/riot.rs; SYSTEMS has them gun by gun), with the Gear
Shredder's blade as it is left stuck where it struck, and the Chaos-Rift Combustor's
bomb, clusters and impact marker (MESH_SYSTEMS); the enemies' (src/ai: their
guns' and rounds', the effects their animations play, the Guard's scanner beam's
material); and the maps' traps' (src/traps.rs: the flame jets, the combustion
chamber's fires, the electric fences' lightning, the sentries' guns and rockets).

usage: python build_weapon_fx.py <TransGame\\CookedPC> <umodel export folder> <foc_esc\\assets>
       python build_weapon_fx.py --umodel <TransGame\\CookedPC> <umodel export folder>
           prints the umodel commands that export the meshes it needs

The particle systems, their materials and textures are read straight from the
packages (MP_Array_BASE_m.xxx, which a multiplayer map cooks them into, and the
shared multiplayer packages), as tools/levelfx.py reads a map's, leaving out the
emitters the game never draws, having no shaders for them (from the packages' shader
caches: ue3/shadercache.py); the meshes their mesh emitters
draw come from umodel's glTF export of all_shared.xxx:

    umodel -game=trans -export -gltf -path=<CookedPC> -out=<folder> all_shared.xxx Sphere_03_MESH
    umodel -game=trans -export -gltf -path=<CookedPC> -out=<folder> all_shared.xxx MuzzleFlash_STATMuzzleFlash_Cap_STAT
    umodel -game=trans -export -gltf -path=<CookedPC> -out=<folder> all_shared.xxx PROJ_SawBladeProjectile_STAT
    umodel -game=trans -export -gltf -path=<CookedPC> -out=<folder> all_shared.xxx PROJMESH_GrenadeBall_STAT
    umodel -game=trans -export -gltf -path=<CookedPC> -out=<folder> all_shared.xxx PROJMESH_Grenade_STAT
    umodel -game=trans -export -gltf -path=<CookedPC> -out=<folder> all_shared.xxx Shell_STAT

(and the other mesh emitters' meshes the same way, from whichever package has
them: --umodel prints the commands for them all; the build names any it can't
find, and keeps those an earlier build wrote into the assets' models/fx; Oblivion's
exploding barrel's debris, Debris1_STAT ... Debris4_STAT and
CoverBLockCLarge_debris_02_STAT, from ESC_Ark_ART_m.xxx). The few drawn as a section
of a mesh, MESH_CUTS (the Diffraction Barrier's), are cut from its export.

It writes, as levelfx.py does for a map (see there):

  * fx/weapons_fx.txt - the materials and particle systems (no placements: the
    game spawns them where they are needed)
  * shaders/levelfx/weapons.wgsl - their materials (put into shaders/levelfx.wgsl,
    shader defs FXM_WEAPONS_<n>)
  * textures/levelfx/ - their textures
  * models/fx/<mesh>.fxmesh - the meshes of their mesh emitters
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)
import levelfx  # noqa: E402
import particles  # noqa: E402

# (ESC_Ark_BASE_m.xxx, Oblivion's, for the Escalation pickups' materials: the
# Diffraction Barrier's shield, MP_Barrier_Distortion_MATINST; and the other
# Escalation maps' packages for the enemies' effects, which each map cooks for the
# enemies it fields: the Guard's in Ancients' and Ignition's, ESC_Temple and
# ESC_Gladiator.)
PACKAGES = ["MP_Array_BASE_m.xxx", "all_shared.xxx", "mp_shared.xxx", "ESC_Ark_BASE_m.xxx",
            "ESC_Temple_BASE_m.xxx", "ESC_Iacon_BASE_m.xxx", "ESC_Gladiator_BASE_m.xxx",
            # (Oblivion's art package, last, for its exploding barrel's own effects.)
            "ESC_Ark_ART_m.xxx"]

# The systems, by the game objects that spawn them, each with whether to leave out
# the emitters the game itself never draws, having no shaders for them (see
# LevelFx._undrawable and ue3/shadercache.py: checked against the game's shader
# caches; the emitters with a DynamicParameter module whose material doesn't read
# them, the boosters' cross beams and the heavy weapons' dark smoke among them, and
# three of the rockets' and the Pulsar Cannon's fires, sub-UV emitters whose
# material has shaders for plain sprites only). The check once also left out the
# emitters whose material reads dynamic parameters that no module of theirs sets
# (some 200, the Riot Cannon's brightening glows, the Path Blaster's lightning and
# the heavies' tracer among them), which the game draws.
# The EMP grenade's are kept whole, as they were built before the check (it leaves
# none of them out either), but for their scorch mark (UNSEEN_DECALS). A third item, where
# there is one, has options checked against the game's videos: emitters to leave
# out besides ("leave_out"), emitters the check would leave out that the videos
# show all the same ("shown"), whether the emitters the game marks to be killed
# on completing (bKillOnCompleted) are, their particles going as their emitter's
# loops end ("kill_on_completed"; elsewhere they live out their lives), and
# emitters drawn dimmer than their data has them, by name and the factor their
# starting colour is taken by ("dim": where the game's own picture shows them far
# darker than the demo draws them).
# How much dimmer the ground smash's flat flash is drawn (its "dim").
DIM_SMASH_FLASH = 0.004

SYSTEMS = [
    # TnProjectileFlashBang's mesh (WEP_FlashBangs_p.FlashBang_PROJMESH): flying,
    # going off, and the arc shown while the ability button is held.
    ("FlashBang_Grenade_FX", False),
    ("Exp_FlashGrenade_FX", False),
    ("Grenade_Trajectory_FX", False),
    # TnBuffFlashBang's ApplyFx, on whoever the blast catches.
    ("EMPAttack_FX", False),
    # TnBuffGrenadeIncreaseSpeed's ApplyFx (Speed Redirector).
    ("SpeedBoost_Trail_FX", False),
    # The Riot Cannon (WEP_Bazooka_p): its Bazooka_WEPMESH's muzzle flash
    # (TnWeaponEffectPlayMuzzleFX), and its round's Bazooka_PROJMESH: flying
    # (FlightEffect, the energy ball and its trail of smoke and sparks) and going
    # off (ExplosionEffect, with its scorch mark).
    ("RiotBazooka_MuzzleFlash_FX", True),
    ("Tracer_Bazooka_FX", True),
    ("RiotCannon_ImpactExp_FX", True),
    # The Orbital Pistol (WEP_TracerDart_p, the game's "TracerDart"): its
    # TracerDart_WEPMESH's muzzle flash (TnWeaponEffectPlayMuzzleFX), its tracer
    # (TnWeaponEffectPlayTracerFX: a beam out to where the shot struck), its impact
    # (TnWeaponEffectPlayImpactFX's DefaultParticleSystem), the steam it lets off on
    # its ShellEject socket while it is out (TnWeaponEffectPlayFXOnSocket, from
    # ActiveStart to ActiveStop), and the marks on what it hits (TnBuffHardLocked,
    # TnBuffHardLocked2 and TnBuffHardLocked3's ApplyFx, in FX_Multiplayer_p).
    ("TracerDartMuzzle_FX", True),
    ("TracerDart_TracerBeam_FX", True),
    ("TracerDart_Exp_FX", True),
    ("TracerDart_Idle_FX", True),
    ("OrbitalPistol_Marker1_FX", True),
    ("OrbitalPistol_Marker2_FX", True),
    ("OrbitalPistol_Marker3_FX", True),
    # The Titan's tanks (their TnTankFormBlueprints): the hover jets on their HoverFX
    # sockets (HoverFX), the boosters (BoostFx: Grimlock's own), the jump (JumpFX)
    # and the landing (LandFX); and their cannon (WEP_TankCannon_p): its
    # TankCannon_Vehicle_WEPMESH's muzzle flash (TnWeaponEffectPlayMuzzleFX) and its
    # shells' TankShell_PROJMESH: flying (FlightEffect) and going off
    # (ExplosionEffect).
    ("hover_tank01_FX", True),
    ("Afterburner_D_FX", True),
    ("Afterburner_A_Grim_FX", True),
    ("Jump_tank01_FX", True),
    ("Vehland_High_Lrg_FX", True),
    ("Tank_MuzzleFlash_FX", True),
    ("Tracer_HomRocket_FX", True),
    ("TankCannon_ImpactExp_FX", True),
    # The Scientist's jets (their TnPlaneFormBlueprints): the hover jets on their
    # HoverFX sockets while they hover (TnPlaneForm.Hovering.PlayHoverFx), and flying
    # (Flying.PlayBoostFx: BoostFx) the afterburners, the contrails off the wing tips
    # and the glow on the nose (Shield_F's Trail_SatHomingRocket_FX); and their
    # homing rockets (WEP_HomingRocketVh_p): HomingRocket_Vehicle_WEPMESH's muzzle
    # flash, and the rockets' HomingRocketVh_PROJMESH: flying (FlightEffect,
    # Tracer_HomRocket_FX, above) and going off (ExplosionEffect).
    ("hover_flyer01_A_FX", True),
    ("hover_flyer02_A_FX", True),
    ("hover_plane_FX", True),
    ("hover_plane_sm_FX", True),
    ("Afterburner_A_FX", True),
    ("Afterburner_A_Sm_FX", True),
    ("Afterburner_Round_Sm_FX", True),
    ("Afterburner_Shockwave_FX", True),
    ("Afterburner_Vortex_FX", True),
    ("Sharpshot_Boost_round_FX", True),
    ("Trails_Jet_A_FX", True),
    ("Trails_Jet_D_FX", True),
    ("Trail_SatHomingRocket_FX", True),
    ("MuzzleFlash_HomRocket_FX", True),
    ("HomRocket_Explosion_FX", True),
    # The Infiltrator's cars and the Destroyer's trucks (their TnCarFormBlueprints and
    # TnTruckFormBlueprints, TnCarForm): the hover jets on their HoverFX sockets while
    # they hover (Hovering.PlayHoverFx), the boosters and trails while they drive on
    # their wheels (Driving.PlayBoostFx: BoostFx), the jump (JumpFX) and the
    # landings, on their wheels or hovering, after a short time in the air or a long
    # one (LowLandingFX, HighLandingFX, LowLandingHoverFX; Driving and Hovering's
    # OnLandComplete). (Afterburner_A_FX, Afterburner_D_FX, Trail_SatHomingRocket_FX,
    # Jump_tank01_FX and Vehland_High_Lrg_FX are above.)
    ("CarHover_A_01_FX", True),
    ("CarHover_D_01_FX", True),
    ("CarHover_A_Sm_FX", True),
    ("CarHover_D_Sm_FX", True),
    ("HoverTank_SM_D_01_FX", True),
    ("Booster_Jazz_FX", True),
    ("bumble_boost_small1_FX", True),
    ("Booster_1_FX", True),
    ("Booster_Thin_SSwipe_FX", True),
    ("Booster_Thin_Short_FX", True),
    ("Booster_Thin_Med_FX", True),
    ("Booster_Thin_UMagnus_FX", True),
    ("Trail_Vehicles_FX", True),
    ("Trail_Vehicle_FX", True),
    ("Char_DustTrails_FX", True),
    ("Ram_Dino_FX", True),
    ("Truck_ram_FX", True),
    ("OptimusG1_Exhaust_FX", True),
    ("Hound_Exhaust_FX", True),
    ("CarJump_01_FX", True),
    ("CarJump_Small_FX", True),
    ("Jump_FX", True),
    ("Vehland_Low_Med_FX", True),
    ("Vehland_Low_XSm_FX", True),
    ("Vehland_High_Med_FX", True),
    ("Vehland_High_XSm_FX", True),
    # The Destroyer's trucks' rockets (WEP_RocketVh_p): Rocket_Vehicle_WEPMESH's
    # muzzle flash, and the rockets' Rocket_PROJMESH: flying (FlightEffect) and going
    # off (ExplosionEffect).
    ("MuzzleFlash_Blaster_FX", True),
    ("Tracer_HomRocketVH_FX", True),
    ("Squib_HomRocket_01_FX", True),
    # The X18 Scrapmaker (WEP_HeavyMG_p.HeavyMG_WEPMESH): with every shot its muzzle
    # flash, the effect on its IdleFX socket, its tracer, its impact and its shell
    # (Shell4_FX on ShellEject).
    ("X18_Scrapmaker_MuzzleFlash_FX", True),
    ("X18_Scrapmaker_Idle_FX", True),
    ("Scrapmaker_Tracer_FX", True),
    ("X18_Scrapmaker_Squib_FX", True),
    ("Shell4_FX", True),
    # The Energon Harvester (WEP_DrainGun_p.DrainGun_WEPMESH, on the EMP Shotgun's
    # effects): its muzzle flash, tracers and impacts, the glow round its muzzle
    # while it is ready, the burst on its MeleeSocket as it drains an enemy (FX_Death's
    # Harvester_PlayerHeal_FX) and its reload's steam.
    ("EMPShotgun_MuzzleFlash_FX", True),
    ("EMPShotgun_Tracer_FX", True),
    ("EMPShotgun_Squib_FX", True),
    ("EMPShotgun_Idle_FX", True),
    ("Harvester_PlayerHeal_FX", True),
    ("ReloadSteam_Harvester_FX", True),
    # The E.D.K. TechVolt (WEP_ChainBeam_p.ChainBeam_WEPMESH and its data's
    # ChainTemplate): its beam (the muzzle flash and tracer in one, looping while it
    # fires), where it strikes, its chains to further enemies, its glow while ready
    # and its reload's steam.
    ("TechvoltEmitter_MuzzleFlashAndTracer_FX", True),
    ("TechvoltEmitter_Squib_FX", True),
    ("TechvoltEmitter_ExtensionTracer_FX", True),
    ("TechvoltEmitter_Idle_FX", True),
    ("ReloadSteam_Tech_FX", True),
    # The Corrosive Slime Cannon (WEP_AoeSlow_p): its muzzle flash, the drip on its
    # MeleeSocket while ready, its reload's steam, and its round (AoeSlow_PROJMESH):
    # flying, going off, and the arc fine aim shows (TrajectoryPreviewEffect).
    ("Corrosive_Slime_Cannon_MuzzleFlash_FX", True),
    ("SlimeCannon_Idle_FX", True),
    ("ReloadSteam_Slime_FX", True),
    ("Corrosive_Slime_Cannon_Projectile_FX", True),
    ("SlimeCannon_Exp_FX", True),
    ("Slime_Trajectory_FX", True),
    # The Gear Shredder (WEP_Sawblade_p): each blade loaded onto it as it charges
    # (TnSawBladeExtension's SawBladeEffect), its blades (SawBlade_PROJMESH): flying,
    # their impact, the path fine aim shows; its reload's steam.
    ("SawBlade_Weapon_FX", True),
    ("SawBlade_Trail_FX", True),
    ("Squib_PlasmaCannon_Med_FX", True),
    ("SawBlade_Trajectory_FX", True),
    ("ReloadSteam_Med_FX", True),
    ("ReloadSteam_Med02_FX", True),
    # The Thermo Rocket Cannon (WEP_RocketLauncher_p): its muzzle flash, the exhaust
    # out of its back (MuzzleExhaust), its reload's smoke and steam, and its rockets
    # (RocketLauncher_PROJMESH): flying and going off.
    ("MuzzleFlash_RocketLauncher_FX", True),
    ("RocketLauncherExhaust_FX", True),
    ("ReloadSteam_DarkSmke_FX", True),
    ("ReloadSteam_FX", True),
    ("Tracer_RocketLauncher_FX", True),
    ("Squib_RocketLauncher_FX", True),
    # The A-4 Pulsar Cannon (WEP_GrenadeLauncher_p): its muzzle flash, its glow while
    # ready (IdleFXSocket), and its round (GrenadeLauncher_PROJMESH): flying and going
    # off.
    ("Pulsar_MuzzleFlash_FX", True),
    ("Pulsar_Idle_FX", True),
    ("Trail_PlasmaCannon_Med_FX", True),
    ("Squib_Pulsar_Lrg_FX", True),
    # The Chaos-Rift Combustor (WEP_ClusterCannon_p): its muzzle flash, its shell
    # (Lrg_Shell1_FX on ShellEject), its bomb (ClusterCannon_PROJMESH) flying and going
    # off, its clusters' (Clusters_PROJMESH) blasts, and the arc fine aim shows.
    ("ClusterCannon_MuzzleFlash_FX", True),
    ("Lrg_Shell1_FX", True),
    ("ClusterCannon_Tracer_FX", True),
    ("ClusterCannon_ExplosionPrimary_FX", True),
    ("ClusterCannon_ExplosionSecondary_FX", True),
    ("Cluster_Trajectory_FX", True),
    # The upgrades' special shots (src/specials.rs). The Path Blaster's Lights Out
    # (HeavyPistol_SV_WEPDATA's fire mode 2 on HeavyPistol_WEPMESH): its muzzle flash,
    # tracer, impact and shell.
    ("MuzzleFlash_Sniper_FX", True),
    ("Tracer_Gambler_FX", True),
    ("Exp_LastShot_FX", True),
    ("AssaultRifleArm_Shell_FX", True),
    # The Riot Cannon's Planet Buster (TnProjectileBazookaSpecial's
    # BazookaSpecial_SP_PROJMESH): flying and going off.
    ("LightningSphere_Tracer_FX", True),
    ("EXP_PlanetBuster_FX", True),
    # The Scatter Blaster's Fireblast (fire mode 1 on Shotgun_WEPMESH: the assault
    # rifle arm's flash, by its full name, as the vehicle arm has one of the same
    # name), its tracers and impacts, and the fire on what it sets alight
    # (TnBuffOnFire's ApplyFx).
    ("FX_WPN_GUN_AssaultRifleArm_p.FX.AssaultRifleArm_MuzzleFlash_FX", True),
    ("Trail_ScatterBlaster_SP_FX", True),
    ("Squib_ScatterBlaster_SP_FX", True),
    ("Flamethrower_Impact_FX", True),
    # The support items in use (src/equipment.rs): the Thermo Mines' blast
    # (KamikazeMineKillstreak_PROJDATA's ExplosionEffect) and steam (Nav_KMine_Activate's
    # notify), the Energon Recharger's sparks (TnBuffHealthRegenKillStreak's), the
    # Intercooler's blast (its ability's OnTriggerEffect) and its buff's loop, and
    # the Dimensional Decimator's blast and black hole (TnRollerMineGravityAura's,
    # TnGravityAura's) and its end.
    ("Grenade_EXP_FX", True),
    ("MineSteam_FX", True),
    ("KillStreak_HealthRegen_FX", True),
    # (Its flashes' material, Basic_Particle_MOD_MAT, only has shaders for
    # particles with dynamic parameters, which their emitters don't set, as with
    # Intercooler_FX's lightning: drawn, see there. Its GLOW is left out: a 30 m
    # glow sprite at 11 times its colour, one every 0.1 s for its first half
    # second, that would wash the screen white from 0.1 s on, where the game's
    # videos show the lightning alone until the sphere's flash at 0.4 s and no haze
    # before it. And its sphere goes as its emitter ends, 0.6 s in, as in the
    # videos, killed on completing as the game has it, where its particle would
    # live to 1.4 s.)
    ("Intercooler_Blast_FX", True, {"leave_out": {"GLOW"}, "kill_on_completed": True}),
    # TnBuffFastAbilityCooldown's ApplyFx (the Intercooler's buff, on everyone it
    # gives it to, on his chest for its 12 s). Its lightning (Electricity,
    # Lightning_4x4_MAT) and its cyan streaks (Debris_Export_Dup, Mod_Glow2x2_MAT)
    # have no DynamicParameter module and their materials only shaders for one,
    # yet the game's videos show both round the buffed (TFOC.exe picks a sprite's
    # vertex factory by its material's use of dynamic parameters, the emitters
    # without the module giving it 1s in the demo: see ue3/shadercache.py).
    ("Intercooler_FX", True),
    ("GravityBomb_Exp_FX", True),
    ("BlackHole_FX", True),
    ("BlackHole_End_FX", True),
    # The steam an Armory Recreator lets out as it opens (src/recreator.rs: its
    # opening's HmAnimNotify_PlayEffects, on the pod's FX sockets).
    ("ReloadSteam_Titan_FX", True),
    # The class abilities (src/abilities.rs and its modules). The Whirlwind's blade's
    # trail (whirlwind.rs: the Titan's Transform_Whirlwind_ROBO's notify); the
    # Shared_SCOUT spin's boosters, stabilizers and hands' trails, which the enemies'
    # clips play too (the boosters' also burn while the Hover hovers); the Heal
    # Beam's beam and its rewards (healbeam.rs); the Hover's landing marker (hoverslam.rs); the Battle
    # Sentry's laser sight, gun, tracers and impacts, and the Rust Sentry's beam and
    # what it strikes (battlesentry.rs); the Flak Shield's break (flakshield.rs);
    # the Cloak's lightning as it goes on and comes off (cloak.rs: TnBuffCloak's
    # ApplyFx and UnapplyFx).
    ("Cloaking_FX", True),
    # A melee blow's on the striker's screen (src/melee.rs: TnDamageTypeMelee's
    # AttackerCameraEffect, TnPlayerController.ClientNotifyCausedDamage's
    # SpawnCameraEmitter: just in front of the camera, facing it).
    ("Screen_EnergonBloodsplatter_FX", True),
    ("Sword_Trail_Whirlwind_FX", True),
    # The Whirlwind's other trail (the Titan's Transform_Whirlwind_ROBO's notify at
    # 1.361 s, on WeaponTrailSocket: whirlwind.rs): Weapon_Trail_Melee_FX's Trail_Dup,
    # white-hot and refracting the scene (Faction_Whirlwind_MAT, with the
    # ENV_Metals_CUBEMAP's reflection). The check would leave it out (a DynamicParameter
    # module its material doesn't read, as the Flamethrower's FlameTip below), but the
    # game's videos of the Whirlwind show its glare and the smear round the Titan as
    # he spins. Written apart, so that the enemies' own (below) stay as they were.
    ("Weapon_Trail_Melee_FX", True, {"as": "Weapon_Trail_Melee_Whirlwind_FX", "shown": ("Trail_Dup",)}),
    ("hover_jump_A_FX", True),
    ("stabilizer_01_FX", True),
    ("Trails_Bumblebee_FX", True),
    ("HealAbility_Beam_FX", True),
    ("HealBeacon_FX", True),
    ("SelfHeal_FX", True),
    ("HealDoubleDmg_FX", True),
    ("Hover_Ability_FX", True),
    ("SentryLaser_FX", True),
    ("AssaultRifle_VEH_MuzzleFlash_FX", True),
    ("AssaultRifle_Tracer_FX", True),
    ("Impact_OS_Turret_A_FX", True),
    ("SentrySlow_Beam_FX", True),
    ("Squib_RepairEnemy_FX", True),
    ("Shield_Break_1A_FX", True),
    # Every Jazz's shield (TnShieldMaterialOverrider, shieldfx.rs): its sphere
    # filling as the shield comes back after a hit (_PowerUpParticleTemplate;
    # _FullPowerUpParticleTemplate from empty), breaking as it runs out
    # (_PowerDownParticleTemplate), and the ripple where a shot strikes it
    # (_HitParticleTemplate: Shell_STAT from all_shared.xxx).
    ("Shield_Restore_FX", True),
    ("Shield_FullRestore_FX", True),
    ("Shield_Break_FX", True),
    ("Squib_Shield_FX", True),
    # The enemies' guns (src/ai/mod.rs's GUN_FX): the soldiers' rifle's impact
    # (TnWeaponBurstRifleAI's BurstRifle_AI_WEPMESH: its muzzle flash and tracer are
    # the Neutron Assault Rifle's and the Battle Sentry's, above).
    ("AssaultRifleArm_Squib_FX", True),
    # The soldiers' grenades (TnProjectileGrenade's GrenadeSkill_PROJMESH): flying
    # (FlightEffect; ExplosionEffect is the Thermo Mines' Grenade_EXP_FX, above).
    ("Grenade_Trail_FX", True),
    # The enemies' other guns (src/ai/mod.rs's GUN_FX) and their rounds
    # (src/ai/projectile.rs: their projectile meshes' FlightEffect and
    # ExplosionEffect). The shotgunners' (Shotgun_AI_WEPMESH): its muzzle flash and
    # impact (its tracer is the Scatter Blaster's, above). The rocket troopers'
    # (RocketLauncher_AI_WEPMESH): its muzzle flash and its rockets' flight
    # (TnProjectileRocketAI; their blast is the trucks' rockets', above). The
    # sharpshooters' (SharpShot_WEPMESH): its muzzle flash, tracer and impact, and
    # the laser sight they aim with (Laser_Sight_FX). The Heavy Gunners' and
    # Titans' machine guns (TitanHeavyMG_WEPMESH): their muzzle flash and tracer. The
    # Guard's Guardian Blaster (GuardianBlaster_WEPMESH): the glow on its muzzle
    # while it charges (TnWeaponEventStartCharging to StopCharging), its muzzle
    # flash, and its rounds' flight and impact (GuardianBlaster_PROJMESH; its
    # barrage from its wings is the Thermo Rocket Cannon's rockets, above). The
    # jets' sticky bombs (TnProjectileFlyerSticky): flying, coming off what they
    # stuck to and going off.
    ("TurretArm_MuzzleFlash_FX", True),
    ("Squib_ScatterBlaster_FX", True),
    ("RocketLauncherAI_MuzzleFlash_FX", True),
    ("Tracer_AI_RocketLauncher_02_FX", True),
    ("MuzzleFlash_Sniper_AI_FX", True),
    # (Its Bolt, which the game doesn't draw, kept as what its lightning and smoke
    # trail follow; its bolts gone as their 0.1 s is up, as in the game's videos of
    # the Snipers' shots.)
    ("Tracer_Sniper_AI_FX", True, {"kill_on_completed": True}),
    ("Sniper_Squib_FX", True),
    ("Laser_Sight_FX", True),
    ("WEP_TitanMG_MuzzleFlash_FX", True),
    ("WEP_TitanMG_Tracer_FX", True),
    ("Guardian_Arm_Cannon_Charge_FX", True),
    ("GuardianBlaster_MuzzleFlash_Fx", True),
    ("Guardian_Arm_Cannon_Projectile_FX", True),
    ("Guardian_Arm_Cannon_Squib_FX", True),
    ("FlyersSticky_Trail_FX", True),
    ("FlyersSticky_Detach_FX", True),
    ("Exp_StickyGrenade_FX", True),
    # The jets' planes (their FlyerAut/Dec_VEHDEF: TnPlaneFormBlueprint) boosting:
    # their afterburners and contrail on their BoostSockets (BoostFx; their hover
    # jets, HoverFX, are the leaps' boosters below); and their robots' pulse attack's
    # tell on its socket (Ability_PulseAttack's notify).
    ("Afterburner_Sm_AI_Jet_FX", True),
    ("Trail_FlyersBoost_FX", True),
    ("Flyer_EMPAttack_Tell_FX", True),
    # The effects the enemies' animations play on their sockets (their
    # HmAnimNotify_PlayEffect notifies: tools/build_ai_anims.py's fx lines): the
    # dashes' pulse, the long falls' landings, the scrappers' and Leapers' leaps
    # (their boosters, trails and the ground they smash and the swell before it),
    # their punches, the Leaper's dive, lunge, footsteps, foot slides, smoke and
    # blast, the Titans' ground blow, weapon trail and the release of their drones,
    # the Guard's sword trails and its blast (EMP_Pulse_FX, its
    # TnAreaOfEffectAttackOperator's animation's).
    ("DashPulse_1_FX", True),
    ("Long_Fall_Landing_1_FX", True),
    ("Long_Fall_Landing_Sm_FX", True),
    ("HighLeap_Boosters_Thick_Short_FX", True),
    ("HighLeap_Boosters_Thin_Short_FX", True),
    # (The ground smash's flash on the ground, flat: its colour starts 50 times
    # over at 4, 4, 2.5 (ColorScaleOverLife), which the demo draws as a white sheet
    # across the floor; in the game's videos of the Hover Slam it is a glow under
    # the dust for two or three frames.)
    ("HighLeap_GroundSmash_FX", True, {"dim": {"flat": DIM_SMASH_FLASH}}),
    ("HighLeap_GroundSwell_FX", True),
    ("Leaper_Boosters_LowBurn_FX", True),
    ("Trails_Leap_FX", True),
    ("H2H_Punch01_FX", True),
    ("Leaper_Dive_FX", True),
    ("LungeTraill_FX", True),
    ("Footstep_Grimlock_FX", True),
    ("Sparks_FootSlide_FX", True),
    ("Sparks_FootSlide_Short_FX", True),
    ("SmokeTrail_FX", True),
    ("Aoe_LeaperBlast_FX", True),
    ("Melee_Ground_FX", True),
    ("Weapon_Trail_Melee_FX", True),
    ("Release_FX", True),
    ("Sword_Trail_Swing_FX", True),
    ("EMP_Pulse_FX", True),
    # The enemies' own: the AI's jumps (JumpParticleTemplate), the snipers' boosters
    # (their jets' Template), the Titans' drones in flight (EffectTemplate), and
    # what the enemies' hit effect players (their chassis' HitEffectPlayerBlueprint)
    # play on them as they are hurt (FXTemplate: sparks, blasts and arcs).
    ("AI_Jump_FX", True),
    ("Boosters_Med_FX", True),
    ("DroneFlight_FX", True),
    # The snipers' insects' feet and claws as they scuttle and change form (their
    # clips' notifies).
    ("Scuttle_Sparks_I_FX", True),
    # The Titan's drones': the explosive one's fuse (its suicide animation's
    # notify), the support one's beam to its Titan (TnBuffAuraSupportDroneDamage's
    # _Beam) and the shield it puts on him (TnBuffSupportDroneDamage's ApplyFx).
    ("TitanDrone_Attack_FX", True),
    ("ChainBeam_MainBeam_FX", True),
    ("ShieldHeal_FX", True),
    # Their deaths (TitanExplosiveDrone_DEATHSET's TnDeathActionPlayEffect): shot
    # down (Generic_DEATH), the sparks; blown up (SelfDestruct_DEATH), the blast.
    ("Electrical_Death_FX", True),
    ("Enforcer_Deathbeam_Squib_FX", True),
    ("Exp_TitanDrone_FX", True),
    ("Light_Dmg_Hit_FX", True),
    ("Explosion_Hit_FX", True),
    ("Sporatic_ElecBody_FX", True),
    ("BigDudes_Sling_HitSm_FX", True),
    # The deaths' (models/enemies/deaths.txt, tools/build_death_sets.py: every death
    # set's TnDeathActionPlayEffect systems, on the dying body's sockets and bones;
    # its destructibles' HmParticleDestructionEffectFactory systems; its gibs'
    # trails, HmGibsDestructionEffectFactory's TrailParticleTemplate).
    ("AI_Electrical_Death_FX", True),
    ("EXP_MED_01_FX", True),
    ("Egg_Explode_FX", True),
    ("Elec_Death_Flak_Trail_FX", True),
    ("Electrical_Death_Sniper_FX", True),
    ("Electrical_Dmg_FX", True),
    ("Electrocution_Transformer_FX", True),
    ("Electrocution_body_FX", True),
    ("Exp_Death_FX", True),
    ("Exp_Death_Lrg_FX", True),
    ("Exp_Death_Sm_FX", True),
    ("Exp_Death_XLrg_FX", True),
    ("Exp_Electrical_Death_FX", True),
    ("Exp_Fireball_FX", True),
    ("Exp_Flak_Lrg_FX", True),
    ("Exp_GenericLvl_LRG_FX", True),
    ("Fiery_Smokey_GibsTrail_FX", True),
    ("FireDeath_Linger_FX", True),
    ("Fire_Death_FX", True),
    ("Fire_Sm_FX", True),
    ("Fire_Swarmer_FX", True),
    ("Fire_Trail_Sm_FX", True),
    ("GibTrail_Electrical_FX", True),
    ("GibTrail_Swarmer_FX", True),
    ("Harvester_Death_FX", True),
    ("HeadOn_Lrg_Sparks_FX", True),
    ("HeadShot_Sparks_FX", True),
    ("Insct_Crush_Exp_FX", True),
    ("Nanomites_FX", True),
    ("Pieces_Electrical_Death_FX", True),
    ("PlasmaBubble_Death_FX", True),
    ("ScopedPistol_Death_FX", True),
    ("ShockBlast_Electrical_Death_FX", True),
    ("ShockwaveGun_Body_FX", True),
    ("ShockwaveGun_Electrical_Hit_FX", True),
    ("SideImpact_Lrg_Sparks_FX", True),
    ("SlimeCannon_Death_FX", True),
    ("SlimeCannon_Vaporized_Sm_FX", True),
    ("Splatter_Cut_INSCT_FX", True),
    ("Squib_Leaper_Metal_FX", True),
    ("TF_Death_GibsTrail_FX", True),
    ("Titan_Post-Death_FX", True),
    # The maps' traps (src/traps.rs; tools/build_doors.py's TRAP_SYSTEMS name them by
    # their Emitters' Templates): Ignition's flame jets (Grimlock's flamethrower,
    # FX_CHR_Grimlock_p's Flamethrower_FX) and Oblivion's combustion chamber's fires
    # (ESC_IncineratorFire_FX, and Exodus' flamethrower, another Flamethrower_FX:
    # written as Flamethrower_Exodus_FX). The fire on what the flames catch is
    # Flamethrower_Impact_FX (TnBuffOnFire's, above). The flame jets' videos show
    # Grimlock's flamethrower's FlameTip, its long white-hot tongue of fire shooting
    # from the nozzle into the billows (Fire_Anim_03_MAT, along its speed), and its
    # FlameBalls, the bright core inside them (Fire_Anim_05_MAT): sub-UV emitters
    # with a DynamicParameter module whose materials don't read them, which the
    # check would leave out (as it does the Scrapmaker's dark smoke). Without them
    # the jets' fire hangs 10 m off the nozzle, past a short blue flame. (Its heat
    # haze, Distort, stays out: the videos can't show whether it is there.)
    ("FX_CHR_Grimlock_p.FX.Flamethrower_FX", True,
     {"as": "Flamethrower_FX", "shown": ("FlameTip", "FlameBalls")}),
    ("FX_L01_Exodus_p.FX.Flamethrower_FX", True, {"as": "Flamethrower_Exodus_FX"}),
    ("ENV_MP_ProtoTypes_p.ESC_IncineratorFire_FX", True),
    # Ancients' Electric Fences: their posts' lightning (ElecFence_FX, two beam
    # emitters each reaching 25 m along the post's X, at the Emitters' 1.25).
    ("ENV_MP_ProtoTypes_p.FX.ElecFence_FX", True),
    # The gunship's guns (TnWeaponSentryInteractMG's MGInteract_Sentry_WEPMESH): the
    # muzzle flash, the tracer and the impact.
    ("AttackDrone_Sentry_MuzzleFlash_FX", True),
    ("AssaultRifle_VEH_Tracer_FX", True),
    ("AssaultRifle_VEH_Squib_FX", True),
    # Downfall's rocket sentries (TnWeaponSentryInteractRocket: its WEPMESH's muzzle
    # flash is the jets' MuzzleFlash_HomRocket_FX, above; its rockets'
    # RocketInteract_Sentry_PROJMESH: flying, the Swortex's Rocket_Trail_FX, written
    # as SentryRocket_Trail_FX, another system having its name - it draws the rocket
    # too, the projectile mesh's StaticMesh, PROJ_rocket_STAT - and going off,
    # EXP_Metroturret_FX).
    ("FX_Swortex_p.FX.Rocket_Trail_FX", True, {"as": "SentryRocket_Trail_FX"}),
    ("EXP_Metroturret_FX", True),
    # Both sentries' sights, which their Activate clips put on (and their
    # DeActivate clips take off): HmAnimNotify_PlayEffect's Sentry_Ring_FX on their
    # LaserSocket and Sentry_Laser_FX on their LaserSocket_2.
    ("WEP_SentryBlaster01_p.Sentry_Ring_FX", True),
    ("WEP_SentryBlaster01_p.Sentry_Laser_FX", True),
    # Oblivion's exploding barrel (ENV_IAC_ExplodingBarrel_p's
    # DES_AutobotExplodingBarrel_DSYS, src/barrels.rs): blowing up
    # (AutobotExplodingBarrel_State2's HmParticleDestructionEffectFactory: the
    # explosion, FX_Explosives_p's Exp_GenericLvl_MED_FX, and the fire left burning on
    # the wreck for the state's 10 s, FX_Level_Generic_p's Fire_ExpBarrel_FX), and the
    # trail of smoke and fire its big piece leaves (its Debris4_STAT's Particles,
    # FX_OS_Misc_p's Wounded_Soldier_FX; the cover pieces' Fiery_Smokey_GibsTrail_FX
    # is the deaths' already).
    ("Exp_GenericLvl_MED_FX", True),
    ("Fire_ExpBarrel_FX", True),
    ("Wounded_Soldier_FX", True),
    # Downfall's scaffolds coming down (ENV_MP_ProtoTypes_p's DES_ASH_ESCBeam1_DSYS,
    # src/doors.rs): its broken state's dust and falling bits (FX_Debris_p's
    # Catwalk_Debris_FX).
    ("Catwalk_Debris_FX", True),
    # The Neutron Assault Rifle's own muzzle flash (AssaultRifleArm_WEPMESH's
    # TnWeaponEffectPlayMuzzleFX: the vehicle rifle's package's, by its full name, as
    # the Fireblast's above is another of the same name): its flame cap, black smoke
    # cap, spark bolts, haze and glow, the game's own in place of the demo's sprites
    # (src/fx.rs's RIFLE_MUZZLE, now its light alone). Its dark smoke (Smoke_Dark)
    # the game never draws.
    ("FX_AssaultRifleVH_p.AssaultRifleArm.AssaultRifleArm_MuzzleFlash_FX", True,
     {"as": "NeutronRifle_MuzzleFlash_FX"}),
    # The Subsonic Repeater's muzzle flash (IonBlaster_WEPMESH's
    # TnWeaponEffectPlayMuzzleFX, and again on its MuzzleFlash2 socket with every
    # shot: TnWeaponEffectPlayFXOnSocket): its squiggly caps on the cap mesh, the Ion
    # core mesh bursting out along the barrel, the starburst and the glows, the
    # game's own in place of the demo's sprites (src/fx.rs's SUBSONIC_MUZZLE, now its
    # light alone).
    ("FX_OS_Weapons_p.Subsonic_Repeater_FX.Subsonic_Repeater_MuzzleFlash_FX", True),
    # The Photon Burst Rifle's, the Scatter Blaster's, the Nucleon Charge Rifle's and
    # the Path Blaster's muzzle flashes (BurstRifle_WEPMESH's, Shotgun_WEPMESH's,
    # SniperRifle_WEPMESH's and HeavyPistol_WEPMESH's TnWeaponEffectPlayMuzzleFX,
    # their first fire mode's), the game's own in place of the demo's sprites
    # (src/fx.rs's PHOTON_MUZZLE, SCATTER_MUZZLE, NUCLEON_MUZZLE and PATH_MUZZLE, now
    # their lights alone).
    ("FX_OS_Weapons_p.Photon_BurstRifle_FX.Photon_BurstRifle_MuzzleFlash_Fx", True),
    ("FX_OS_Weapons_p.ScatterBlaster_FX.Scatterblaster_MuzzleFlash_FX", True),
    ("FX_OS_Weapons_p.NucleonRifle_FX.NucleonRifle_MuzzleFlash_FX", True),
    ("FX_OS_Weapons_p.PathBlaster_FX.PathBlaster_MuzzleFlash_FX", True),
]


# The meshes drawn as systems of their own, each one still mesh particle of a
# StaticMesh in its material, at a size (its particle's), for as long as its system
# plays (its seconds, unless it is killed first), in the system's own space (it
# moves and turns with where the game puts it), lit or not (as the material is; a
# lit one's material, solid as it is, is marked lit too, levelfx.py's lit_solid, and
# lit by the light where it starts wherever the demo lights it: FxSystems::set_lit,
# as src/gibs.rs does a destructible's chunks):
#
# * The Gear Shredder's blade once it has stopped, stuck where it struck: the
#   projectile's StaticMesh (SawBlade_PROJMESH's PROJ_SawBladeProjectile_STAT, in
#   its SawBlade_MAT) on its MeshComp, which TnProjectileSawblade.Explode shows there
#   for the 10 s it is left (DestroyProjectile: its LifeSpan), as the projectile
#   turned (its Rotation, the way it was fired: bRotationFollowsVelocity is off), at
#   the size and in the colour the mesh has (the material's VertexColor is the mesh's
#   own, white), lit as the projectile's light environment lights it.
# * The Chaos-Rift Combustor's bomb and its clusters as they fly: their projectile
#   meshes' StaticMesh (ClusterCannon_PROJMESH's PROJMESH_GrenadeBall_STAT in
#   PROP_NEU_MineBall_MATINST, Clusters_PROJMESH's PROJMESH_Grenade_STAT in
#   WEP_ClusterCannon_cans_MATINST), on the round (src/riot.rs draws the clusters'
#   at their DrawScale, 1.5), lit.
# * The Combustor's impact marker (TnWeaponTankCannonArcable's _ImpactMarker:
#   Sphere_03_MESH in MarkTarget_Depth_MAT, its Scale3D 10, where its lob would
#   land), unlit: the material brightens what the sphere cuts through (its
#   DepthBiasedAlpha), the ring on the ground where the bomb will land.
# * The Diffraction Barrier's shield over its pickup at an equipment station
#   (src/shop.rs): MP_Pickup_Barrier_STAT's second section, in its own material
#   (MP_Barrier_Distortion_MATINST, of TR_Barrier_Distortion_AA_MAT: see-through,
#   its hexagons lit at the rim and a band of them sweeping up it), turning with the
#   pickup, unlit. Its mesh is that section alone: MP_Pickup_Barrier_Shield, cut from
#   umodel's export of the pickup (ESC_Ark_BASE_m.xxx MP_Pickup_Barrier_STAT:
#   MESH_CUTS).
STUCK_BLADE = "SawBlade_Stuck_FX"
MESH_SYSTEMS = [
    # (system, mesh, material, size, seconds, lit)
    (STUCK_BLADE, "PROJ_SawBladeProjectile_STAT", "SawBlade_MAT", 1.0, 10.0, True),
    ("ClusterCannon_Bomb_FX", "PROJMESH_GrenadeBall_STAT", "PROP_NEU_MineBall_MATINST", 1.0, 60.0, True),
    ("ClusterCannon_Cluster_FX", "PROJMESH_Grenade_STAT", "WEP_ClusterCannon_cans_MATINST", 1.0, 60.0, True),
    ("ClusterCannon_Marker_FX", "Sphere_03_MESH", "MarkTarget_Depth_MAT", 10.0, 60.0, False),
    ("Barrier_Pickup_Shield_FX", "MP_Pickup_Barrier_Shield", "MP_Barrier_Distortion_MATINST", 1.0, 1.0e6, False),
    # The soldiers' grenade as it flies and rests (src/ai/grenade.rs): its projectile
    # mesh's StaticMesh (GrenadeSkill_PROJMESH's WEP_Grenade_Flak_STAT, all_shared.xxx),
    # lit, for as long as it can be out.
    ("AI_Grenade_FX", "WEP_Grenade_Flak_STAT", "WEP_Grenade_MATINST", 1.0, 60.0, True),
    # The Diffraction Barrier standing (src/equipment.rs): WEP_Barrier_AB_SKEL's three
    # sections, each cut from umodel's export of it (all_shared.xxx: MESH_CUTS) as a
    # still mesh of its own, in its own material: the hexagons, the
    # distortion with its sweeping band and the edge's glow (the last two in each
    # side's colour: levelfx.py's VARIANTS).
    ("Barrier_Wall_Hex_FX", "WEP_Barrier_Hexagons", "TR_Barrier_Hexigons_MAT_INST", 1.0, 1.0e6, False),
    ("Barrier_Wall_Distortion_Decepticon_FX", "WEP_Barrier_Distortion", "TR_Barrier_Distortion_AA_MATINST@Decepticon",
     1.0, 1.0e6, False),
    ("Barrier_Wall_Distortion_Autobot_FX", "WEP_Barrier_Distortion", "TR_Barrier_Distortion_AA_MATINST@Autobot",
     1.0, 1.0e6, False),
    ("Barrier_Wall_Edge_Decepticon_FX", "WEP_Barrier_EdgeGlow", "TR_Barrier_EdgeGlow_MAT_INST@Decepticon", 1.0, 1.0e6, False),
    ("Barrier_Wall_Edge_Autobot_FX", "WEP_Barrier_EdgeGlow", "TR_Barrier_EdgeGlow_MAT_INST@Autobot", 1.0, 1.0e6, False),
] + [
    # The deaths' gibs (their destructibles' HmGibsDestructionEffectFactory and
    # HmDebrisDestructionEffectFactory chunks: HmDestructibleSpawnedChunk's
    # StaticMesh in its own material, unlit: TR_Death_Gibs_MAT's emissive, the
    # pipe's its own), each a system "<mesh>_FX" that src/gibs.rs flies, for as long
    # as it lives (it ends them).
    ("%s_FX" % m, m, mat, 1.0, 1.0e6, False) for m, mat in
    [("RB_Gib_%02d_STAT" % i, "DeathGib_Mat_INST") for i in range(1, 12)]
    + [("RB_GibTire_01_STAT", "DeathSkelGib_Mat_INST"),
       ("DES_RST_PipeMediumGibs01_STAT", "ENV_BIGCABLERUST_matinst")]
] + [
    # The Swarmer's (Swarmer_Death_MeleeCut_01_DSYS's egg pods, Swarmer_Death_Thrown_DSYS's
    # claw and leg pieces: ESC_Temple_BASE_m's), in their meshes' own materials, lit
    # (the claw in two: its thorax's and its legs', a list of each section's).
    ("%s_FX" % m, m, mat, 1.0, 1.0e6, True) for m, mat in
    [("Pod%d_STAT" % i, "InsecticonPod01_MAT_INST") for i in (11, 12, 14)]
    + [("KickbackMutilationJib%d_STAT" % i, "VH_Swarmer_Legs_Mat") for i in (1, 2, 5)]
    + [("KickbackMutilationJib3_STAT", "VH_Swarmer_Thorax_Mat"),
       ("VH_SwarmerClawJib_STAT", ["VH_Swarmer_Thorax_Mat", "VH_Swarmer_Legs_Mat"])]
] + [
    # Oblivion's exploding barrel's debris (DES_AutobotExplodingBarrel_DSYS's
    # HmDebrisDestructionEffectFactory chunks, which src/barrels.rs throws as
    # src/gibs.rs throws the deaths'): its own pieces in its glowless material
    # (AutobotExplodingBarrel_noGlow_MATINST, the wreck's) and DES_ASH_DestructCoverBlocks_p's
    # cover block piece in its own, lit, at their own size (the chunks', DrawScale 1;
    # ESC_Ark_ART_m's).
    ("%s_FX" % m, m, mat, 1.0, 1.0e6, True) for m, mat in
    [("Debris%d_STAT" % i, "AutobotExplodingBarrel_noGlow_MATINST") for i in (1, 2, 3, 4)]
    + [("CoverBLockCLarge_debris_02_STAT", "CoverBlocksTypeA_MATINST")]
] + [
    # Downfall's scaffolds' pieces (DES_ASH_ESCBeam1_DSYS's broken state's
    # HmDebrisDestructionEffectFactory chunks, which src/doors.rs throws as
    # src/gibs.rs throws the deaths'): the beam's four parts, in the beam's own
    # material, lit, at their own size (ESC_Iacon_BASE_m's).
    ("%s_FX" % m, m, "ENV_beam1_matinst", 1.0, 1.0e6, True) for m in
    ["Beam1_pt%d_STAT" % i for i in (1, 2, 3, 4)]
]


# The deaths' decals (their destructibles' HmDecalDestructionEffectFactory), each a
# system "<material>_DECAL_FX" of one decal particle laid straight down on what is
# under it, 100 units across at a scale of 1 (src/deaths.rs's destructibles scale it
# to their DecalWidth and its range), for its seconds (DecalLifetime). (Not
# DeathDecal_MAT's: UNSEEN_DECALS.)
DECAL_SYSTEMS = [
    # (material, seconds)
    ("DECAL_SplatterEnergon01_MATINST", 20.0),
]

# The scorch marks the game's videos never show: DeathDecal_MAT, the blasts' black
# burn with glowing specks (blastmark_glow_CLR: its green the burn, its red the
# specks), 8 to 40 m across in the effects' decal emitters (the tank shell's 12 to
# 13.5 m, the rockets' 8 to 18 m, the sentries' 30 to 40 m) and 5 to 15 m in the
# deaths' destructibles. Drawn as the data has them, each left the floor round a
# blast near-black for up to half a minute, and a few close shots blacked out most
# of the view; the game's own videos of a tank shell striking the floor ahead, of
# Downfall's sentries' rockets and of the soldiers they blow up show the floor
# unmarked after. Their decal emitters are left out (LevelFx.unseen_decals), and
# the deaths lay none of them (DECAL_SYSTEMS).
UNSEEN_DECALS = ("DeathDecal_MAT",)


# Materials the demo draws meshes of its own with (FxSystems::mesh_material): the
# Guard's scanner beam (TnScannerBeamComponent, a ProjectedMeshComponent: its arc
# in Scanner_Beam_MAT, src/ai/scanner.rs builds it).
EXTRA_MATERIALS = ["Scanner_Beam_MAT"]


def decal_system(fx, material, secs):
    """Adds a system of one decal particle (DECAL_SYSTEMS) to `fx`'s systems; False
    if its material can't be had."""
    em = particles.Emitter()
    em.name = "Decal"
    em.material_name = material
    em.mat = fx.material(material)
    if em.mat is None:
        return False
    em.typedata = {"kind": "decal", "dir": particles.const_vector((0.0, 0.0, -1.0))}
    em.req = {"align": 0, "local": 0, "duration": secs, "durlow": 0.0, "durrange": 0, "durrecalc": 0,
              "loops": 1, "delay": 0.0, "delayfirst": 0, "subh": 1, "subv": 1, "interp": 0, "randtime": 1.0,
              "sort": 0, "camoffset": 0.0, "maxdraw": 500, "probability": 1.0, "scaleuv": 0, "lock": 0,
              "killdeact": 0}
    em.rate = particles.const_float(0.0)
    em.bursts = [(1, -1, 0.0)]
    em.peak = 1
    em.modules = [
        particles.Module(0x01, [particles.const_float(secs)], [], []),
        particles.Module(0x06, [], [particles.const_vector((100.0, 100.0, 100.0))], []),
        particles.Module(0x02, [particles.const_float(1.0)], [particles.const_vector((1.0, 1.0, 1.0))], []),
    ]
    em.spawn = [0, 1, 2]
    em.update = []
    name = "%s_DECAL_FX" % material
    s = particles.System(name)
    s.lit = False
    s.emitters = [em]
    s.lods = []
    fx.sys_index[name] = len(fx.systems)
    fx.systems.append(s)
    return True


def mesh_system(fx, name, mesh, material, size, secs, lit):
    """Adds a system of one still mesh particle (MESH_SYSTEMS) to `fx`'s systems;
    False if its mesh or material can't be had. `material` a list: one particle for
    each of the mesh's sections, each in its own (levelfx.py's "<mesh>#<k>")."""
    if isinstance(material, list):
        ems = [mesh_emitter(fx, "%s#%d" % (mesh, k), m, size, secs) for k, m in enumerate(material)]
    else:
        ems = [mesh_emitter(fx, mesh, material, size, secs)]
    if None in ems:
        return False
    s = particles.System(name)
    s.lit = lit
    s.emitters = ems
    if lit:
        # (Its material lit as well, solid as it is: levelfx.py's lit_solid.)
        fx.lit_solid.update(em.mat for em in ems)
    s.lods = []
    fx.sys_index[name] = len(fx.systems)
    fx.systems.append(s)
    return True


def mesh_emitter(fx, mesh, material, size, secs):
    """A mesh system's emitter: one still particle of `mesh` in `material`; None if
    either can't be had."""
    em = particles.Emitter()
    em.name = "Mesh"
    em.material_name = material
    em.mat = fx.material(material)
    em.typedata = {"kind": "mesh", "mesh_name": mesh, "camera_facing": 0}
    if em.mat is None or not fx._mesh(em):
        return None
    em.req = {"align": 0, "local": 1, "duration": secs, "durlow": 0.0, "durrange": 0, "durrecalc": 0,
              "loops": 1, "delay": 0.0, "delayfirst": 0, "subh": 1, "subv": 1, "interp": 0, "randtime": 1.0,
              "sort": 0, "camoffset": 0.0, "maxdraw": 500, "probability": 1.0, "scaleuv": 0, "lock": 0,
              "killdeact": 1}
    em.rate = particles.const_float(0.0)
    em.bursts = [(1, -1, 0.0)]
    em.peak = 1
    em.modules = [
        particles.Module(0x01, [particles.const_float(secs)], [], []),
        particles.Module(0x06, [], [particles.const_vector((size, size, size))], []),
        particles.Module(0x02, [particles.const_float(1.0)], [particles.const_vector((1.0, 1.0, 1.0))], []),
    ]
    em.spawn = [0, 1, 2]
    em.update = []
    return em


# The meshes cut from others: the Diffraction Barrier pickup's shield (its second
# section) and the standing barrier's three sections, by their materials.
MESH_CUTS = {
    "MP_Pickup_Barrier_Shield": ("MP_Pickup_Barrier_STAT", "MP_Barrier_Distortion_MATINST"),
    "WEP_Barrier_Hexagons": ("WEP_Barrier_AB_SKEL", "TR_Barrier_Hexigons_MAT_INST"),
    "WEP_Barrier_Distortion": ("WEP_Barrier_AB_SKEL", "TR_Barrier_Distortion_AA_MATINST"),
    "WEP_Barrier_EdgeGlow": ("WEP_Barrier_AB_SKEL", "TR_Barrier_EdgeGlow_MAT_INST"),
}


def main(cooked, export, assets):
    fx = effects(cooked, export, os.path.join(assets, "models", "fx"))
    fx.write(assets)


def effects(cooked, export, built):
    """The effects read (`built`: the models/fx folder of meshes an earlier build
    wrote, kept for those the export hasn't got)."""
    fx = levelfx.LevelFx("weapons", cooked, PACKAGES, mesh_export=export, txt_path="fx/weapons_fx.txt", lods=True,
                         mesh_built=built)
    fx.mesh_cuts = dict(MESH_CUTS)
    fx.unseen_decals = set(UNSEEN_DECALS)
    for name, game_shaders, *rest in SYSTEMS:
        opts = rest[0] if rest else {}
        idx = fx.system_named(name, game_shaders=game_shaders, rename=opts.get("as"), shown=opts.get("shown", ()))
        if idx is None:
            print(f"  ! {name} can't be drawn")
        elif opts.get("leave_out") or opts.get("kill_on_completed") or opts.get("dim"):
            # (Its options, at every level of detail: emitters left out by name, and
            # whether the emitters the game kills on completing are killed so.)
            s = fx.systems[idx]
            for k in [idx] + [lod for _dist, lod in s.lods]:
                system = fx.systems[k]
                system.emitters = [em for em in system.emitters if em.name not in opts.get("leave_out", ())]
                if opts.get("kill_on_completed"):
                    for em in system.emitters:
                        if getattr(em, "kill_on_completed", False):
                            em.req["killdone"] = 1
                for em in system.emitters:
                    if em.name in opts.get("dim", {}):
                        dim_colour(em, opts["dim"][em.name])
    for name, mesh, material, size, secs, lit in MESH_SYSTEMS:
        if not mesh_system(fx, name, mesh, material, size, secs, lit):
            print(f"  ! {name} can't be drawn")
    for material, secs in DECAL_SYSTEMS:
        if not decal_system(fx, material, secs):
            print(f"  ! decal {material} can't be drawn")
    for name in EXTRA_MATERIALS:
        if fx.material(name) is None:
            print(f"  ! material {name} can't be drawn")
    return fx


def dim_colour(em, factor):
    """Emitter `em`'s starting colour (its Color modules' StartColor) taken by
    `factor`: a constant or a range throughout, a curve's range and its entries."""
    for m in em.modules:
        if m.kind != 0x02:
            continue
        for d in m.vectors:
            if d.kind in (7, 9):
                d.table = [v * factor for v in d.table]
            elif d.kind in (8, 10):
                d.table = [v * factor for v in d.table[:2]] + d.table[2:4] + [v * factor for v in d.table[4:]]


def umodel_commands(cooked, export):
    """The umodel commands that export the meshes the effects' mesh emitters draw
    (each from the first package in PACKAGES' order, all_shared.xxx and mp_shared.xxx
    first, that has it; a cut mesh's whole mesh)."""
    import contextlib
    import io
    from ue3pkg import Package
    wanted = []

    def record(self, em):
        name = em.typedata["mesh_name"]
        if name not in wanted:
            wanted.append(name)
        em.typedata["file"] = f"models/fx/{name}.fxmesh"
        return True
    levelfx.LevelFx._mesh = record
    with contextlib.redirect_stdout(io.StringIO()):
        effects(cooked, None, None)
    meshes = sorted({MESH_CUTS[n][0] if n in MESH_CUTS else n.split("#")[0] for n in wanted})
    order = ["all_shared.xxx", "mp_shared.xxx"] + [p for p in PACKAGES if p not in ("all_shared.xxx", "mp_shared.xxx")]
    by_package, left = {}, set(meshes)
    for package in order:
        pkg = Package(os.path.join(cooked, package))
        have = {e["name"] for e in pkg.exports if pkg.class_name(e) in ("StaticMesh", "SkeletalMesh")}
        for m in sorted(left & have):
            by_package.setdefault(package, []).append(m)
        left -= have

    def q(p):
        return f'"{p}"' if " " in p else p
    out = [f"umodel -game=trans -export -gltf -path={q(cooked)} -out={q(export)} {package} "
           + " ".join(f"-obj={m}" for m in ms) for package, ms in by_package.items()]
    out += [f"# not in the packages: {m}" for m in sorted(left)]
    return out


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--umodel":
        print("\n".join(umodel_commands(sys.argv[2], sys.argv[3])))
    elif len(sys.argv) == 4:
        main(*sys.argv[1:])
    else:
        sys.exit(__doc__)
