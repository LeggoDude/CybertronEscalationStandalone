//! Escalation's support items in use: the equipment the stations sell (shop.rs) as
//! the game's abilities run them (TransGame.xxx, TransContent in the Escalation map
//! packages, and their config; tools/equipment notes in the README).
//!
//! * **The slots** (TnAbilityManager): what is bought goes in its slot, offensive
//!   (Assault T.E.C.H., 1) or defensive (Utility T.E.C.H., 2), ready. Its key uses
//!   it, in robot form only (the commands' RelevantModes), and not again for a
//!   second (SpamPreventionTime); each item has one use (ResourceMax 1, spent by
//!   the use), shown red in its slot while its effect lasts, after which the slot
//!   empties (ShouldBeDiscarded). The Diffraction Barrier and the Dimensional
//!   Decimator can be cancelled: the key again lets the barrier down, sets the ball
//!   off. Buying into a slot ends what its item has going (ReplaceAbility), and so
//!   does buying the same item (ForceEndAbilityByClass), but a buff runs on
//!   (TnAbilityAddBuff can't be force-ended). Dying loses what is unused and ends
//!   it all.
//! * **Thermo Mine Payload** (TnKillstreakMinePooper, TnBuffMinePooper): three mines
//!   let out half a second apart round him, each falling to the floor, rising off
//!   it and bobbing (KamikazeMineKillstreak_PROJDATA's hover), then, once one sees
//!   an enemy within 65 m, homing on it at 20 m/s and going off as it reaches it
//!   or a wall: 250 to every enemy within 8.5 m. "Thermo Mine Deployed" shows in
//!   the game's messages.
//! * **Diffraction Barrier** (TnAbilityBarrier, TnBarrierSpawnable): a curved wall
//!   10 m ahead (BarrierOffset), 15.7 m wide and 8.4 m high, growing in over 0.2 s
//!   and wearing away at 25 a second from its 1000 (BarrierBP[1]): 40 s.
//! * **Energon Recharger** (TnKillStreakHealthRegen, TnBuffHealthRegenKillStreak):
//!   15 s of healing, his health and then his shield, at 0.4 of his form's shield
//!   recharge and without its wait (main.rs's player_health).
//! * **Intercooler** (TnKillstreakFastAbilityCooldown, TnBuffIntercooler): 12 s for
//!   him and every teammate within 25 m of him: 1.2 times the robot's speed and the
//!   class ability's meter refilling 8 times as fast (emp.rs), its lightning crackling
//!   over each of them (TnBuffFastAbilityCooldown's ApplyFx).
//! * **Dimensional Decimator** (TnAbilityRollerSphereGravityAura,
//!   TnRollerMineGravityAura, TnGravityAura): a ball rolled out ahead, set off 2.5 s
//!   later (or as it strikes an enemy once armed, or as he cancels it) into a black
//!   hole that kills every lesser enemy within 50 m, draws their bodies in and
//!   flings them out as it collapses 5.5 s on; the big ones (Leaper, Enforcer,
//!   Bruiser, Titan) it leaves be.
//!
//! What he uses goes to the other players' PCs (net.rs's Gear, Mine and GearEnd),
//! whose copies do the same there; the Intercooler a teammate there uses near this
//! PC's player gives him its boost too.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use crate::abilities::Sockets;
use crate::enemies::{self, Death, Hit};
use crate::hud::HudEvent;
use crate::levelfx::{FxId, FxSystems};
use crate::net::JazzEvent;
use crate::ragdoll::{Rig, RigState};
use crate::riot::ScreenShake;
use crate::shop::Equipment;
use crate::sound::Sfx;
use crate::{
    Colliders, Enemy, Form, GltfClips, Level, LocalPlayer, Player, PlayerInput, RemotePlayer,
};

// ---------------------------------------------------------------------------
// The game's numbers
// ---------------------------------------------------------------------------

/// TnAbilityManager's SpamPreventionTime: a slot's key does nothing for this long
/// after a use.
const SPAM_SECS: f32 = 1.0;

/// TnBuffMinePooper ([TransContent.TnBuffMinePooper]): it lasts BuffTime, letting a
/// mine out every PoopRate (three in all), each PoopOffset from him (forward,
/// right: 7 m ahead and 5.75 m to his left), turned on by SpawnRotationOffsetDeg
/// more each time; a box this big all round (the trace's extent) pulls one in
/// against a wall in its way.
const MINE_BUFF_SECS: f32 = 1.6;
const MINE_RATE: f32 = 0.5;
const MINE_OFFSET: Vec2 = Vec2::new(7.0, -5.75);
const MINE_TURN: f32 = 50.0;
const MINE_TRACE: f32 = 1.0;
/// KamikazeMineKillstreak_PROJDATA: GravityScale times the world's gravity while it
/// falls; how far under it the floor counts as reached (the 70 UU trace); its hover
/// (HoverSpeed, HoverDistance's Min and Max, HoverTime before it searches) and
/// spin (HoverRotationRate, pitch: radians a second); its search (SearchRadius)
/// and homing (HomingVelocity); its blast (Damage within DamageRadius, as much at
/// its edge as at its heart and through walls: IgnoreHurtRadiusTrace) and its
/// effect's size (ExplosionEffectScale); its life (LifeSpan) and the fade it goes
/// in at the end of it or as its owner dies (FadeOutTime).
const MINE_GRAVITY: f32 = -29.4 * 3.0;
const MINE_LAND: f32 = 0.7;
const MINE_HOVER_SPEED: f32 = 0.75;
const MINE_HOVER_MIN: f32 = 0.5;
const MINE_HOVER_MAX: f32 = 3.15;
const MINE_HOVER_SECS: f32 = 0.4;
const MINE_SPIN: f32 = FRAC_PI_2;
const MINE_SEARCH: f32 = 65.0;
const MINE_HOMING_SPEED: f32 = 20.0;
const MINE_DAMAGE: f32 = 250.0;
const MINE_RADIUS: f32 = 8.5;
const MINE_EXPLOSION_SCALE: f32 = 0.5;
const MINE_LIFE_SECS: f32 = 999.0;
const MINE_FADE_SECS: f32 = 1.0;
/// Its homing clip's speed (HomingIdleAnim's RateScale) and the fade between its
/// flight loops as it finds a target.
const MINE_ACTIVATE_RATE: f32 = 0.75;
const MINE_LOOP_FADE: f32 = 0.25;
/// A mine this near an enemy's body (or closer) has reached it (the projectile is a
/// point: its cylinder has no size).
const MINE_TOUCH: f32 = 0.2;
/// TnCameraShakeProjectileExtension_16326: the blast shakes the view of a player
/// within 300 m (beyond its damage radius), its ScreenShakeStruct.
const MINE_SHAKE: ScreenShake = ScreenShake {
    radius: 300.0,
    secs: 1.0,
    rot_amp: Vec3::new(50.0, 25.0, 15.0),
    rot_freq: Vec3::new(35.0, 25.0, 10.0),
    loc_amp: Vec3::new(10.0, 0.0, 5.0),
    loc_freq: Vec3::new(15.0, 0.0, 10.0),
    fov_amp: 0.0,
    fov_freq: 0.0,
};

/// TnAbilityBarrier: BarrierOffset (forward and up from his middle: 10 m ahead, 2 m
/// down) and BarrierBP[1]'s BarrierHealth and DegenRate; its Barrier_Equip clip's
/// grow (WEP_Body_XB scaled from 0.001 to 1 over 0.2 s); and where its loop is
/// heard from (its middle, over its foot).
const BARRIER_OFFSET: Vec2 = Vec2::new(10.0, -2.0);
const BARRIER_HEALTH: f32 = 1000.0;
const BARRIER_DEGEN: f32 = 25.0;
const BARRIER_GROW_SECS: f32 = 0.2;
const BARRIER_MIN_SCALE: f32 = 0.001;
const BARRIER_MIDDLE: f32 = 4.0;
/// The barrier's collision (TnBarrierSpawnable's physics asset,
/// WEP_Shield_p.Barrier.WEP_Barrier_AB_PHYS: ten boxes on WEP_Body_XB, the bone
/// its growth scales): each box's middle and axes in the barrier's frame (Unreal's
/// units and axes, as its effects' meshes have it: X ahead, Y to its right, Z up)
/// and its size along them. The barrier is world geometry (bWorldGeometry) to the
/// enemies' rounds, taking their damage (BarrierBP[1].TakesDamage), but seen
/// through (ForcePawnSeeThrough); its own side's rounds pass (TnBarrierBase's
/// ShouldTrace: only its enemies' traces and projectiles stop at a barrier).
#[rustfmt::skip]
const BARRIER_BOXES: [([f32; 3], [[f32; 3]; 3], [f32; 3]); 10] = [
    (
        [155.221, 0.0, 243.038],
        [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]],
        [110.307, 524.8, 524.8],
    ),
    (
        [25.1067, -444.043, 243.038],
        [[0.8426, -0.5386, 0.0], [-0.5386, -0.8426, 0.0], [0.0, 0.0, 1.0]],
        [110.307, 524.8, 524.8],
    ),
    (
        [-247.763, -715.309, 241.62],
        [[0.421, -0.907, -0.0057], [-0.907, -0.4209, -0.0145], [-0.0107, -0.0113, 0.9999]],
        [110.307, 354.666, 524.8],
    ),
    (
        [25.76, 457.325, 243.038],
        [[0.8517, 0.524, 0.0], [0.524, -0.8517, 0.0], [0.0, 0.0, 1.0]],
        [110.307, 524.8, 524.8],
    ),
    (
        [-246.109, 700.063, 243.038],
        [[0.2486, 0.9686, 0.0], [0.9686, -0.2486, 0.0], [0.0, 0.0, 1.0]],
        [110.307, 346.405, 524.8],
    ),
    (
        [-271.195, 602.332, 642.659],
        [[0.2034, 0.7926, 0.5749], [0.9686, -0.2486, 0.0], [-0.1429, -0.5568, 0.8182]],
        [90.9149, 346.405, 416.557],
    ),
    (
        [-55.5862, 400.871, 645.532],
        [[0.7119, 0.4337, 0.5524], [0.5344, -0.8448, -0.0254], [-0.4557, -0.3132, 0.8332]],
        [90.9149, 523.49, 416.557],
    ),
    (
        [62.7878, -6.3486, 655.734],
        [[0.8496, 0.0334, 0.5264], [0.0341, -0.9994, 0.0083], [-0.5264, -0.0109, 0.8502]],
        [90.9149, 523.49, 416.557],
    ),
    (
        [-53.8936, -402.099, 654.878],
        [[0.7149, -0.4592, 0.5274], [-0.5405, -0.8414, 0.0], [-0.4437, 0.285, 0.8496]],
        [90.9149, 523.49, 416.557],
    ),
    (
        [-296.667, -641.42, 658.161],
        [[0.3484, -0.7819, 0.517], [-0.9007, -0.432, -0.0464], [-0.2596, 0.4495, 0.8547]],
        [90.9149, 335.37, 416.557],
    ),
];

/// TnBuffHealthRegenKillStreak: BuffTime, and FloatModifierSp, its share of his
/// form's shield recharge (main.rs's player_health heals by it).
const RECHARGE_SECS: f32 = 15.0;
pub(crate) const RECHARGE_SCALE: f32 = 0.4;

/// TnBuffIntercooler: BuffTime; TnKillstreakFastAbilityCooldown's BuffRange; and its
/// two buffs' SpeedMultiplier (TnBuffIntercoolerGroundSpeed: main.rs's movement)
/// and CooldownMultiplier (TnBuffFastAbilityCooldown: the class ability's refill,
/// emp.rs).
const INTERCOOLER_SECS: f32 = 12.0;
const INTERCOOLER_RANGE: f32 = 25.0;
pub(crate) const INTERCOOLER_SPEED: f32 = 1.2;
pub(crate) const INTERCOOLER_COOLDOWN: f32 = 8.0;

/// TnAbilityRollerSphereGravityAura: the ball comes out SpawnDelay after the key
/// (trying again every second while its place isn't clear), SpawnOffset from his
/// middle (5 m ahead, 1 m up), at InitialVelocity (25 m/s ahead, 5 up).
const BALL_DELAY: f32 = 0.2;
const BALL_RETRY: f32 = 1.0;
const BALL_SPAWN: Vec2 = Vec2::new(5.0, 1.0);
const BALL_SPEED: Vec2 = Vec2::new(25.0, 5.0);
/// TnRollerMineGravityAura: its physics sphere's radius (131.692 UU), the room its
/// spawn needs (that, times 1.1), ArmTime, its fuse (2.5 s, its FUSE sound with 1.5 s
/// left) and its _Health (damage past it sets it off).
const BALL_RADIUS: f32 = 1.32;
const BALL_CLEARANCE: f32 = 1.45;
const BALL_ARM_SECS: f32 = 1.0;
const BALL_FUSE_SECS: f32 = 2.5;
const BALL_FUSE_SOUND_SECS: f32 = 1.0;
/// Its rigid body under the world's gravity (DefaultGravityZ), and what a bounce
/// keeps of its speed into the surface and along it (its physical material's
/// restitution and friction, unresolved: a heavy ball's).
const BALL_GRAVITY: f32 = -29.4;
const BALL_BOUNCE: f32 = 0.3;
const BALL_GRIP: f32 = 0.85;
/// A rolling ball loses this share of its speed a second (rolling resistance).
const BALL_ROLL_DRAG: f32 = 0.15;

/// TnGravityAura (GravityGrenade_PROJDATA's GravityAura): its LifeSpan, RiseSpeed
/// (staying 5 m under a ceiling), its area (AoeBlueprint Radius: a cylinder that
/// wide and as tall either way), its pull on the bodies of the enemies it kills
/// (PullInAcceleration, times OrbitScale beyond OrbitDistance while moving away;
/// PullUpAcceleration; FinalSquishForce in its last FinalSquishDuration), and its
/// end: the bodies within GibDistance flung out (ExplosionImpulse, as a speed), and
/// itself gone a second on.
const HOLE_SECS: f32 = 5.5;
const HOLE_RISE: f32 = 3.5;
const HOLE_CEILING: f32 = 5.0;
const HOLE_RADIUS: f32 = 50.0;
const HOLE_PULL_IN: f32 = 500.0;
const HOLE_ORBIT: f32 = 1.5;
const HOLE_ORBIT_SCALE: f32 = 5.0;
const HOLE_PULL_UP: f32 = 1000.0;
const HOLE_SQUISH_SECS: f32 = 2.0;
const HOLE_SQUISH: f32 = 350.0;
const HOLE_GIB_DISTANCE: f32 = 25.0;
const HOLE_FLING: f32 = 32.0;
const HOLE_GONE_SECS: f32 = 1.0;
/// The enemies it doesn't kill (TnGravityAura.ShouldKill: AICT_Leaper, Enforcer,
/// Bruiser and Titan), which it only stuns.
const HOLE_SPARED: [&str; 4] = ["AICT_Leaper", "AICT_Enforcer", "AICT_Bruiser", "AICT_Titan"];
/// TnRollerMine's PlayExplosion camera shake (for the players beyond its damage
/// radius, within 300 m): a long low rumble.
const HOLE_SHAKE: ScreenShake = ScreenShake {
    radius: 300.0,
    secs: 12.0,
    rot_amp: Vec3::new(1.5, 1.0, 1.0),
    rot_freq: Vec3::new(10.0, 75.0, 50.0),
    loc_amp: Vec3::new(1.5, 1.5, 3.5),
    loc_freq: Vec3::new(75.0, 25.0, 50.0),
    fov_amp: 0.0,
    fov_freq: 0.0,
};

// ---------------------------------------------------------------------------
// Models, effects and sounds
// ---------------------------------------------------------------------------

/// The mine (its clips: 0 its idle, Nav_KMine_Idle; 1 its homing, Nav_KMine_Activate)
/// and the ball (tools/build_assets.py's build_equipment_models).
const MINE_MODEL: &str = "models/props/thermo_mine.glb";
const BALL_MODEL: &str = "models/props/decimator_ball.glb";

/// The effects (fx/weapons_fx.txt): the Intercooler's blast as it is used (its
/// ability's OnTriggerEffect) and its buff's loop on everyone it buffs
/// (TnBuffFastAbilityCooldown's ApplyFx: lightning crackling over him).
const FX_MINE_BLAST: &str = "Grenade_EXP_FX";
const FX_MINE_STEAM: &str = "MineSteam_FX";
const FX_RECHARGE: &str = "KillStreak_HealthRegen_FX";
const FX_INTERCOOLER: &str = "Intercooler_Blast_FX";
/// The robot socket the Intercooler's blast plays on (see blast_place).
const BLAST_SOCKET: &str = "PositionSocket";
const FX_INTERCOOLED: &str = "Intercooler_FX";
const FX_BALL_BLAST: &str = "GravityBomb_Exp_FX";
const FX_HOLE: &str = "BlackHole_FX";
const FX_HOLE_END: &str = "BlackHole_End_FX";
/// The barrier's three layers (TR_Barrier_Hexigons_MAT_INST,
/// TR_Barrier_Distortion_AA_MATINST and TR_Barrier_EdgeGlow_MAT_INST:
/// build_weapon_fx.py's mesh systems): the hexagons, then the distortion with its
/// sweeping band and the edge, those two in his side's colour (the Autobots', the
/// Decepticons').
const FX_BARRIER: &str = "Barrier_Wall_Hex_FX";
const FX_BARRIER_SIDE: [[&str; 2]; 2] = [
    [
        "Barrier_Wall_Distortion_Autobot_FX",
        "Barrier_Wall_Edge_Autobot_FX",
    ],
    [
        "Barrier_Wall_Distortion_Decepticon_FX",
        "Barrier_Wall_Edge_Decepticon_FX",
    ],
];

const SND_MINE_DEPLOY: &str = "BL_WPN_GRENADE.KAMIKAZE_DEPLOY";
const SND_MINE_IDLE: &str = "BL_WPN_GRENADE.KAMIKAZE_FLIGHT_LP_IDLE";
const SND_MINE_TRACKING: &str = "BL_WPN_GRENADE.KAMIKAZE_FLIGHT_LP_TRACKING";
const SND_MINE_FUSE: &str = "BL_WPN_GRENADE.KAMIKAZE_FUSE_START";
const SND_MINE_BLAST: &str = "BL_WPN_GRENADE.KAMIKAZE_EXPL_IMPT_WORLD";
const SND_BARRIER_DEPLOY: &str = "BL_TRANS_POWER.BARRIER_DEPLOY";
const SND_BARRIER_LOOP: &str = "BL_TRANS_POWER.BARRIER_LP";
const SND_BARRIER_RETRACT: &str = "BL_TRANS_POWER.BARRIER_RETRACT";
const SND_RECHARGE_LOOP: &str = "BL_TRANS_POWER.ENERGON_RECHARGER_START_LP";
const SND_RECHARGE_END: &str = "BL_TRANS_POWER.ENERGON_RECHARGER_END";
const SND_INTERCOOLER_LOOP: &str = "BL_TRANS_POWER.INTERCOOLER_START_LP";
const SND_INTERCOOLER_END: &str = "BL_TRANS_POWER.INTERCOOLER_END";
const SND_BALL_THROW: &str = "BL_WPN_GRENADE.GRAVITY_GRENADE_DEPLOY_ROBOT";
const SND_BALL_DEPLOY: &str = "BL_WPN_GRENADE.GRAVITY_GRENADE_DEPLOY";
const SND_BALL_LOOP: &str = "BL_WPN_GRENADE.GRAVITY_GRENADE_FLIGHT_LP";
const SND_BALL_FUSE: &str = "BL_WPN_GRENADE.GRAVITY_GRENADE_FUSE";
const SND_BALL_BLAST: &str = "BL_WPN_GRENADE.GRAVITY_GRENADE_IMPT_WORLD";
const SND_HOLE_LOOP: &str = "BL_WPN_GRENADE.GRAVITY_GRENADE_CYCLONE_LP";
const SND_HOLE_END: &str = "BL_WPN_GRENADE.GRAVITY_GRENADE_CYCLONE_END_EXPLO";

/// The Thermo Mine Payload's announcement (TnKillstreakActivatedMinePooper's
/// SelfAnnouncementMessage, in TnMessageTextColors' NeutralColor).
const MINE_MESSAGE: &str = "Thermo Mine Deployed";
const NEUTRAL: u8 = 3;

/// A Jazz's chest (his effects' Chest_FX socket), over his feet in robot form.
const CHEST: f32 = 2.2;

pub struct EquipmentPlugin;

impl Plugin for EquipmentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RemoteGear>()
            .add_systems(Startup, load_support_assets)
            .add_systems(OnEnter(crate::menu::AppState::Game), clear_support)
            .add_systems(
                Update,
                (
                    // What his keys do, before he moves (his speed) and fights.
                    (use_support, remote_support, support_buffs)
                        .chain()
                        .after(crate::weapon_pickups)
                        .before(crate::player_movement),
                    (fly_mines, stand_barriers, roll_balls, black_holes)
                        .chain()
                        .after(crate::melee_hits)
                        .before(crate::health_stations),
                    animate_mines,
                )
                    .run_if(in_state(crate::menu::AppState::Game))
                    .before(crate::hud::HudSystems),
            );
    }
}

// ---------------------------------------------------------------------------
// A Jazz's support items
// ---------------------------------------------------------------------------

/// A Jazz's support items in use (his TnAbilityManager's equipment slots, Player::
/// equipment, and what they have going): each slot's spam guard (seconds left) and
/// key last frame, whether its item has been used (and which: its ability, active
/// while its effect lasts), the items just bought (Player::equipment has them),
/// TnBuffMinePooper's time and mines let out, his barrier standing, his ball to come
/// out (seconds to it) and out, the Energon Recharger's and Intercooler's buffs
/// on him (seconds left) and their effects, and where he used an Intercooler this
/// frame (for his teammates near it on this PC to show its buff).
#[derive(Default)]
pub(crate) struct Support {
    guard: [f32; 2],
    keys: [bool; 2],
    pub spent: [bool; 2],
    used: [Option<Equipment>; 2],
    pub bought: Vec<Equipment>,
    mines: Option<(f32, u32)>,
    barrier: Option<Entity>,
    ball_in: Option<f32>,
    ball: Option<Entity>,
    pub recharge_left: f32,
    pub intercooler_left: f32,
    recharge_fx: Option<FxId>,
    intercooler_fx: Option<FxId>,
    cooled_at: Option<Vec3>,
    /// His use's clip (Nav_ToStealth over his upper body: main.rs's animate_jazz),
    /// seconds into it.
    pub trigger_anim: Option<f32>,
    /// Whether the buffs were on last frame (their loops' starts and ends).
    recharging: bool,
    intercooled: bool,
}

impl Support {
    /// Whether item `kind`'s effect is going (its ability IsActive): the mines'
    /// buff, the barrier, the buffs on him, the ball to come or out.
    fn active(&self, kind: Equipment) -> bool {
        match kind {
            Equipment::MinePayload => self.mines.is_some(),
            Equipment::Barrier => self.barrier.is_some(),
            Equipment::EnergonRecharger => self.recharge_left > 0.0,
            Equipment::Intercooler => self.intercooler_left > 0.0,
            Equipment::Decimator => self.ball_in.is_some() || self.ball.is_some(),
        }
    }

    /// The class ability's refill (and drain's divisor) for the Intercooler's buff
    /// (TnBuffFastAbilityCooldown: the ability manager's CooldownMultiplier).
    pub(crate) fn cooldown_scale(&self) -> f32 {
        if self.intercooler_left > 0.0 {
            INTERCOOLER_COOLDOWN
        } else {
            1.0
        }
    }

    /// His robot's speed for the Intercooler's buff (TnBuffIntercoolerGroundSpeed).
    pub(crate) fn speed_scale(&self) -> f32 {
        if self.intercooler_left > 0.0 {
            INTERCOOLER_SPEED
        } else {
            1.0
        }
    }
}

/// The mine's and ball's models, and the mine's clips (an animation graph of its
/// two: idle and homing). (Their scenes from their files, the mine's its clips' too:
/// gltffiles.rs.)
#[derive(Resource)]
struct SupportAssets {
    mine: Option<crate::gltffiles::SceneOf>,
    ball: Option<crate::gltffiles::SceneOf>,
    mine_graph: Handle<AnimationGraph>,
    mine_clips: [AnimationNodeIndex; 2],
}

fn load_support_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut clips: ResMut<GltfClips>,
) {
    let scene = |path: &'static str| {
        crate::asset_exists(path).then(|| crate::gltffiles::SceneOf::load(&asset_server, path))
    };
    let mut graph = AnimationGraph::new();
    let mut pending = Vec::new();
    let root = graph.root;
    let idle = crate::gltf_clip(&mut graph, &mut pending, 0, root);
    let activate = crate::gltf_clip(&mut graph, &mut pending, 1, root);
    let mine_graph = graphs.add(graph);
    let mine = scene(MINE_MODEL);
    if let Some(mine) = &mine {
        clips.wait(mine.file.clone(), mine_graph.clone(), pending);
    }
    commands.insert_resource(SupportAssets {
        mine,
        ball: scene(BALL_MODEL),
        mine_graph,
        mine_clips: [idle, activate],
    });
}

/// The other players' support items (their PCs' Gear, Mine and GearEnd events,
/// net_apply's) for remote_support: whose (his Jazz here) and what.
#[derive(Resource, Default)]
pub(crate) struct RemoteGear(pub Vec<(Entity, JazzEvent)>);

/// Whether a Jazz is down (dead, or dying this frame): his things end.
fn down(player: &Player) -> bool {
    player.dead_left > 0.0 || player.dying
}

/// The way he looks, on the ground (his pawn's Rotation: the view's heading, its
/// pitch left out), and to his right.
fn facing(input: &PlayerInput) -> (Vec3, Vec3) {
    let turn = Quat::from_rotation_y(input.yaw);
    (turn * Vec3::NEG_Z, turn * Vec3::X)
}

/// His middle (his pawn's Location: his collision cylinder's) standing at `feet`.
fn middle(player: &Player, feet: Vec3) -> Vec3 {
    player.centre(feet)
}

/// Where the Intercooler's blast goes off on `jazz`: his robot's PositionSocket (on
/// his lower spine), as his ability's OnTriggerEffect plays it there
/// (OnTriggerEffectSocket, PEM.PlayEffectOnSocket); his middle (`centre`) without
/// his robot's bones to hand. (Attached to his mesh, the game would draw it at his
/// robot's size, Robot::effect_scale, as the buffs' effects; it is kept at full size
/// for now: in the user's Intercooler video its sphere sweeps out past the camera,
/// further than the demo's does even at full size.)
fn blast_place(sockets: &mut Sockets, jazz: Entity, centre: Vec3) -> Vec3 {
    sockets
        .frame(jazz, BLAST_SOCKET)
        .map_or(centre, |(at, _, _)| at)
}

/// The keys, the items' uses and their ends, for this PC's Jazz (the others' PCs run
/// theirs): what each slot's key does, the mines let out, the ball coming out, and
/// the slots emptied as their effects end; all of it lost as he dies.
#[allow(clippy::too_many_arguments)]
fn use_support(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<Level>,
    colliders: Res<Colliders>,
    assets: Res<SupportAssets>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut players: Query<(Entity, &Transform, &mut Player, &PlayerInput), With<LocalPlayer>>,
    mut barriers: Query<&mut Barrier>,
    mut balls: Query<&mut DecimatorBall>,
    mut sockets: Sockets,
) {
    let dt = time.delta_secs();
    for (entity, transform, mut player, input) in &mut players {
        let player = &mut *player;
        let feet = transform.translation;
        let decepticon = player.look.decepticon;
        // (His things lose track of what has gone of itself.)
        let s = &mut player.support;
        if s.barrier.is_some_and(|b| barriers.get(b).is_err()) {
            s.barrier = None;
        }
        if s.ball.is_some_and(|b| balls.get(b).is_err()) {
            s.ball = None;
        }
        for g in &mut s.guard {
            *g = (*g - dt).max(0.0);
        }

        // Dead: what he had is lost (a new pawn has a new ability manager), and
        // what it had going ends (destroyed on his death, his buffs stopped: their
        // mines fade out by themselves, fly_mines).
        if down(player) {
            end_support(player, &mut barriers, &mut balls, &mut fx, false);
            player.equipment = [None; 2];
            continue;
        }

        // Downed (revive.rs): his barrier goes and his ball goes off as his active
        // abilities are ended (RobotDowned's ForceEndActiveAbilities); his buffs run
        // on, what he hasn't used stays his, and his keys do nothing till he is up.
        if player.revive.locked() {
            if player.revive.downed.is_some() {
                for kind in [Equipment::Barrier, Equipment::Decimator] {
                    if player.support.active(kind) {
                        force_end(player, kind, &mut barriers, &mut balls);
                        player
                            .played
                            .push(JazzEvent::GearEnd { item: kind.index() });
                    }
                }
            }
            player.support.keys = input.tech;
            continue;
        }

        // Bought (weapon_pickups): what the slot's item had going ends
        // (ReplaceAbility's force end: the barrier let down, the ball set off; a
        // buff runs on), and the slot's ability is new, ready.
        for kind in std::mem::take(&mut player.support.bought) {
            let slot = kind.slot();
            if let Some(old) = player.support.used[slot].take() {
                force_end(player, old, &mut barriers, &mut balls);
            }
            player.support.spent[slot] = false;
        }

        // The keys (TriggerAbility 4 and 5): in robot form only, standing as a
        // robot (not changing form).
        let robot = player.form == Form::Robot
            && player.transform_left <= 0.0
            && player.pending_form.is_none();
        for slot in 0..2 {
            let held = input.tech[slot];
            let pressed = held && !player.support.keys[slot];
            player.support.keys[slot] = held;
            if !pressed || !robot || player.support.guard[slot] > 0.0 {
                continue;
            }
            let Some(kind) = player.equipment[slot] else {
                continue;
            };
            // In use and cancellable (CanBeCancelled && IsActive): it ends.
            if player.support.used[slot] == Some(kind)
                && matches!(kind, Equipment::Barrier | Equipment::Decimator)
                && player.support.active(kind)
            {
                force_end(player, kind, &mut barriers, &mut balls);
                player
                    .played
                    .push(JazzEvent::GearEnd { item: kind.index() });
                continue;
            }
            if player.support.spent[slot] {
                continue;
            }
            // Used: its one use spent, its effect started, its clip played
            // (PlayTriggerAnim).
            player.support.spent[slot] = true;
            player.support.used[slot] = Some(kind);
            player.support.guard[slot] = SPAM_SECS;
            player.support.trigger_anim = Some(0.0);
            let (forward, right) = facing(input);
            let centre = middle(player, feet);
            match kind {
                Equipment::MinePayload => {
                    player.support.mines = Some((0.0, 0));
                    sfx.play(SND_MINE_DEPLOY);
                    player.hud_events.push(HudEvent::GameMessage(vec![(
                        NEUTRAL,
                        MINE_MESSAGE.to_string(),
                    )]));
                    player.played.push(JazzEvent::Gear {
                        item: kind.index(),
                        at: centre,
                        facing: forward,
                    });
                }
                Equipment::Barrier => {
                    // (One barrier each: his class ability's goes, abilities.rs.)
                    if let Some(b) = player.ability.barrier.take() {
                        lower_barrier(&mut barriers, b);
                        player.ability.end(crate::abilities::End::Forced);
                    }
                    sfx.play(SND_BARRIER_DEPLOY);
                    let at = centre + forward * BARRIER_OFFSET.x + Vec3::Y * BARRIER_OFFSET.y;
                    player.support.barrier = Some(spawn_barrier(
                        &mut commands,
                        &mut fx,
                        Some(entity),
                        at,
                        forward,
                        right,
                        decepticon,
                    ));
                    player.played.push(JazzEvent::Gear {
                        item: kind.index(),
                        at,
                        facing: forward,
                    });
                }
                Equipment::EnergonRecharger => {
                    player.support.recharge_left = RECHARGE_SECS;
                    player.played.push(JazzEvent::Gear {
                        item: kind.index(),
                        at: centre,
                        facing: forward,
                    });
                }
                Equipment::Intercooler => {
                    // (Him and the teammates near him: theirs on their PCs, from
                    // this event.)
                    player.support.intercooler_left = INTERCOOLER_SECS;
                    player.support.cooled_at = Some(feet);
                    let at = blast_place(&mut sockets, entity, centre);
                    fx.spawn(FX_INTERCOOLER, at, forward, &[]);
                    player.played.push(JazzEvent::Gear {
                        item: kind.index(),
                        at: feet,
                        facing: forward,
                    });
                }
                Equipment::Decimator => {
                    sfx.play(SND_BALL_THROW);
                    player.support.ball_in = Some(BALL_DELAY);
                }
            }
        }

        // The mines let out (TnBuffMinePooper.PoopMine): each where his offset,
        // turned on 50 degrees more each time, puts it from him as he stands then,
        // or in against what is in the way.
        let mut mines = player.support.mines;
        if let Some((t, done)) = mines.as_mut() {
            *t += dt;
            while *done < 3 && *t >= MINE_RATE * (*done + 1) as f32 {
                let (forward, right) = facing(input);
                let turn = Quat::from_axis_angle(Vec3::Y, -(MINE_TURN * *done as f32).to_radians());
                let offset = turn * (forward * MINE_OFFSET.x + right * MINE_OFFSET.y);
                let centre = middle(player, feet);
                let len = offset.length();
                let dir = offset / len.max(1e-4);
                let reach = level
                    .shot_surface(&colliders, centre, dir, len + MINE_TRACE)
                    .map_or(len, |(t, _)| (t - MINE_TRACE).clamp(0.0, len));
                let at = centre + dir * reach;
                spawn_mine(&mut commands, &assets, Some(entity), at, forward);
                player.played.push(JazzEvent::Mine { at });
                *done += 1;
            }
            if *t >= MINE_BUFF_SECS {
                mines = None;
            }
        }
        player.support.mines = mines;

        // The ball comes out once its place is clear (IsSafeSpawnLocation: nothing
        // of the level within its reach), else a second later.
        if let Some(left) = player.support.ball_in {
            let left = left - dt;
            if left > 0.0 {
                player.support.ball_in = Some(left);
            } else {
                let (forward, _) = facing(input);
                let at = middle(player, feet) + forward * BALL_SPAWN.x + Vec3::Y * BALL_SPAWN.y;
                if clear_for_ball(&level, at) {
                    player.support.ball_in = None;
                    player.support.ball = Some(spawn_ball(
                        &mut commands,
                        &assets,
                        &mut sfx,
                        Some(entity),
                        at,
                        forward,
                    ));
                    player.played.push(JazzEvent::Gear {
                        item: Equipment::Decimator.index(),
                        at,
                        facing: forward,
                    });
                } else {
                    player.support.ball_in = Some(BALL_RETRY);
                }
            }
        }

        // Spent and over: the slot empties (TnAbility.ShouldBeDiscarded).
        for slot in 0..2 {
            let s = &player.support;
            if s.spent[slot] && !s.used[slot].is_some_and(|k| s.active(k)) {
                player.equipment[slot] = None;
                player.support.spent[slot] = false;
                player.support.used[slot] = None;
            }
        }
    }
}

/// Ends item `kind`'s effect for him (its ability's ServerForceEndAbility): his
/// barrier let down, his ball set off (or not to come out); the buffs run on.
fn force_end(
    player: &mut Player,
    kind: Equipment,
    barriers: &mut Query<&mut Barrier>,
    balls: &mut Query<&mut DecimatorBall>,
) {
    let s = &mut player.support;
    match kind {
        Equipment::Barrier => {
            if let Some(b) = s.barrier.take()
                && let Ok(mut barrier) = barriers.get_mut(b)
            {
                barrier.ending = true;
            }
        }
        Equipment::Decimator => {
            s.ball_in = None;
            if let Some(b) = s.ball.take()
                && let Ok(mut ball) = balls.get_mut(b)
            {
                ball.set_off = true;
            }
        }
        _ => {}
    }
}

/// Everything of his ended as he dies: his barrier and ball gone at once (destroyed
/// with him: no retract, no black hole), his buffs stopped (`sounds`: their ends'
/// sounds), the mines' buff over, the slots' uses forgotten.
fn end_support(
    player: &mut Player,
    barriers: &mut Query<&mut Barrier>,
    balls: &mut Query<&mut DecimatorBall>,
    fx: &mut FxSystems,
    sounds: bool,
) {
    let s = &mut player.support;
    if let Some(b) = s.barrier.take()
        && let Ok(mut barrier) = barriers.get_mut(b)
    {
        barrier.gone = true;
    }
    s.ball_in = None;
    if let Some(b) = s.ball.take()
        && let Ok(mut ball) = balls.get_mut(b)
    {
        ball.gone = true;
    }
    s.mines = None;
    s.recharge_left = 0.0;
    s.intercooler_left = 0.0;
    if !sounds {
        s.recharging = false;
        s.intercooled = false;
    }
    if let Some(id) = s.recharge_fx.take() {
        fx.kill(id);
    }
    if let Some(id) = s.intercooler_fx.take() {
        fx.kill(id);
    }
    s.spent = [false; 2];
    s.used = [None; 2];
    s.bought.clear();
}

/// The other players' support items, as their PCs tell of them: their barriers,
/// mines and balls here too, their buffs' effects on their Jazzes here, and the
/// Intercooler's boost for this PC's player if he is near enough.
#[allow(clippy::too_many_arguments)]
fn remote_support(
    mut commands: Commands,
    mut remote: ResMut<RemoteGear>,
    assets: Res<SupportAssets>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut players: Query<(Entity, &Transform, &mut Player, Has<RemotePlayer>)>,
    mut barriers: Query<&mut Barrier>,
    mut balls: Query<&mut DecimatorBall>,
    mut sockets: Sockets,
) {
    for (owner, event) in std::mem::take(&mut remote.0) {
        // (Its Intercooler's place, for this PC's player once the Jazz is let be.)
        let mut intercooler = None;
        {
            let Ok((_, transform, mut player, true)) = players.get_mut(owner) else {
                continue;
            };
            let feet = transform.translation;
            let decepticon = player.look.decepticon;
            match event {
                JazzEvent::Gear { item, at, facing } => {
                    let Some(kind) = Equipment::from_index(item) else {
                        continue;
                    };
                    player.support.trigger_anim = Some(0.0);
                    let facing = Vec3::new(facing.x, 0.0, facing.z).normalize_or(Vec3::NEG_Z);
                    match kind {
                        Equipment::MinePayload => {}
                        Equipment::Barrier => {
                            let right = facing.cross(Vec3::Y);
                            if let Some(old) = player.support.barrier.take()
                                && let Ok(mut b) = barriers.get_mut(old)
                            {
                                b.ending = true;
                            }
                            player.support.barrier = Some(spawn_barrier(
                                &mut commands,
                                &mut fx,
                                Some(owner),
                                at,
                                facing,
                                right,
                                decepticon,
                            ));
                        }
                        Equipment::EnergonRecharger => {
                            player.support.recharge_left = RECHARGE_SECS;
                        }
                        Equipment::Intercooler => {
                            player.support.intercooler_left = INTERCOOLER_SECS;
                            player.support.cooled_at = Some(at);
                            let centre = player.centre(feet);
                            let place = blast_place(&mut sockets, owner, centre);
                            fx.spawn(FX_INTERCOOLER, place, facing, &[]);
                            intercooler = Some(at);
                        }
                        Equipment::Decimator => {
                            if let Some(old) = player.support.ball.take()
                                && let Ok(mut b) = balls.get_mut(old)
                            {
                                b.set_off = true;
                            }
                            player.support.ball = Some(spawn_ball(
                                &mut commands,
                                &assets,
                                &mut sfx,
                                Some(owner),
                                at,
                                facing,
                            ));
                        }
                    }
                }
                JazzEvent::Mine { at } => {
                    let facing = (at - feet).with_y(0.0).normalize_or(Vec3::NEG_Z);
                    spawn_mine(&mut commands, &assets, Some(owner), at, facing);
                }
                JazzEvent::GearEnd { item } => match Equipment::from_index(item) {
                    Some(Equipment::Barrier) => {
                        if let Some(b) = player.support.barrier.take()
                            && let Ok(mut barrier) = barriers.get_mut(b)
                        {
                            barrier.ending = true;
                        }
                    }
                    Some(Equipment::Decimator) => {
                        if let Some(b) = player.support.ball.take()
                            && let Ok(mut ball) = balls.get_mut(b)
                        {
                            ball.set_off = true;
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        // (TnTeamHandler.GetTeamMembers: this PC's player, if he is within its
        // range.)
        if let Some(at) = intercooler {
            intercool_local(&mut players, at);
        }
    }
}

/// A teammate's Intercooler used at `at`: this PC's player gets its buff if he is
/// alive and within its range.
fn intercool_local(
    players: &mut Query<(Entity, &Transform, &mut Player, Has<RemotePlayer>)>,
    at: Vec3,
) {
    for (_, transform, mut player, remote) in players.iter_mut() {
        if remote || down(&player) || player.waiting {
            continue;
        }
        if transform.translation.distance(at) <= INTERCOOLER_RANGE {
            player.support.intercooler_left = INTERCOOLER_SECS;
        }
    }
}

/// The Energon Recharger's and the Intercooler's buffs running out, for every Jazz:
/// their effects on his chest (KillStreak_HealthRegen_FX and Intercooler_FX,
/// looping; on his vehicle's middle in the car), and, for this PC's player, their
/// loops and ends. An Intercooler used this frame shows its buff on the other
/// players' Jazzes near it here too (their own PCs give them its boost).
fn support_buffs(
    time: Res<Time>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut players: Query<(Entity, &Transform, &mut Player, Has<LocalPlayer>)>,
) {
    let dt = time.delta_secs();
    let blasts: Vec<(Entity, Vec3)> = players
        .iter_mut()
        .filter_map(|(e, _, mut p, _)| p.support.cooled_at.take().map(|at| (e, at)))
        .collect();
    for (entity, transform, mut player, local) in &mut players {
        if !local
            && !down(&player)
            && !player.waiting
            && blasts.iter().any(|&(user, at)| {
                user != entity && transform.translation.distance(at) <= INTERCOOLER_RANGE
            })
        {
            player.support.intercooler_left = INTERCOOLER_SECS;
        }
    }
    for (entity, transform, mut player, local) in &mut players {
        let feet = transform.translation;
        let chest = match player.form {
            Form::Robot => feet + Vec3::Y * CHEST,
            Form::Vehicle => player.centre(feet),
        };
        let forward = *transform.forward();
        let gone = down(&player);
        // (Their effects at his robot's size, as the game attaches them to his mesh.)
        let size = player.kit.robot.effect_scale();
        let s = &mut player.support;
        if gone {
            s.recharge_left = 0.0;
            s.intercooler_left = 0.0;
        }
        s.recharge_left = (s.recharge_left - dt).max(0.0);
        s.intercooler_left = (s.intercooler_left - dt).max(0.0);
        let recharging = s.recharge_left > 0.0;
        let intercooled = s.intercooler_left > 0.0;
        // (Its effect, hidden while he is cloaked: no cloak here.)
        if recharging {
            if s.recharge_fx.is_none_or(|id| !fx.alive(id)) {
                s.recharge_fx = fx.spawn_scaled(FX_RECHARGE, chest, forward, size, &[]);
            }
            if let Some(id) = s.recharge_fx {
                fx.place(id, chest, forward, Vec3::ZERO);
            }
        } else if let Some(id) = s.recharge_fx.take() {
            fx.stop(id);
        }
        if intercooled {
            if s.intercooler_fx.is_none_or(|id| !fx.alive(id)) {
                s.intercooler_fx = fx.spawn_scaled(FX_INTERCOOLED, chest, forward, size, &[]);
            }
            if let Some(id) = s.intercooler_fx {
                fx.place(id, chest, forward, Vec3::ZERO);
            }
        } else if let Some(id) = s.intercooler_fx.take() {
            fx.stop(id);
        }
        if local {
            let owner = entity.to_bits();
            if recharging {
                sfx.hold(owner ^ 0x5243, SND_RECHARGE_LOOP, None);
            } else if s.recharging && !gone {
                sfx.play(SND_RECHARGE_END);
            }
            if intercooled {
                sfx.hold(owner ^ 0x4943, SND_INTERCOOLER_LOOP, None);
            } else if s.intercooled && !gone {
                sfx.play(SND_INTERCOOLER_END);
            }
        }
        s.recharging = recharging;
        s.intercooled = intercooled;
    }
}

/// A new match: nothing of the last one's left out, and nobody's items going.
fn clear_support(
    mut commands: Commands,
    mut fx: ResMut<FxSystems>,
    mines: Query<(Entity, &ThermoMine)>,
    barriers: Query<(Entity, &Barrier)>,
    balls: Query<Entity, With<DecimatorBall>>,
    holes: Query<(Entity, &BlackHole)>,
    mut players: Query<&mut Player>,
) {
    for (e, mine) in &mines {
        if let Some(id) = mine.steam {
            fx.kill(id);
        }
        commands.entity(e).despawn();
    }
    for (e, barrier) in &barriers {
        for id in &barrier.fx {
            fx.kill(*id);
        }
        commands.entity(e).despawn();
    }
    for e in &balls {
        commands.entity(e).despawn();
    }
    for (e, hole) in &holes {
        if let Some(id) = hole.fx {
            fx.kill(id);
        }
        commands.entity(e).despawn();
    }
    for mut player in &mut players {
        if let Some(id) = player.support.recharge_fx.take() {
            fx.kill(id);
        }
        if let Some(id) = player.support.intercooler_fx.take() {
            fx.kill(id);
        }
        player.support = Support::default();
    }
}

// ---------------------------------------------------------------------------
// Thermo Mines
// ---------------------------------------------------------------------------

/// What a mine is doing (TnProjectileKamikazeMine's states): falling (Inactive),
/// hovering over where it came to rest, or homing on an enemy.
#[derive(Clone, Copy, PartialEq, Debug)]
enum MineState {
    Falling,
    Hovering,
    Homing(Entity),
}

/// A Thermo Mine (TnProjectileKamikazeMineKillstreak): whose (its Instigator, a
/// Jazz here), what it is doing and its velocity, where it began to hover and off
/// what (the surface's normal) and for how long, its age and fade (seconds of it),
/// its spin and the way it faces, its model's animation player and the clip it
/// plays, and its steam.
#[derive(Component)]
pub(crate) struct ThermoMine {
    owner: Option<Entity>,
    state: MineState,
    velocity: Vec3,
    base: Vec3,
    normal: Vec3,
    hover_secs: f32,
    age: f32,
    fade: Option<f32>,
    spin: f32,
    facing: Vec3,
    animator: Option<Entity>,
    homing_clip: bool,
    steam: Option<FxId>,
}

/// Its model, under the mine (turned to face the way it faces, spun about its pitch).
#[derive(Component)]
struct MineModel;

fn spawn_mine(
    commands: &mut Commands,
    assets: &SupportAssets,
    owner: Option<Entity>,
    at: Vec3,
    facing: Vec3,
) -> Entity {
    let facing = facing.with_y(0.0).normalize_or(Vec3::NEG_Z);
    let mut mine = commands.spawn((
        ThermoMine {
            owner,
            state: MineState::Falling,
            velocity: Vec3::ZERO,
            base: at,
            normal: Vec3::Y,
            hover_secs: 0.0,
            age: 0.0,
            fade: None,
            spin: 0.0,
            facing,
            animator: None,
            homing_clip: false,
            steam: None,
        },
        Transform::from_translation(at).with_rotation(model_turn(facing, 0.0)),
        Visibility::default(),
    ));
    if let Some(scene) = &assets.mine {
        mine.with_child((
            scene.clone(),
            Transform::default(),
            MineModel,
            crate::OwnAnimation,
        ));
    }
    mine.id()
}

/// The mine's model turned to face `facing` (its +X, Unreal's forward, along it) and
/// spun `spin` about its pitch axis (nose up).
fn model_turn(facing: Vec3, spin: f32) -> Quat {
    let yaw = (-facing.z).atan2(facing.x);
    Quat::from_rotation_y(yaw) * Quat::from_rotation_z(spin)
}

/// Each mine as TnProjectileKamikazeMine moves it: falling to the floor, hovering
/// over it, searching once it has hovered a while, homing, going off; and fading
/// out at the end of its life or as its owner dies.
#[allow(clippy::too_many_arguments)]
fn fly_mines(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<Level>,
    colliders: Res<Colliders>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut mines: Query<(Entity, &mut ThermoMine, &mut Transform)>,
    mut enemies: Query<(Entity, &mut Enemy), Without<Player>>,
    mut players: Query<(Entity, &Transform, &mut Player, Has<LocalPlayer>), Without<ThermoMine>>,
) {
    let dt = time.delta_secs();
    for (entity, mut mine, mut transform) in &mut mines {
        let mine = &mut *mine;
        mine.age += dt;
        let at = transform.translation;
        let voice = entity.to_bits();
        // (Its owner dead, or its life over: it fades away, shrinking, and goes.)
        let owner_down = mine
            .owner
            .is_some_and(|o| players.get(o).map_or(true, |(_, _, p, _)| down(p)));
        if mine.fade.is_none() && (owner_down || mine.age >= MINE_LIFE_SECS) {
            mine.fade = Some(0.0);
        }
        if let Some(f) = mine.fade.as_mut() {
            *f += dt;
            let k = (*f / MINE_FADE_SECS).clamp(0.0, 1.0);
            transform.scale = Vec3::splat(1.0 + (0.01 - 1.0) * k);
            if k >= 1.0 {
                if let Some(id) = mine.steam {
                    fx.stop(id);
                }
                commands.entity(entity).despawn();
            }
            continue;
        }
        let mut blast: Option<(Vec3, Vec3)> = None;
        match mine.state {
            MineState::Falling => {
                sfx.hold(voice, SND_MINE_IDLE, Some(at));
                mine.velocity.y += MINE_GRAVITY * dt;
                let step = mine.velocity * dt;
                let len = step.length();
                let mut next = at + step;
                let mut landed = None;
                if len > 1e-6
                    && let Some((t, n)) = level.shot_surface(&colliders, at, step / len, len)
                {
                    next = at + step / len * (t - 0.05).max(0.0);
                    landed = Some(n);
                }
                // (The floor within its 70 UU trace below it.)
                if landed.is_none()
                    && let Some((_, n)) = level.ray_hit(next, Vec3::NEG_Y, MINE_LAND)
                {
                    landed = Some(n);
                }
                transform.translation = next;
                if let Some(n) = landed {
                    // StartHovering: off the surface it reached.
                    let n = if n.dot(Vec3::Y) < -0.5 { Vec3::Y } else { n };
                    mine.state = MineState::Hovering;
                    mine.base = next;
                    mine.normal = n;
                    mine.velocity = n * MINE_HOVER_SPEED;
                    mine.hover_secs = 0.0;
                }
            }
            MineState::Hovering => {
                sfx.hold(voice, SND_MINE_IDLE, Some(at));
                mine.hover_secs += dt;
                let d = mine.base.distance(at);
                if d <= MINE_HOVER_MIN {
                    mine.velocity = mine.normal * MINE_HOVER_SPEED;
                } else if d >= MINE_HOVER_MAX {
                    mine.velocity = -mine.normal * MINE_HOVER_SPEED;
                }
                transform.translation = at + mine.velocity * dt;
                if mine.hover_secs >= MINE_HOVER_SECS {
                    mine.spin += MINE_SPIN * dt;
                    // FindEnemy: a living enemy within its reach that it can see.
                    let found = enemies
                        .iter()
                        .filter(|(_, e)| e.alive())
                        .map(|(t, e)| (t, e.centre()))
                        .filter(|(_, c)| c.distance(at) <= MINE_SEARCH && !level.blocked(at, *c))
                        .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
                        .map(|(t, _)| t);
                    if let Some(target) = found {
                        // OnFoundTarget: its fuse's sound, the tracking loop in
                        // place of the idle one, and its steam as it starts homing.
                        sfx.play_at(SND_MINE_FUSE, at);
                        mine.state = MineState::Homing(target);
                        mine.homing_clip = true;
                        mine.steam = fx.spawn(FX_MINE_STEAM, at, mine.facing, &[]);
                    }
                }
            }
            MineState::Homing(target) => {
                sfx.hold_fading(voice, SND_MINE_TRACKING, Some(at), MINE_LOOP_FADE);
                let goal = enemies.get(target).ok().filter(|(_, e)| e.alive());
                let Some((_, e)) = goal else {
                    // Its target dead: it falls, hovers and searches again.
                    mine.state = MineState::Falling;
                    mine.velocity = Vec3::ZERO;
                    mine.homing_clip = false;
                    continue;
                };
                let to = e.centre() - at;
                let dir = to.normalize_or(mine.facing);
                mine.velocity = dir * MINE_HOMING_SPEED;
                mine.facing = dir.with_y(0.0).normalize_or(mine.facing);
                let step = MINE_HOMING_SPEED * dt;
                // (It goes off as it reaches an enemy, or a wall in its way.)
                let enemy_hit = enemies
                    .iter()
                    .filter(|(_, e)| e.alive())
                    .filter_map(|(_, e)| {
                        if e.reach(at) <= MINE_TOUCH {
                            Some(0.0)
                        } else {
                            e.ray(at, dir, step)
                        }
                    })
                    .reduce(f32::min);
                let wall = level.shot_surface(&colliders, at, dir, step);
                match (enemy_hit, wall) {
                    (Some(t), w) if w.is_none_or(|(wt, _)| t <= wt) => {
                        blast = Some((at + dir * t, -dir));
                    }
                    (_, Some((t, n))) => blast = Some((at + dir * t, n)),
                    _ => transform.translation = at + dir * step,
                }
            }
        }
        if let Some(id) = mine.steam {
            fx.place(id, transform.translation, mine.facing, mine.velocity);
        }
        transform.rotation = model_turn(mine.facing, mine.spin);
        if let Some((point, normal)) = blast {
            mine_blast(
                &mut commands,
                (&mut fx, &mut sfx),
                (entity, &*mine),
                (point, normal),
                &mut enemies,
                &mut players,
            );
        }
    }
}

/// A mine going off (TnProjectile.Explode): its blast's effect along the surface's
/// normal and its sound, 250 to every enemy within 8.5 m (HurtRadius: through walls,
/// at its full strength all through), the hit ping for its owner if he is one of
/// this PC's players, and the shake for each of them near. (Escalation's blasts
/// don't hurt their own players: riot.rs's SELF_DAMAGE.)
fn mine_blast(
    commands: &mut Commands,
    (fx, sfx): (&mut FxSystems, &mut Sfx),
    (entity, mine): (Entity, &ThermoMine),
    (at, normal): (Vec3, Vec3),
    enemies: &mut Query<(Entity, &mut Enemy), Without<Player>>,
    players: &mut Query<(Entity, &Transform, &mut Player, Has<LocalPlayer>), Without<ThermoMine>>,
) {
    let normal = normal.normalize_or(Vec3::Y);
    if let Some(id) = mine.steam {
        fx.stop(id);
    }
    commands.entity(entity).despawn();
    fx.spawn_scaled(FX_MINE_BLAST, at, normal, MINE_EXPLOSION_SCALE, &[]);
    sfx.play_at(SND_MINE_BLAST, at);
    let mut struck = false;
    for (_, mut e) in enemies.iter_mut() {
        if !e.alive() || e.reach(at) > MINE_RADIUS {
            continue;
        }
        let c = e.centre();
        e.take(
            MINE_DAMAGE,
            mine.owner,
            crate::xp::Blow::Weapon,
            Hit::new(Death::Explosive, c, c - at).of_type("TnDamageTypeKamikazeMine"),
        );
        struck = true;
    }
    for (me, t, mut player, local) in players.iter_mut() {
        if !local {
            continue;
        }
        if struck && mine.owner == Some(me) {
            player.hud_events.push(HudEvent::Hit);
        }
        let mid = middle(&player, t.translation);
        if let Some(felt) = MINE_SHAKE.felt(mid.distance(at), MINE_RADIUS) {
            player.screen_shakes.push(felt);
        }
    }
}

/// The mines' models' clips: idle (Nav_KMine_Idle, looping) as their animation
/// players appear, homing (Nav_KMine_Activate, looping at 0.75) once they home.
fn animate_mines(
    mut commands: Commands,
    assets: Option<Res<SupportAssets>>,
    mut animators: Query<(Entity, &mut AnimationPlayer, Has<AnimationGraphHandle>)>,
    parents: Query<&ChildOf>,
    mut mines: Query<&mut ThermoMine>,
) {
    let Some(assets) = assets else {
        return;
    };
    let [idle, activate] = assets.mine_clips;
    // (A mine's model's player, new: not yet given a graph.)
    for (entity, mut animation_player, has_graph) in &mut animators {
        if has_graph {
            continue;
        }
        let Some(mine) = parents.iter_ancestors(entity).find(|e| mines.contains(*e)) else {
            continue;
        };
        commands
            .entity(entity)
            .insert(AnimationGraphHandle(assets.mine_graph.clone()));
        animation_player.play(idle).repeat();
        if let Ok(mut m) = mines.get_mut(mine) {
            m.animator = Some(entity);
        }
    }
    for mine in &mut mines {
        let Some(animator) = mine.animator else {
            continue;
        };
        let Ok((_, mut animation_player, _)) = animators.get_mut(animator) else {
            continue;
        };
        let homing = animation_player.is_playing_animation(activate);
        if mine.homing_clip && !homing {
            animation_player.stop(idle);
            animation_player
                .play(activate)
                .repeat()
                .set_speed(MINE_ACTIVATE_RATE);
        } else if !mine.homing_clip && homing {
            animation_player.stop(activate);
            animation_player.play(idle).repeat();
        }
    }
}

// ---------------------------------------------------------------------------
// Diffraction Barriers
// ---------------------------------------------------------------------------

/// A Diffraction Barrier standing (TnBarrierSpawnable): whose (a Jazz here), where it
/// stands (its foot) and its axes (forward, and to the right), its health and age,
/// its three layers' effects, and whether it is to go (let down: retracting with its
/// sound; or `gone`, destroyed with its owner).
#[derive(Component)]
pub(crate) struct Barrier {
    owner: Option<Entity>,
    at: Vec3,
    forward: Vec3,
    right: Vec3,
    health: f32,
    age: f32,
    fx: Vec<FxId>,
    ending: bool,
    gone: bool,
}

/// A box of a barrier's where it stands (the demo's axes, metres): its middle, its
/// axes (unit) and its half sizes along them.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BarrierBox {
    pub centre: Vec3,
    pub axes: [Vec3; 3],
    pub half: Vec3,
}

/// How big a barrier `age` seconds old has grown (its Barrier_Equip clip's
/// WEP_Body_XB scale).
fn grown(age: f32) -> f32 {
    let k = (age / BARRIER_GROW_SECS).clamp(0.0, 1.0);
    BARRIER_MIN_SCALE + (1.0 - BARRIER_MIN_SCALE) * k
}

/// A barrier's collision's boxes, the barrier standing at `at` facing `forward`
/// (`right` to its right) and grown to `scale`.
pub(crate) fn barrier_boxes(at: Vec3, forward: Vec3, right: Vec3, scale: f32) -> [BarrierBox; 10] {
    let frame = |v: [f32; 3]| forward * v[0] + right * v[1] + Vec3::Y * v[2];
    BARRIER_BOXES.map(|(c, axes, size)| BarrierBox {
        centre: at + frame(c) * (0.01 * scale),
        axes: axes.map(|a| frame(a).normalize_or(Vec3::X)),
        half: Vec3::from_array(size) * (0.005 * scale),
    })
}

impl Barrier {
    /// Its collision's boxes where it stands, as big as it has grown.
    pub(crate) fn boxes(&self) -> [BarrierBox; 10] {
        barrier_boxes(self.at, self.forward, self.right, grown(self.age))
    }

    /// A blow to it (TnBarrierSpawnable.TakeDamage: an Escalation barrier takes
    /// damage, BarrierBP[1].TakesDamage): its health running out lets it down
    /// (stand_barriers).
    pub(crate) fn damage(&mut self, amount: f32) {
        self.health -= amount.max(0.0);
    }

    /// Where it stands (its foot): the same on every PC, its copies raised there.
    pub(crate) fn foot(&self) -> Vec3 {
        self.at
    }
}

/// Where a ray from `from` along unit `dir` enters box `b` within `max`, and the
/// face it enters by (its outward normal): at once, facing back along the ray,
/// for one starting inside it.
pub(crate) fn ray_box(b: &BarrierBox, from: Vec3, dir: Vec3, max: f32) -> Option<(f32, Vec3)> {
    let o = from - b.centre;
    let (mut t0, mut t1) = (0.0f32, max);
    let mut normal = -dir;
    for k in 0..3 {
        let (a, e) = (b.axes[k], b.half[k]);
        let (p, d) = (o.dot(a), dir.dot(a));
        if d.abs() < 1e-8 {
            if p.abs() > e {
                return None;
            }
            continue;
        }
        let (near, far) = {
            let (n, f) = ((-e - p) / d, (e - p) / d);
            if n <= f { (n, f) } else { (f, n) }
        };
        if near > t0 {
            t0 = near;
            normal = -a * d.signum();
        }
        t1 = t1.min(far);
        if t0 > t1 {
            return None;
        }
    }
    Some((t0, normal))
}

/// The first of these barriers (each with its boxes) a round from `from` along
/// unit `dir` strikes within `max`: how far along, the face it strikes (its
/// outward normal) and which barrier.
pub(crate) fn barrier_hit<'a>(
    barriers: impl IntoIterator<Item = &'a (Entity, [BarrierBox; 10])>,
    from: Vec3,
    dir: Vec3,
    max: f32,
) -> Option<(f32, Vec3, Entity)> {
    let mut best: Option<(f32, Vec3, Entity)> = None;
    for (e, boxes) in barriers {
        for b in boxes {
            let reach = best.map_or(max, |h| h.0);
            if let Some((t, n)) = ray_box(b, from, dir, reach)
                && best.is_none_or(|h| t < h.0)
            {
                best = Some((t, n, *e));
            }
        }
    }
    best
}

/// The barriers standing, each with its boxes where it stands now (for the
/// enemies' rounds to stop at).
pub(crate) fn barrier_shapes<'a>(
    barriers: impl IntoIterator<Item = (Entity, &'a Barrier)>,
) -> Vec<(Entity, [BarrierBox; 10])> {
    barriers
        .into_iter()
        .filter(|(_, b)| !b.ending && !b.gone && b.health > 0.0)
        .map(|(e, b)| (e, b.boxes()))
        .collect()
}

/// A barrier stands between a blast at `from` and a body's middle `to` (a
/// HurtRadius can't see him through it).
pub(crate) fn barrier_between(
    barriers: &[(Entity, [BarrierBox; 10])],
    from: Vec3,
    to: Vec3,
) -> bool {
    let d = to - from;
    let len = d.length();
    len > 1e-4 && barrier_hit(barriers, from, d / len, len).is_some()
}

/// How near a blast at `at` comes to a barrier's boxes (the distance to the
/// nearest of them: none inside one).
pub(crate) fn barrier_reach(boxes: &[BarrierBox; 10], at: Vec3) -> f32 {
    boxes
        .iter()
        .map(|b| {
            let o = at - b.centre;
            let q = Vec3::new(
                (o.dot(b.axes[0]).abs() - b.half.x).max(0.0),
                (o.dot(b.axes[1]).abs() - b.half.y).max(0.0),
                (o.dot(b.axes[2]).abs() - b.half.z).max(0.0),
            );
            q.length()
        })
        .fold(f32::INFINITY, f32::min)
}

fn spawn_barrier(
    commands: &mut Commands,
    fx: &mut FxSystems,
    owner: Option<Entity>,
    at: Vec3,
    forward: Vec3,
    right: Vec3,
    decepticon: bool,
) -> Entity {
    let mut layers = Vec::new();
    for name in [FX_BARRIER]
        .iter()
        .chain(&FX_BARRIER_SIDE[usize::from(decepticon)])
    {
        if let Some(id) = fx.spawn_scaled(name, at, forward, BARRIER_MIN_SCALE, &[]) {
            fx.place_turned(id, at, forward, right);
            layers.push(id);
        }
    }
    commands
        .spawn((
            Barrier {
                owner,
                at,
                forward,
                right,
                health: BARRIER_HEALTH,
                age: 0.0,
                fx: layers,
                ending: false,
                gone: false,
            },
            Transform::from_translation(at),
        ))
        .id()
}

/// Raises a Diffraction Barrier for `owner` as the Leader's class ability
/// (abilities.rs): where the support item's goes (BARRIER_OFFSET from his middle as
/// he looks), with its deploy sound. Returns it, where it stands and the way it
/// faces.
pub(crate) fn raise_barrier(
    commands: &mut Commands,
    fx: &mut FxSystems,
    sfx: &mut Sfx,
    owner: Entity,
    player: &Player,
    feet: Vec3,
    input: &PlayerInput,
) -> (Entity, Vec3, Vec3) {
    let (forward, right) = facing(input);
    let at = middle(player, feet) + forward * BARRIER_OFFSET.x + Vec3::Y * BARRIER_OFFSET.y;
    sfx.play(SND_BARRIER_DEPLOY);
    let b = spawn_barrier(
        commands,
        fx,
        Some(owner),
        at,
        forward,
        right,
        player.look.decepticon,
    );
    (b, at, forward)
}

/// Lets barrier `b` down (its retract).
pub(crate) fn lower_barrier(barriers: &mut Query<&mut Barrier>, b: Entity) {
    if let Ok(mut barrier) = barriers.get_mut(b) {
        barrier.ending = true;
    }
}

/// His support item's barrier let down as his class ability's goes up (one barrier
/// each; the new one's Gear event takes the old one down on the other PCs).
pub(crate) fn lower_support_barrier(player: &mut Player, barriers: &mut Query<&mut Barrier>) {
    if let Some(b) = player.support.barrier.take() {
        lower_barrier(barriers, b);
    }
}

/// Each barrier growing in, wearing away (DegenRate a second), its loop heard from it,
/// and going as its health runs out or it is let down (its retract's sound), or with
/// its owner.
fn stand_barriers(
    mut commands: Commands,
    time: Res<Time>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut barriers: Query<(Entity, &mut Barrier)>,
    players: Query<&Player>,
) {
    let dt = time.delta_secs();
    for (entity, mut b) in &mut barriers {
        let b = &mut *b;
        b.age += dt;
        b.health -= BARRIER_DEGEN * dt;
        let owner_down = b.owner.is_some_and(|o| players.get(o).map_or(true, down));
        let middle = b.at + Vec3::Y * BARRIER_MIDDLE;
        if b.gone || owner_down {
            for id in b.fx.drain(..) {
                fx.kill(id);
            }
            commands.entity(entity).despawn();
            continue;
        }
        if b.ending || b.health <= 0.0 {
            sfx.play_at(SND_BARRIER_RETRACT, middle);
            for id in b.fx.drain(..) {
                fx.kill(id);
            }
            commands.entity(entity).despawn();
            continue;
        }
        let scale = grown(b.age);
        for &id in &b.fx {
            fx.set_scale(id, scale);
            fx.place_turned(id, b.at, b.forward, b.right);
        }
        sfx.hold(entity.to_bits(), SND_BARRIER_LOOP, Some(middle));
    }
}

// ---------------------------------------------------------------------------
// Dimensional Decimators
// ---------------------------------------------------------------------------

/// A Dimensional Decimator's ball (TnRollerMineGravityAura): whose (a Jazz here), its
/// velocity, its turn as it rolls, its age, whether its fuse's sound has played, and
/// whether it is to go off now (set off: its key again, his going down) or go without
/// (destroyed with its owner).
#[derive(Component)]
pub(crate) struct DecimatorBall {
    owner: Option<Entity>,
    velocity: Vec3,
    age: f32,
    fused: bool,
    set_off: bool,
    gone: bool,
}

/// Whether the ball has room to come out at `at` (TnSpawnLocationValidator's
/// IsSafeSpawnLocation: no wall or floor within its clearance).
fn clear_for_ball(level: &Level, at: Vec3) -> bool {
    [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Z,
        Vec3::NEG_Z,
        Vec3::Y,
        Vec3::NEG_Y,
    ]
    .iter()
    .all(|d| level.ray(at, *d, BALL_CLEARANCE).is_none())
}

fn spawn_ball(
    commands: &mut Commands,
    assets: &SupportAssets,
    sfx: &mut Sfx,
    owner: Option<Entity>,
    at: Vec3,
    forward: Vec3,
) -> Entity {
    sfx.play_at(SND_BALL_DEPLOY, at);
    let mut ball = commands.spawn((
        DecimatorBall {
            owner,
            velocity: forward * BALL_SPEED.x + Vec3::Y * BALL_SPEED.y,
            age: 0.0,
            fused: false,
            set_off: false,
            gone: false,
        },
        Transform::from_translation(at).with_rotation(model_turn(forward, 0.0)),
        Visibility::default(),
    ));
    if let Some(scene) = &assets.ball {
        ball.with_child((scene.clone(), Transform::default()));
    }
    ball.id()
}

/// Each ball rolling (a rigid body: falling, bouncing off the level, rolling along
/// it), its loop heard from it, its fuse's sound, and going off as its fuse runs out,
/// as it strikes an enemy once armed, or as it is set off; gone without a black hole
/// with its owner.
#[allow(clippy::too_many_arguments)]
fn roll_balls(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<Level>,
    colliders: Res<Colliders>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut balls: Query<(Entity, &mut DecimatorBall, &mut Transform)>,
    enemies: Query<&Enemy, Without<Player>>,
    mut players: Query<(Entity, &Transform, &mut Player, Has<LocalPlayer>), Without<DecimatorBall>>,
) {
    let dt = time.delta_secs();
    let local = players
        .iter()
        .find(|p| p.3)
        .map(|(e, t, p, _)| (e, middle(p, t.translation)));
    for (entity, mut ball, mut transform) in &mut balls {
        let ball = &mut *ball;
        ball.age += dt;
        let owner_down = ball
            .owner
            .is_some_and(|o| players.get(o).map_or(true, |(_, _, p, _)| down(p)));
        if ball.gone || owner_down {
            commands.entity(entity).despawn();
            continue;
        }
        let at = transform.translation;
        sfx.hold(entity.to_bits(), SND_BALL_LOOP, Some(at));
        if !ball.fused && ball.age >= BALL_FUSE_SOUND_SECS {
            ball.fused = true;
            sfx.play_at(SND_BALL_FUSE, at);
        }
        let armed = ball.age >= BALL_ARM_SECS;
        let touched = armed
            && enemies
                .iter()
                .any(|e| e.alive() && e.reach(at) <= BALL_RADIUS);
        if ball.set_off || touched || ball.age >= BALL_FUSE_SECS {
            // Explode: no blast of its own (_DamageAmount 0); its effect and sound,
            // the black hole in its place, and its rumble for this PC's player.
            commands.entity(entity).despawn();
            let facing = *transform.forward();
            fx.spawn(FX_BALL_BLAST, at, facing, &[]);
            sfx.play_at(SND_BALL_BLAST, at);
            spawn_black_hole(&mut commands, &mut fx, ball.owner, at);
            if let Some((me, mid)) = local
                && let Some(felt) = HOLE_SHAKE.felt(mid.distance(at), 0.0)
                && let Ok((_, _, mut player, _)) = players.get_mut(me)
            {
                player.screen_shakes.push(felt);
            }
            continue;
        }
        // Its fall and roll: gravity, then its move, bouncing off what it strikes
        // (BALL_BOUNCE of its speed into it, BALL_GRIP along it).
        ball.velocity.y += BALL_GRAVITY * dt;
        let mut pos = at;
        let mut left = dt;
        for _ in 0..3 {
            let step = ball.velocity * left;
            let len = step.length();
            if len < 1e-6 {
                break;
            }
            let dir = step / len;
            match level.shot_surface(&colliders, pos, dir, len + BALL_RADIUS) {
                Some((t, n)) if t - BALL_RADIUS < len => {
                    let go = (t - BALL_RADIUS).max(0.0);
                    pos += dir * go;
                    left *= 1.0 - go / len;
                    let n = if n.dot(dir) > 0.0 { -n } else { n };
                    let into = ball.velocity.dot(n);
                    let along = ball.velocity - n * into;
                    ball.velocity = along * BALL_GRIP - n * into * BALL_BOUNCE;
                }
                _ => {
                    pos += step;
                    break;
                }
            }
        }
        // (Rolling on the floor: it loses a little speed.)
        if level.ray(pos, Vec3::NEG_Y, BALL_RADIUS + 0.05).is_some() {
            let k = (1.0 - BALL_ROLL_DRAG * dt).max(0.0);
            ball.velocity.x *= k;
            ball.velocity.z *= k;
            if ball.velocity.y < 0.0 && ball.velocity.y > -1.0 {
                ball.velocity.y = 0.0;
            }
        }
        // Its turn as it rolls: about the axis across its way, as far as it went.
        let moved = pos - at;
        let axis = Vec3::Y.cross(moved.with_y(0.0));
        if axis.length_squared() > 1e-8 {
            let angle = moved.with_y(0.0).length() / BALL_RADIUS;
            transform.rotation =
                Quat::from_axis_angle(axis.normalize(), angle) * transform.rotation;
        }
        transform.translation = pos;
    }
}

/// A black hole (TnGravityAura): whose (a Jazz here), where it is, its age, the
/// enemies it has killed (their bodies drawn in), its effect, and how long it has
/// been over (it goes a second after).
#[derive(Component)]
pub(crate) struct BlackHole {
    owner: Option<Entity>,
    at: Vec3,
    age: f32,
    victims: Vec<Entity>,
    touched: Vec<Entity>,
    fx: Option<FxId>,
    over: Option<f32>,
    /// Its first sweep (the enemies it can see as it opens) is done.
    swept: bool,
}

fn spawn_black_hole(commands: &mut Commands, fx: &mut FxSystems, owner: Option<Entity>, at: Vec3) {
    let id = fx.spawn(FX_HOLE, at, Vec3::X, &[]);
    commands.spawn((
        BlackHole {
            owner,
            at,
            age: 0.0,
            victims: Vec::new(),
            touched: Vec::new(),
            fx: id,
            over: None,
            swept: false,
        },
        Transform::from_translation(at),
    ));
}

/// Each black hole: rising (under the ceiling), killing every lesser enemy in its
/// area as it reaches them (as it opens, those it can see; then any in reach) and
/// drawing their bodies in, its loop heard from it; and at its end, collapsing (its
/// end's effect and sound, the bodies near it flung out), gone a second on.
#[allow(clippy::too_many_arguments)]
fn black_holes(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<Level>,
    kinds: Res<enemies::Kinds>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut holes: Query<(Entity, &mut BlackHole, &mut Transform)>,
    mut enemies: Query<(Entity, &mut Enemy), Without<Player>>,
    mut rigs: Query<&mut Rig>,
) {
    let dt = time.delta_secs();
    for (entity, mut hole, mut transform) in &mut holes {
        let hole = &mut *hole;
        hole.age += dt;
        if let Some(over) = hole.over.as_mut() {
            *over += dt;
            if *over >= HOLE_GONE_SECS {
                commands.entity(entity).despawn();
            }
            continue;
        }
        if hole.age >= HOLE_SECS {
            // Explode: the loop's end, the effect's end, the bodies flung out.
            sfx.play_at(SND_HOLE_END, hole.at);
            if let Some(id) = hole.fx.take() {
                fx.stop(id);
            }
            fx.spawn(FX_HOLE_END, hole.at, Vec3::X, &[]);
            for &victim in &hole.victims {
                if let Ok(mut rig) = rigs.get_mut(victim)
                    && let RigState::Ragdoll(doll) = &mut rig.state
                    && doll.centre().distance(hole.at) <= HOLE_GIB_DISTANCE
                {
                    doll.explode(hole.at, HOLE_FLING, HOLE_FLING * 0.25);
                }
            }
            hole.over = Some(0.0);
            continue;
        }
        // It rises, staying 5 m under any ceiling.
        let up = level.ray(hole.at, Vec3::Y, HOLE_CEILING + HOLE_RISE * dt);
        hole.at.y += match up {
            Some(t) => (t - HOLE_CEILING).min(HOLE_RISE * dt),
            None => HOLE_RISE * dt,
        };
        transform.translation = hole.at;
        if let Some(id) = hole.fx {
            fx.place(id, hole.at, Vec3::X, Vec3::Y * HOLE_RISE);
        }
        sfx.hold(entity.to_bits(), SND_HOLE_LOOP, Some(hole.at));
        // Touch: each enemy in its cylinder (the first sweep needs a clear line).
        let first = !hole.swept;
        hole.swept = true;
        for (e, mut enemy) in &mut enemies {
            if !enemy.alive() || hole.touched.contains(&e) {
                continue;
            }
            let c = enemy.centre();
            let flat = (c - hole.at).with_y(0.0).length();
            if flat > HOLE_RADIUS || (c.y - hole.at.y).abs() > HOLE_RADIUS {
                continue;
            }
            if first && level.blocked(hole.at, c) {
                continue;
            }
            hole.touched.push(e);
            let spared = kinds
                .kinds
                .get(enemy.kind)
                .is_some_and(|k| HOLE_SPARED.contains(&k.ai_type.as_str()));
            if spared {
                continue;
            }
            // Killed by the gravity bomb, its killer the ball's owner.
            let kill =
                enemy.health / (enemy.damage_scale * enemy.mark.damage_scale()).max(1e-3) + 1.0;
            enemy.take(
                kill,
                hole.owner,
                crate::xp::Blow::Weapon,
                Hit::new(Death::Plain, c, hole.at - c).of_type("TnDamageTypeGravityBomb"),
            );
            hole.victims.push(e);
        }
        // The bodies drawn in: towards it (harder beyond its orbit while moving
        // away), up to it, and squeezed in at the end.
        let squish = hole.age >= HOLE_SECS - HOLE_SQUISH_SECS;
        let centre = hole.at;
        for &victim in &hole.victims {
            if let Ok(mut rig) = rigs.get_mut(victim)
                && let RigState::Ragdoll(doll) = &mut rig.state
            {
                doll.pull(dt, |p, v| {
                    let to = centre - p;
                    let d = to.length();
                    let dir = to / d.max(1e-3);
                    let away = v.dot(dir) < 0.0;
                    let mut a = dir
                        * HOLE_PULL_IN
                        * if d > HOLE_ORBIT && away {
                            HOLE_ORBIT_SCALE
                        } else {
                            1.0
                        };
                    if p.y < centre.y {
                        a += Vec3::Y * HOLE_PULL_UP;
                    }
                    if squish {
                        a += dir * HOLE_SQUISH;
                    }
                    // (Held near it rather than flung through: what is near slows.)
                    if d < HOLE_ORBIT * 2.0 {
                        a -= v * 8.0;
                    }
                    a
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A round at a barrier stops at its face, one over or wide of it flies on,
    /// and a blast's reach to it is to its nearest box.
    #[test]
    fn rounds_stop_at_a_barrier() {
        // Standing at the origin facing -Z, its right +X.
        let boxes = barrier_boxes(Vec3::ZERO, Vec3::NEG_Z, Vec3::X, 1.0);
        let shapes = vec![(Entity::PLACEHOLDER, boxes)];
        // From 20 m ahead of it, 2 m up: its middle box's face, 2.1 m ahead of its
        // foot (1.55 m out, 1.1 m thick), facing the round.
        let from = Vec3::new(0.0, 2.0, -20.0);
        let (t, n, _) = barrier_hit(&shapes, from, Vec3::Z, 30.0).unwrap();
        assert!((t - 17.9).abs() < 0.01, "{t}");
        assert!((n - Vec3::NEG_Z).length() < 1e-3, "{n}");
        // Over it (it stands 8.7 m at most), wide of it, or short of it.
        assert!(barrier_hit(&shapes, Vec3::new(0.0, 12.0, -20.0), Vec3::Z, 30.0).is_none());
        assert!(barrier_hit(&shapes, Vec3::new(12.0, 2.0, -20.0), Vec3::Z, 30.0).is_none());
        assert!(barrier_hit(&shapes, from, Vec3::Z, 10.0).is_none());
        // Its ends curve back towards its owner (the side boxes 2.5 m behind its
        // foot, 7 m out to either side).
        assert!(barrier_hit(&shapes, Vec3::new(7.0, 2.0, -20.0), Vec3::Z, 30.0).is_some());
        // A blast 3 m before its face.
        assert!((barrier_reach(&boxes, Vec3::new(0.0, 2.0, -5.1)) - 3.0).abs() < 0.01);
    }
}
