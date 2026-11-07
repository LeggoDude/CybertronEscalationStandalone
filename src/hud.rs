//! Fall of Cybertron's HUD, played from the game's own HUD movie.
//!
//! The game's HUD is a Scaleform (Flash) movie, UI_GFxHud_p.Hud_GFX: clips with
//! timelines, bitmaps, bar morphs and text in the game's fonts, run by ActionScript
//! that reacts to the game. tools/build_hud.py exports the parts the demo uses to
//! assets/hud (hud.txt, the images and glyph atlases). This module plays those clips
//! the way Flash does (30 frames a second, frame labels, stop frames, colour
//! transforms) and does what the movie's scripts do, fed from `HudState`.
//!
//! The HUD is drawn in 3D like the game's: hud_mc has a 45-degree perspective, its
//! corner groups are turned a few degrees towards the player (the "screen warp",
//! stronger while aiming and in the car) and the damage indicators lie on a ring
//! tilted back by 50 degrees. It has its own camera (render layer `HUD_LAYER`)
//! drawing over the game, and a small material for Flash's colour transforms
//! (assets/shaders/hud.wgsl).
//!
//! Without assets/hud the demo runs without a HUD.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::mesh::{Indices, MeshTag, MeshVertexBufferLayoutRef};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    AsBindGroup, BufferSize, BufferUsages, PrimitiveTopology, RenderPipelineDescriptor,
    SpecializedMeshPipelineError,
};
use bevy::render::renderer::RenderQueue;
use bevy::render::storage::{GpuShaderStorageBuffer, ShaderStorageBuffer};
use bevy::render::view::Hdr;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy::shader::ShaderRef;
use bevy::window::PrimaryWindow;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::f32::consts::TAU;
use std::path::{Path, PathBuf};

/// Render layer of the HUD and its camera (split screen's guest's HUD has its own,
/// GUEST_HUD_LAYER, and a camera drawing it into his part of the window).
pub const HUD_LAYER: usize = 1;
pub const GUEST_HUD_LAYER: usize = 10;

/// The render layer of seat `seat`'s HUD (split.rs).
fn hud_layer(seat: usize) -> usize {
    if seat == 1 {
        GUEST_HUD_LAYER
    } else {
        HUD_LAYER
    }
}
/// The movie's stage height: one HUD unit is a pixel of a 720-line screen.
const STAGE_H: f32 = 720.0;
/// The movie's frame rate.
const FPS: f32 = 30.0;
/// The least step between two pieces' draw orders (added to the distance the
/// transparent pass sorts by): more than the tilt puts between pieces that overlap.
const ORDER_STEP: i64 = 100;
/// The step between pieces numbered afresh: room for clips attached later
/// between them without renumbering the rest (see `draw_orders`).
const ORDER_SPACING: i64 = ORDER_STEP * 16;
/// Orders stay whole numbers below this, exact as the material's f32.
const ORDER_LIMIT: i64 = 1 << 24;
/// Depth for clips attached at run time (above the timelines' own).
const ATTACH: i32 = 100_000;

// ---------------------------------------------------------------------------
// What the game tells the HUD
// ---------------------------------------------------------------------------

/// A weapon as the HUD shows it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct HudWeapon {
    /// Identifies the weapon: a change redraws the crosshair, icon and ammo bar.
    pub id: u32,
    /// Name shown when it is drawn ("" for none).
    pub label: &'static str,
    /// Its ItemWireframe icon (an image in hud.txt), if it has one.
    pub icon: Option<&'static str>,
    /// Its reticle (a symbol in hud.txt: mc_crosshairRifle, mc_crosshairTactical,
    /// mc_crosshairShotgun, mc_crosshairPistol, mc_crosshairCannon,
    /// mc_crosshairRocket, mc_crosshairChainBeam, mc_crosshairMortar,
    /// mc_crosshairSawblade), or "" for none
    /// (the game's ShowCrosshair false: the Nucleon Charge Rifle shows none, from the
    /// hip or scoped).
    pub crosshair: &'static str,
}

/// The scope fine aim brings up (scopeManager_mc: updateWeaponScope picks it by the
/// weapon's class).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Scope {
    #[default]
    None,
    /// mc_tacticalScope, the Photon Burst Rifle's: range finder and compass.
    Tactical,
    /// mc_sniperScope, the Nucleon Charge Rifle's: its charge.
    Sniper,
}

/// The shotgun reticle (the Scatter Blaster's), whose arcs widen with the spread,
/// and the width of its arc image (Hud_GFX_IA5's quad, in movie pixels).
const SHOTGUN_CROSSHAIR: &str = "mc_crosshairShotgun";
const SHOTGUN_ARC_WIDTH: f32 = 28.0;
/// The pistol reticle (the Path Blaster's): the rifle's three prongs, pushed out
/// less far by the spread.
const PISTOL_CROSSHAIR: &str = "mc_crosshairPistol";
/// The cannon reticle (the Riot Cannon's): three arcs, pushed out as the pistol's.
const CANNON_CROSSHAIR: &str = "mc_crosshairCannon";
/// The mortar reticle (the Corrosive Slime Cannon's, and the Chaos-Rift
/// Combustor's): two prongs pushed out sideways (SpreadMultiplier 300) and its two
/// side lines a fifth as far; and the saw-blade one (the Gear Shredder's): two
/// prongs pushed out sideways (400) and stretched as they go. Both come in with
/// their own timeline (their first ten frames) rather than spinning in.
const MORTAR_CROSSHAIR: &str = "mc_crosshairMortar";
const SAWBLADE_CROSSHAIR: &str = "mc_crosshairSawblade";

/// The share of a clip at or under which the clip is low (LowClipValue).
const LOW_CLIP: f32 = 0.25;

/// Low ammo, as the HUD movie's updateAmmoDisplay and updateAmmoWarnings have it:
/// no more than a clip's worth left in all, clip and reserve together (a gun with
/// no reserve: under a quarter of a clip).
fn low_ammo(clip: u32, reserve: u32, clip_size: u32, reserve_max: u32) -> bool {
    let total = clip + reserve;
    if reserve_max > 0 {
        total <= clip_size
    } else {
        (total as f32 / clip_size.max(1) as f32) < LOW_CLIP
    }
}

pub enum HudEvent {
    /// Jazz was hit from `yaw` (world heading towards the attacker, radians:
    /// 0 = -Z, pi/2 = +X) for `amount` (0..1 of his health).
    Damaged { yaw: f32, amount: f32 },
    /// One of Jazz's shots hit something that takes damage.
    Hit,
    /// A reload started, taking this many seconds.
    Reload(f32),
    /// Ammo picked up for a weapon (listed over the ammo count, with the weapon's
    /// small icon).
    AmmoAdded {
        icon: Option<&'static str>,
        amount: u32,
        current: bool,
    },
    /// A game announcement in the big line at the top (TnHUD.LocalizedGameAnnouncement,
    /// e.g. the game type as a match starts): typed out, held, then erased.
    GameAnnouncement(&'static str),
    /// A reward announcement in the line under it (HudData's rewardAnnouncement:
    /// topCenter_mc's sprite 893, reward_mc), such as Escalation's "Wave 1 Failed,
    /// Restarting Wave": typed out twice as fast, held, then erased.
    RewardAnnouncement(String),
    /// An award's experience as a point transaction over the crosshair
    /// (TnHudDataObserverXpTransactions' pointEvent): its amount, its announcement
    /// and description (either may be empty), and its transaction (a kill's awards
    /// share one: their amounts add up in its "+N").
    Points {
        amount: u32,
        announcement: String,
        description: String,
        transaction: i32,
    },
    /// A level reached (TnHUD.NotifyLevelUp): the level and the class it is for (an
    /// index into customize::SPECIALTIES).
    LevelUp { level: u32, class: usize },
    /// A support item bought at an equipment station (TnEquipmentPickup.ClientGivenTo:
    /// TnHUD.NotifyStoreItemUnlocked, the movie's storeItemUnlocked): its picture
    /// (ItemWireframe<...>, sprite 714's filterItemId of its UniqueId) and its name
    /// (its FriendlyName, sprite 722's), shown in mc_itemUnlocked at the top right.
    ItemUnlocked {
        image: &'static str,
        name: &'static str,
    },
    /// A line of the game's messages at the top left (TnHUD.ColoredLocalizedMessage,
    /// the movie's gameMessage: "Thermo Mine Deployed"): its parts, each its text and
    /// the colour its TextColor's alpha numbers (TnMessageTextColors: 0 the player's,
    /// 1 a friend's, 2 an enemy's, 3 neutral; see message_color).
    GameMessage(Vec<(u8, String)>),
    /// Something bought with energon touched or tried (TnInteractActionHudCurrency-
    /// Notify: the movie's NotifyInteractCurrency, recreator.rs's pods): the energon
    /// counter pops, flashing red when there isn't enough (`low`).
    CurrencyNotify { low: bool },
}

/// The game's state as the HUD needs it, written each frame by the game.
#[derive(Resource)]
pub struct HudState {
    pub health: f32,
    pub max_health: f32,
    /// The shield over the health (shieldBar_mc, shown when max_shield > 0).
    pub shield: f32,
    pub max_shield: f32,
    pub weapon: HudWeapon,
    pub clip: u32,
    pub clip_size: u32,
    /// Ammo left outside the clip, and the most it holds (WeaponReserveCapacity).
    pub reserve: u32,
    pub reserve_max: u32,
    /// Spread of the current weapon (the game's WeaponSpread).
    pub spread: f32,
    /// The camera's shake this frame: pitch, yaw and roll in degrees (the game's
    /// HudRelevantShakeThisFrame, its cameraShake event).
    pub camera_shake: Vec3,
    pub fine_aim: bool,
    /// Fine aiming with a scoped weapon: its scope is up.
    pub scope: Scope,
    /// The Nucleon Charge Rifle's charge, 0..1 (TnWeaponSniperRifle's
    /// GetNormalizedChargeLevel: 0 while it fires), shown by its scope.
    pub charge: f32,
    /// Metres to what the crosshair is on (the scope's range finder), < 0 for nothing.
    pub target_distance: f32,
    /// What the crosshair is on as the tactical scope's target panel shows it (the
    /// Photon Burst Rifle's Cyber Tactical Scope upgrade: TnHudDataObserverTargetName
    /// and TargetHealthNormalized): its name and its health (0..1, < 0 for none).
    pub target_name: String,
    pub target_health: f32,
    /// The round of the clip that is the special one (the ammo bar's segment that
    /// flashes: SpecialShotInClip, TnHudDataObserverSpecialShotInClip), if any.
    pub special_shot: Option<u32>,
    /// The Nucleon Charge Rifle's charge upgrade as its scope's three lights show it
    /// (SniperChargeUpgrade, TnHudDataObserverSniperRifleChargeUpgrade): None
    /// without Deadeye, else how many are lit (3 while it is charging faster, else 0).
    pub sniper_upgrade: Option<u8>,
    /// Something to pick up or buy, shown on the game's contextual command line
    /// ("Hold E to pickup <name>", "Hold E to purchase <name> for *<price>") with
    /// the key Interact/Pick Up is bound to (and whether it is a mouse button or the
    /// wheel).
    pub prompt: Option<Prompt>,
    pub pickup_key: String,
    pub pickup_key_mouse: bool,
    /// A jet's sticky rocket stuck in him (TnContextualCommandFlyerRocket: shown
    /// over any other command), with the key he changes form with (Change Form) and
    /// whether it is a mouse button or the wheel.
    pub sticky_rocket: bool,
    pub transform_key: String,
    pub transform_key_mouse: bool,
    /// The keys the offensive and defensive equipment are on (Assault and Utility
    /// T.E.C.H.: the purchase line's <<OffensiveEquipment>> and
    /// <<DefensiveEquipment>>), and whether each is a mouse button or the wheel.
    pub equipment_keys: [(String, bool); 2],
    /// A controller plays (pad.rs): the HUD shows its buttons (the equipment slots'
    /// D-pad arrows, the ability's RB, its lines' buttons).
    pub pad: bool,
    pub vehicle: bool,
    /// The crosshair is over an enemy.
    pub on_target: bool,
    pub reloading: bool,
    /// The character's ability, if he has one the demo plays (the robot's:
    /// robotAbility_mc, shown in robot form).
    pub ability: Option<HudAbility>,
    /// The support items bought, as the equipment slots under the ammo count show
    /// them: the offensive one (offensive_mc, on the Assault T.E.C.H. key) and the
    /// defensive one (defensive_mc, on the Utility T.E.C.H. key).
    pub equipment: [Option<HudEquipment>; 2],
    pub energon: u32,
    /// Heading the camera faces (radians, as `HudEvent::Damaged`).
    pub view_yaw: f32,
    pub dead: bool,
    /// Down in Escalation (src/revive.rs): his downed health as its meter shows it,
    /// 1 full and running to 0 (HudData's NormalizedDownedHealth: bottomCenter_mc's
    /// sprite 943), or None when he isn't down. Down, the crosshair, health and ammo
    /// corners are hidden as they are dead (updateHudVisibility's isDowned).
    pub downed: Option<f32>,
    /// The team's lives left, 0 to 3 (bottomLeft_mc's sprite 799: lifeMarkers_mc's
    /// emblems, those lost in red).
    pub lives: u8,
    /// Escalation's wave (src/waves.rs) as topRight_mc's sprite 812 shows it
    /// (updateCurrentWave: "WAVE 1 OF 15"): the GRI's _CurrentWave, none shown for 0
    /// (before the first) or past WAVES_SHOWN.
    pub wave: u32,
    /// The countdown to the next wave on sprite 816's clock (the single-team game's
    /// scoreboard, mc_scoreboardSingleTeam): whether it shows (IsCountingDown) and
    /// its seconds (CurrentCountdown).
    pub counting: bool,
    pub countdown: i32,
    /// Another player is down (topCenter_mc's sprite 900: "An Ally is Downed", "Find
    /// and Revive your Ally", while there is one).
    pub ally_downed: bool,
    /// The revive this player is giving (bottomCenter_mc's returnFlagNotifyTime_mc:
    /// NotifyProgressBarChanged's "Reviving <name>" over its bar): the downed one's
    /// name and how far it has come, 0 to 1.
    pub revive_bar: Option<(String, f32)>,
    /// Dead and out till the team's round ends, past his death's view: the respawn
    /// screen over the HUD (MultiplayerRespawn_GFX: "Waiting for Next Wave to
    /// Respawn"), with the teammate he watches (his name: "Currently Spectating"),
    /// or none to watch.
    pub spectating: Option<Option<String>>,
    /// The whole HUD is hidden (the game's pause menu is up).
    pub hidden: bool,
    /// Big centred message (empty for none) and the smaller lines under it.
    pub announcement: String,
    pub hint: String,
    pub events: Vec<HudEvent>,
    /// The other players in the match, each with a marker over him.
    pub allies: Vec<HudAlly>,
    /// The sentries the Orbital Pistol has revealed, each with a marker over it.
    pub revealed: Vec<HudRevealed>,
    /// A homing weapon's lock, its marker on the sentry it is for (a jet's rockets',
    /// the Thermo Rocket Cannon's: TnObjectiveMarkerTypeLockOn).
    pub lock_on: Option<HudLockOn>,
    /// The enemies' grenades near him, each with its marker
    /// (TnObjectiveMarkerTypeGrenade: Escalation's soldiers' frag grenades).
    pub grenades: Vec<HudGrenade>,
    /// The safe frame (Settings > Display > Safe Frame: the share of the screen's
    /// width and height left round the HUD), which markers kept on the screen keep
    /// out of (markers_mc's ScreenClampXMin and so on).
    pub safe_frame: Vec2,
    /// The weapon in hand has rounds out to set off (the A-4 Pulsar Cannon's:
    /// HudData's ActiveGrenadeCount over 0) and a bomb in the air to burst (the
    /// Chaos-Rift Combustor's: CanClusterBomb), each prompted under the crosshair.
    pub grenades_out: bool,
    pub can_cluster_bomb: bool,
    /// Team Coloring Mode on Autobot vs Decepticon (menu.rs's DisplaySettings), and
    /// whether the players are Decepticons on this map: the message parts' colours.
    pub faction_colors: bool,
    pub decepticons: bool,
    /// The Southpaw keyboard layout is in use (the game's KbdControlSchemeID 84):
    /// the prompts' mouse buttons swap.
    pub southpaw: bool,
}

/// A homing weapon's lock as its marker (the movie's mc_markerLockOn) shows it: on
/// the sentry from the moment it is picked, until the lock is let go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudLockOn {
    /// Which sentry (its entity's bits): a new one starts the marker over.
    pub id: u64,
    /// Where it is (its middle) in normalized device coordinates, or None behind
    /// the camera (lock-on markers aren't kept on the screen).
    pub at: Option<Vec2>,
    /// How far the lock has come, 0 to 1 (GetNormalizedLockOnTime: the marker's
    /// special argument).
    pub progress: f32,
}

/// An enemy's grenade as its marker (the movie's mc_markerGrenade) shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudGrenade {
    /// Which grenade (its entity's bits): a new one gets a new marker.
    pub id: u64,
    /// Where it is on the screen with the marker's lift (MarkerSSOffset: a
    /// twentieth of the screen's height up), as Canvas.Project gives it: in
    /// normalized device coordinates (x right, y up, -1..1 across the screen), as
    /// projected whether in front of the camera or not (behind it, the point
    /// comes out turned through the screen's middle), and which it is.
    pub at: Vec2,
    pub in_front: bool,
}

/// An ability as robotAbility_mc shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct HudAbility {
    /// Its icon (an image in hud.txt; the movie's AbilityIcon<name>).
    pub icon: &'static str,
    /// Its meter (the resource, 0..1 of full) and what a use takes of it (0..1:
    /// HudData's AbilityType0Cost).
    pub meter: f32,
    pub cost: f32,
    /// Being used (TnAbility's cooldown running backwards).
    pub in_use: bool,
    /// The key it is on (Console.GetKeyForAbility0).
    pub key: String,
    /// Its cancel icon shows (cancel_mc: the Cloak's and the Barrier's while they
    /// are going, updateShouldShowCancelIcon).
    pub cancel: bool,
    /// Going (HudData's abilityActive: the Hover's icon turns its arrow down).
    pub active: bool,
    /// The Heal Beam's juice, 0 to 1 (HealBeamJuice: its icon's progress_mc).
    pub juice: f32,
    /// Jammed (TnBuffAbilityJammed on him: updateAbilityJam's AbilityJam): amber,
    /// and JAMMED pulsing under it (jammed_mc).
    pub jammed: bool,
}

/// A support item as its equipment slot shows it (offensive_mc or defensive_mc:
/// updateEquipment, updateEquipmentState).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudEquipment {
    /// Its icon (an image in hud.txt: AbilityIcon<name>, from its ability's class).
    pub icon: &'static str,
    /// Ready for a use: the slot in its own colours, its icon full size. Else (in
    /// use, or not recharged enough for a use) it is red, its icon at 90%.
    pub ready: bool,
}

/// What the contextual command line offers.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Prompt {
    /// A weapon on the ground (ContextualCommands.PickupWeapon): its name.
    Pickup(&'static str),
    /// Something for sale, such as health at an Escalation health station
    /// (ContextualCommands.PickupPurchasable): its name and price in energon.
    Purchase(&'static str, u32),
    /// A downed teammate in reach (ContextualCommands.Revive: "Hold <<UseInteract>>
    /// to Revive").
    Revive,
    /// An Armory Recreator in reach (ContextualCommands.LootBox: "Press
    /// <<UseInteract>> to Activate Armory Recreator *`c", recreator.rs): its price.
    Recreator(u32),
    /// An unlock console in reach (its ContextualCommands line, doors.rs: "Press
    /// <<UseInteract>> to deposit *`i of *`c to open door"): the line, the deposit
    /// and the price.
    Deposit(&'static str, u32, u32),
}

/// Another player as his marker (the movie's mc_markerPawn) shows him: his name
/// over a bar of his health.
#[derive(Clone, Debug, PartialEq)]
pub struct HudAlly {
    /// Who he is (his player slot).
    pub id: usize,
    pub name: String,
    /// His health, 0..1.
    pub health: f32,
    /// His shield, 0..1 (None for a player without one).
    pub shield: Option<f32>,
    /// Down (src/revive.rs): his downed health, 1 full to 0 (the marker's
    /// SA_NormalizedDownedHealth: its health bar shows it, and its downed icon
    /// strobes), or None.
    pub downed: Option<f32>,
    /// Where the marker goes, over his head: in normalized device coordinates
    /// (x right, y up, -1..1 across the screen), or None when it can't be seen
    /// (behind the camera, or he is down).
    pub at: Option<Vec2>,
}

/// A sentry revealed by the Orbital Pistol (orbital.rs), as its marker (the movie's
/// mc_markerPawn for an enemy) shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct HudRevealed {
    /// Which sentry (its entity's bits).
    pub id: u64,
    /// Its mark (1 to 3), whose icon the marker shows (0: none).
    pub mark: u8,
    /// Where the marker goes, over it, in normalized device coordinates, or None
    /// when it can't be (behind the camera).
    pub at: Option<Vec2>,
}

impl Default for HudState {
    fn default() -> Self {
        HudState {
            health: 100.0,
            max_health: 100.0,
            shield: 0.0,
            max_shield: 0.0,
            weapon: HudWeapon {
                id: u32::MAX,
                label: "",
                icon: None,
                crosshair: "mc_crosshairRifle",
            },
            clip: 0,
            clip_size: 1,
            reserve: 0,
            reserve_max: 0,
            spread: 0.0,
            camera_shake: Vec3::ZERO,
            fine_aim: false,
            scope: Scope::None,
            charge: 0.0,
            target_distance: -1.0,
            target_name: String::new(),
            target_health: -1.0,
            special_shot: None,
            sniper_upgrade: None,
            prompt: None,
            pickup_key: "E".to_string(),
            pickup_key_mouse: false,
            sticky_rocket: false,
            transform_key: "Q".to_string(),
            transform_key_mouse: false,
            equipment_keys: [("1".to_string(), false), ("2".to_string(), false)],
            pad: false,
            vehicle: false,
            on_target: false,
            reloading: false,
            ability: None,
            equipment: [None, None],
            energon: 0,
            view_yaw: 0.0,
            dead: false,
            downed: None,
            lives: 3,
            wave: 0,
            counting: false,
            countdown: 0,
            ally_downed: false,
            revive_bar: None,
            spectating: None,
            hidden: false,
            announcement: String::new(),
            hint: String::new(),
            events: Vec::new(),
            allies: Vec::new(),
            revealed: Vec::new(),
            lock_on: None,
            grenades: Vec::new(),
            safe_frame: Vec2::ZERO,
            grenades_out: false,
            can_cluster_bomb: false,
            faction_colors: false,
            decepticons: false,
            southpaw: false,
        }
    }
}

/// The HUD's systems; the game writes `HudState` before them.
#[derive(SystemSet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct HudSystems;

pub struct HudPlugin {
    /// The assets folder (assets/hud is read from it).
    pub assets: PathBuf,
}

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<HudMaterial>::default())
            .init_resource::<HudState>()
            .init_resource::<HudIndex>()
            .init_resource::<HudStage>()
            .init_resource::<HudColors>()
            .init_resource::<HudPass>()
            .init_resource::<GuestStores>()
            .insert_resource(HudDir(self.assets.clone()))
            .add_systems(Startup, (make_hud_colors, setup_hud))
            // The first player's HUD, then split screen's guest's (his stores
            // swapped in for it: separate copies of the systems, each with its own
            // state); the clips' timelines step once for both.
            .add_systems(
                Update,
                (
                    guest_hud,
                    hud_logic,
                    step_clips,
                    layout_hud,
                    show_hud,
                    (
                        guest_pass_in,
                        hud_logic,
                        layout_hud,
                        show_hud,
                        guest_pass_out,
                    )
                        .chain()
                        .run_if(guest_hud_up),
                )
                    .chain()
                    .in_set(HudSystems),
            )
            .add_systems(
                PostUpdate,
                place_hud_views.before(bevy::camera::CameraUpdateSystems),
            );
        // The pieces' colours, copied to the render world and written to their
        // buffer as they change (HudColors).
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<HudColorsRender>()
            .add_systems(ExtractSchedule, extract_hud_colors)
            .add_systems(
                Render,
                write_hud_colors.in_set(RenderSystems::PrepareResources),
            );
    }
}

#[derive(Resource)]
struct HudDir(PathBuf);

// ---------------------------------------------------------------------------
// Material: a texture through a Flash colour transform
// ---------------------------------------------------------------------------

/// Draws a texture (white without one) through a Flash colour transform,
/// colour * mul + add, worked out in sRGB as Flash does: its piece's, from the
/// HUD's colours by its mesh tag (HudColors). The output is premultiplied;
/// additive layers (blendMode "add") keep what is behind them.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct HudMaterial {
    /// x: 1 for additive blending.
    #[uniform(2)]
    params: Vec4,
    #[texture(3)]
    #[sampler(4)]
    texture: Option<Handle<Image>>,
    /// The pieces' colour transforms (HudColors): always HUD_COLORS_BUFFER.
    #[storage(5, read_only, visibility(fragment))]
    colors: Handle<ShaderStorageBuffer>,
    /// Draw order (see `depth_bias`).
    order: f32,
}

impl Material for HudMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/hud.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    /// Added to the distance the transparent pass sorts by: the HUD draws in its
    /// display-list order whatever the 3D tilt does to the distances.
    fn depth_bias(&self) -> f32 {
        self.order
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    /// No back-face culling: the HUD is mirrored to use Flash's y-down coordinates.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The pieces' colours
// ---------------------------------------------------------------------------

/// The HUD's colours' buffer (HudColors): one, fixed, so every HUD material can
/// bind it.
const HUD_COLORS_BUFFER: Handle<ShaderStorageBuffer> =
    bevy::asset::uuid_handle!("8d2f4a61-3c7e-4b09-a5d8-1e6f0b9c2d47");

/// The most pieces the HUD's colours have room for (HUD_COLORS_BUFFER, 32 bytes
/// each: 256 KB). The movie's HUD has a few hundred; one past these would be drawn
/// as its texture is, without its colour transform.
const HUD_COLORS_MAX: usize = 8192;

/// A piece's colour transform when it has none set yet: none.
const NO_COLORS: [f32; 8] = [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];

/// The HUD pieces' colour transforms (Flash's: colour * mul + add), each piece's
/// mul then add by its slot (its mesh tag: HudSlot), which the HUD's shader reads
/// (from HUD_COLORS_BUFFER). layout_hud sets them each frame, and the render world
/// writes them to the buffer as they change. They were the pieces' materials': a
/// material changed is one prepared again for the GPU (a buffer made anew for each
/// of its values, and its bind group), and a fight's HUD changes a dozen or so of
/// its pieces' colours a frame (its fades and flashes).
#[derive(Resource, Default)]
pub struct HudColors {
    colors: Vec<[f32; 8]>,
    /// Slots given back, to give out again.
    free: Vec<u32>,
}

impl HudColors {
    /// A slot for a new piece, its colours none.
    fn take(&mut self) -> u32 {
        let slot = self.free.pop().unwrap_or_else(|| {
            self.colors.push(NO_COLORS);
            (self.colors.len() - 1) as u32
        });
        self.colors[slot as usize] = NO_COLORS;
        if slot as usize == HUD_COLORS_MAX {
            warn!("The HUD has more than {HUD_COLORS_MAX} pieces: those past them aren't coloured");
        }
        slot
    }

    /// Sets `slot`'s colour transform; true if it changed.
    fn set(&mut self, slot: u32, mul: Vec4, add: Vec4) -> bool {
        let Some(c) = self.colors.get_mut(slot as usize) else {
            return false;
        };
        let new = [mul.x, mul.y, mul.z, mul.w, add.x, add.y, add.z, add.w];
        if *c == new {
            return false;
        }
        *c = new;
        true
    }
}

/// A piece's slot among the HUD's colours (HudColors): its mesh tag, given as it
/// is spawned and given back as it goes.
#[derive(Component)]
#[component(on_add = take_hud_slot, on_remove = give_back_hud_slot)]
struct HudSlot;

fn take_hud_slot(mut world: DeferredWorld, context: HookContext) {
    // (Marked changed: the slot's colours, none, are written over its last piece's
    // even if the new piece's stay none.)
    let Some(slot) = world.get_resource_mut::<HudColors>().map(|mut c| c.take()) else {
        return;
    };
    if let Some(mut tag) = world.get_mut::<MeshTag>(context.entity) {
        tag.0 = slot;
    }
}

fn give_back_hud_slot(mut world: DeferredWorld, context: HookContext) {
    let Some(slot) = world.get::<MeshTag>(context.entity).map(|t| t.0) else {
        return;
    };
    if let Some(mut colors) = world.get_resource_mut::<HudColors>() {
        colors.bypass_change_detection().free.push(slot);
    }
}

/// The render world's copy of the HUD's colours, to write (HudColors).
#[derive(Resource, Default)]
struct HudColorsRender {
    colors: Vec<[f32; 8]>,
    /// Not written yet.
    fresh: bool,
}

/// Makes the HUD's colours' buffer.
fn make_hud_colors(storage: Option<ResMut<Assets<ShaderStorageBuffer>>>) {
    let Some(mut storage) = storage else {
        return;
    };
    let mut buffer =
        ShaderStorageBuffer::with_size(HUD_COLORS_MAX * 32, RenderAssetUsages::default());
    buffer.buffer_description.label = Some("hud_colors");
    // (Written as they change: write_hud_colors.)
    buffer.buffer_description.usage |= BufferUsages::COPY_DST;
    let _ = storage.insert(HUD_COLORS_BUFFER.id(), buffer);
}

/// Copies the HUD's colours to the render world when they have changed.
fn extract_hud_colors(colors: Extract<Res<HudColors>>, mut render: ResMut<HudColorsRender>) {
    if colors.is_changed() {
        render.colors.clone_from(&colors.colors);
        render.fresh = true;
    }
}

/// Writes the HUD's colours to their buffer (as many as it holds), once it is made.
fn write_hud_colors(
    mut colors: ResMut<HudColorsRender>,
    buffers: Res<RenderAssets<GpuShaderStorageBuffer>>,
    queue: Res<RenderQueue>,
) {
    if !colors.fresh {
        return;
    }
    let Some(gpu) = buffers.get(HUD_COLORS_BUFFER.id()) else {
        return;
    };
    colors.fresh = false;
    let n = colors.colors.len().min(gpu.buffer.size() as usize / 32);
    let Some(size) = BufferSize::new(n as u64 * 32) else {
        return;
    };
    let Some(mut view) = queue.write_buffer_with(&gpu.buffer, 0, size) else {
        return;
    };
    // (Written only: reading the view is slow, and logs a warning each time.)
    let bytes: &mut [u8] = view.as_mut();
    for (to, value) in bytes
        .chunks_exact_mut(4)
        .zip(colors.colors.iter().flatten())
    {
        to.copy_from_slice(&value.to_le_bytes());
    }
}

// ---------------------------------------------------------------------------
// The exported movie (hud.txt)
// ---------------------------------------------------------------------------

/// A Flash colour transform: out = in * mul + add (add in 0..1).
#[derive(Clone, Copy, PartialEq, Debug)]
struct Cx {
    mul: [f32; 4],
    add: [f32; 4],
}

impl Default for Cx {
    fn default() -> Self {
        Cx::IDENT
    }
}

impl Cx {
    const IDENT: Cx = Cx {
        mul: [1.0; 4],
        add: [0.0; 4],
    };

    /// From Flash's ColorTransform values (offsets in 0..255).
    const fn new(mul: [f32; 4], add: [f32; 4]) -> Cx {
        Cx {
            mul,
            add: [
                add[0] / 255.0,
                add[1] / 255.0,
                add[2] / 255.0,
                add[3] / 255.0,
            ],
        }
    }

    /// This transform, then `parent`'s (a child inside a parent clip).
    fn then(self, parent: Cx) -> Cx {
        let mut out = Cx::IDENT;
        for i in 0..4 {
            out.mul[i] = self.mul[i] * parent.mul[i];
            out.add[i] = self.add[i] * parent.mul[i] + parent.add[i];
        }
        out
    }

    fn lerp(self, to: Cx, t: f32) -> Cx {
        let mut out = self;
        for i in 0..4 {
            out.mul[i] += (to.mul[i] - self.mul[i]) * t;
            out.add[i] += (to.add[i] - self.add[i]) * t;
        }
        out
    }
}

// The colour transforms the movie's scripts use.
const NORMAL: Cx = Cx::IDENT;
/// Bars and the weapon name when health or ammo runs low.
const RED: Cx = Cx::new([1.0, 1.0, 1.0, 1.0], [255.0, -80.0, -80.0, 0.0]);
/// The flash when a bar changes.
const WHITE: Cx = Cx::new([0.0, 0.0, 0.0, 1.0], [255.0, 255.0, 255.0, 0.0]);
/// The energon counter's pop (revealMoney: tweenTo r2 seconds, Regular.easeOut): as
/// it changes or a purchase is in reach, white over half a second; with too little
/// for it, RedColor over five.
const MONEY_POP_SECS: f32 = 0.5;
/// The countdown's clock (mc_scoreboardSingleTeam's sprite 229, at x -86 in it): its
/// script shows it sliding in from 10 to the right as it fades in, and out the
/// same way (updateShowCountdown, 0.4 s, Regular.easeOut); from 10 s down each new
/// second flashes it red (RedColor) easing back to NormalColor, at 75%
/// (updateCountdownTime, 0.4 s, Regular.easeInOut).
const WAVE_CLOCK_X: f32 = -86.0;
const WAVE_CLOCK_SLIDE: f32 = 10.0;
const WAVE_CLOCK_SECS: f32 = 0.4;
const WAVE_CLOCK_FLASH_FROM: i32 = 10;
const WAVE_CLOCK_RED: [f32; 3] = [1.0, 0.6, 0.6];
const WAVE_CLOCK_ALPHA: f32 = 0.75;
/// The wave's line ($UIText.Shared.Wave, upper case: `r the wave, `t the waves),
/// typed in at 0.1 s a letter (mc_currentWaveMessage's tweenFrom {text: '_'}).
const WAVE_TEXT: &str = "Wave `r of `t";
const WAVE_TYPING: f32 = 0.1;
const MONEY_LOW_SECS: f32 = 5.0;
const MONEY_RED: Cx = Cx::new([1.0, 0.2, 0.2, 1.0], [100.0, 0.0, 0.0, 0.0]);
/// Crosshair over an enemy.
const CROSSHAIR_RED: Cx = Cx::new([1.0, 0.25, 0.3, 1.0], [255.0, 0.0, 0.0, 0.0]);
/// Crosshair while reloading (BlueColor at 15% alpha).
const CROSSHAIR_BLUE: Cx = Cx::new([0.0, 1.0, 1.0, 0.15], [0.0; 4]);
/// The ammo bar's segments shade from white to this.
const LIGHT_BLUE: Cx = Cx::new([0.5, 1.0, 0.9, 0.8], [0.0; 4]);
/// The ability jammed (robotAbility_mc's AmberColor).
const AMBER: Cx = Cx::new([1.0, 0.8, 0.0, 0.8], [255.0, 0.0, 0.0, 0.0]);
/// Ammo picked up for a weapon not in hand.
const PICKUP_BLUE: Cx = Cx::new([0.0, 1.0, 0.83, 1.0], [0.0; 4]);
/// A friendly player's marker: his name (HudData's FriendlyColor, #00FFD3).
const FRIENDLY: Cx = Cx::new([0.0, 1.0, 0.82, 1.0], [0.0; 4]);
/// An enemy's marker (HudData's EnemyColor, #FF8787), and its buffs' icons
/// (updateBuffs: CurrentColor, for buffs not in the faction's colour).
const ENEMY: Cx = Cx::new([1.0, 0.53, 0.53, 1.0], [0.0; 4]);

#[derive(Clone, Debug)]
struct Key {
    child: u32,
    name: String,
    m: [f32; 6],
    cx: Cx,
    ratio: f32,
    additive: bool,
}

impl Key {
    fn placed(child: u32, name: &str) -> Key {
        Key {
            child,
            name: name.to_string(),
            m: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            cx: Cx::IDENT,
            ratio: 0.0,
            additive: false,
        }
    }
}

struct SpriteDef {
    frames: usize,
    labels: HashMap<String, usize>,
    stops: Vec<bool>,
    gotos: HashMap<usize, String>,
    /// Per depth, the keys in frame order (None = nothing there from that frame).
    tracks: BTreeMap<i32, Vec<(usize, Option<Key>)>>,
    /// Masks: (mask depth, clip depth, rectangle x0 y0 x1 y1). The depths above the
    /// mask, up to the clip depth, show only inside the rectangle.
    masks: Vec<(i32, i32, [f32; 4])>,
    /// Its scaling grid (DefineScalingGrid) and the bounds of what it shows, both x0
    /// y0 x1 y1: scaled, it is drawn 9-sliced (Slice).
    grid: Option<([f32; 4], [f32; 4])>,
}

impl SpriteDef {
    fn key(&self, depth: i32, frame: usize) -> Option<&Key> {
        let mut found = None;
        for (f, k) in self.tracks.get(&depth)? {
            if *f <= frame {
                found = k.as_ref();
            } else {
                break;
            }
        }
        found
    }
}

struct BarDef {
    image: String,
    /// x0 x1 y0 y1 at ratio 0 and at ratio 1.
    start: [f32; 4],
    end: [f32; 4],
    /// uv = (a x + c y + tx, b x + d y + ty)
    uv: [f32; 6],
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Center,
    Right,
}

struct TextDef {
    rect: [f32; 4],
    face: String,
    size: f32,
    color: [f32; 4],
    align: Align,
    spacing: f32,
    text: String,
}

enum Def {
    Sprite(SpriteDef),
    Shape,
    Bar(BarDef),
    Text(TextDef),
}

struct Glyph {
    uv: [f32; 4],
    rect: [f32; 4],
    advance: f32,
}

struct FontDef {
    image: Handle<Image>,
    ascent: f32,
    glyphs: HashMap<char, Glyph>,
}

#[derive(Clone)]
struct ShapePiece {
    mesh: Handle<Mesh>,
    image: Option<Handle<Image>>,
    color: [f32; 4],
    /// A textured or filled rectangle (rect, uv), which a mask can cut down.
    quad: Option<([f32; 4], [f32; 4])>,
}

#[derive(Resource)]
struct HudLib {
    defs: HashMap<u32, Def>,
    symbols: HashMap<String, u32>,
    root: u32,
    images: HashMap<String, (Handle<Image>, Vec2)>,
    fonts: HashMap<String, FontDef>,
    /// The fonts' names in a fixed order (a text's pieces for runs in other
    /// fonts are numbered by it).
    font_names: Vec<String>,
    /// Each shape's quads and lines, built once.
    shapes: HashMap<u32, Vec<ShapePiece>>,
}

fn floats(parts: &[&str]) -> Vec<f32> {
    parts
        .iter()
        .map(|p| p.parse::<f32>().unwrap_or(0.0))
        .collect()
}

fn hex_color(s: &str) -> [f32; 4] {
    let v = u32::from_str_radix(s, 16).unwrap_or(0xffff_ffff);
    [
        ((v >> 24) & 255) as f32 / 255.0,
        ((v >> 16) & 255) as f32 / 255.0,
        ((v >> 8) & 255) as f32 / 255.0,
        (v & 255) as f32 / 255.0,
    ]
}

impl HudLib {
    /// Reads hud.txt (see tools/build_hud.py for the format).
    fn load(dir: &Path, asset_server: &AssetServer, meshes: &mut Assets<Mesh>) -> Option<HudLib> {
        let text = std::fs::read_to_string(dir.join("hud.txt")).ok()?;
        let mut lib = HudLib {
            defs: HashMap::new(),
            symbols: HashMap::new(),
            root: 0,
            images: HashMap::new(),
            fonts: HashMap::new(),
            font_names: Vec::new(),
            shapes: HashMap::new(),
        };
        // Shape pieces: (rect, uv, image, colour) quads and (points, width, colour) lines.
        let mut quads: Vec<(u32, [f32; 4], [f32; 4], Option<String>, [f32; 4])> = Vec::new();
        let mut lines: Vec<(u32, Vec<Vec2>, f32, [f32; 4])> = Vec::new();
        for raw in text.lines() {
            if raw.starts_with('#') || raw.trim().is_empty() {
                continue;
            }
            if let Some(rest) = raw.strip_prefix("text ") {
                let (head, body) = rest.split_once('|').unwrap_or((rest, ""));
                let p: Vec<&str> = head.split_whitespace().collect();
                if p.len() < 10 {
                    continue;
                }
                let f = floats(&p[1..5]);
                lib.defs.insert(
                    p[0].parse().unwrap_or(0),
                    Def::Text(TextDef {
                        rect: [f[0], f[1], f[2], f[3]],
                        face: p[5].to_string(),
                        size: p[6].parse().unwrap_or(12.0),
                        color: hex_color(p[7]),
                        align: match p[8] {
                            "right" => Align::Right,
                            "center" => Align::Center,
                            _ => Align::Left,
                        },
                        spacing: p[9].parse().unwrap_or(0.0),
                        text: body.to_string(),
                    }),
                );
                continue;
            }
            let p: Vec<&str> = raw.split_whitespace().collect();
            let num = |i: usize| p.get(i).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
            match p[0] {
                "image" if p.len() >= 5 => {
                    let handle = asset_server.load(format!("hud/{}", p[2]));
                    let size = Vec2::new(p[3].parse().unwrap_or(1.0), p[4].parse().unwrap_or(1.0));
                    lib.images.insert(p[1].to_string(), (handle, size));
                }
                "font" if p.len() >= 9 => {
                    lib.fonts.insert(
                        p[1].to_string(),
                        FontDef {
                            image: asset_server.load(format!("hud/{}", p[2])),
                            ascent: p[6].parse().unwrap_or(0.8),
                            glyphs: HashMap::new(),
                        },
                    );
                }
                "glyph" if p.len() >= 12 => {
                    let f = floats(&p[3..12]);
                    if let (Some(font), Some(ch)) =
                        (lib.fonts.get_mut(p[1]), char::from_u32(num(2)))
                    {
                        font.glyphs.insert(
                            ch,
                            Glyph {
                                uv: [f[0], f[1], f[2], f[3]],
                                rect: [f[4], f[5], f[6], f[7]],
                                advance: f[8],
                            },
                        );
                    }
                }
                "quad" if p.len() >= 15 => {
                    let f = floats(&p[3..15]);
                    quads.push((
                        num(1),
                        [f[0], f[1], f[2], f[3]],
                        [f[4], f[5], f[6], f[7]],
                        (p[2] != "-").then(|| p[2].to_string()),
                        [f[8], f[9], f[10], f[11]],
                    ));
                }
                "line" if p.len() >= 7 => {
                    let f = floats(&p[2..]);
                    let points = f[5..]
                        .chunks(2)
                        .filter(|c| c.len() == 2)
                        .map(|c| Vec2::new(c[0], c[1]))
                        .collect();
                    lines.push((num(1), points, f[0], [f[1], f[2], f[3], f[4]]));
                }
                "bar" if p.len() >= 17 => {
                    let f = floats(&p[3..17]);
                    lib.defs.insert(
                        num(1),
                        Def::Bar(BarDef {
                            image: p[2].to_string(),
                            start: [f[0], f[1], f[2], f[3]],
                            end: [f[4], f[5], f[6], f[7]],
                            uv: [f[8], f[9], f[10], f[11], f[12], f[13]],
                        }),
                    );
                }
                "sprite" if p.len() >= 3 => {
                    let frames = (num(2) as usize).max(1);
                    lib.defs.insert(
                        num(1),
                        Def::Sprite(SpriteDef {
                            frames,
                            labels: HashMap::new(),
                            stops: vec![false; frames],
                            gotos: HashMap::new(),
                            tracks: BTreeMap::new(),
                            masks: Vec::new(),
                            grid: None,
                        }),
                    );
                }
                "grid" if p.len() >= 10 => {
                    let f = floats(&p[2..10]);
                    if let Some(Def::Sprite(sprite)) = lib.defs.get_mut(&num(1)) {
                        sprite.grid = Some(([f[0], f[1], f[2], f[3]], [f[4], f[5], f[6], f[7]]));
                    }
                }
                "mask" if p.len() >= 8 => {
                    let f = floats(&p[4..8]);
                    if let Some(Def::Sprite(sprite)) = lib.defs.get_mut(&num(1)) {
                        let depth = p[2].parse::<i32>().unwrap_or(0);
                        let clip = p[3].parse::<i32>().unwrap_or(0);
                        sprite.masks.push((depth, clip, [f[0], f[1], f[2], f[3]]));
                    }
                }
                "label" | "stop" | "goto" | "key" => {
                    let frame = num(2) as usize;
                    let Some(Def::Sprite(sprite)) = lib.defs.get_mut(&num(1)) else {
                        continue;
                    };
                    match p[0] {
                        "label" if p.len() >= 4 => {
                            sprite.labels.insert(p[3].to_string(), frame);
                        }
                        "stop" => {
                            if let Some(s) = sprite.stops.get_mut(frame) {
                                *s = true;
                            }
                        }
                        "goto" if p.len() >= 4 => {
                            sprite.gotos.insert(frame, p[3].to_string());
                        }
                        "key" if p.len() >= 5 => {
                            let depth = p[3].parse::<i32>().unwrap_or(0);
                            let key = if p[4] == "-" || p.len() < 22 {
                                None
                            } else {
                                let f = floats(&p[6..22]);
                                Some(Key {
                                    child: num(4),
                                    name: if p[5] == "-" {
                                        String::new()
                                    } else {
                                        p[5].to_string()
                                    },
                                    m: [f[0], f[1], f[2], f[3], f[4], f[5]],
                                    cx: Cx {
                                        mul: [f[6], f[7], f[8], f[9]],
                                        add: [
                                            f[10] / 255.0,
                                            f[11] / 255.0,
                                            f[12] / 255.0,
                                            f[13] / 255.0,
                                        ],
                                    },
                                    ratio: f[14] / 65535.0,
                                    // PlaceObject3 blend mode 8 is "add".
                                    additive: f[15] as i32 == 8,
                                })
                            };
                            sprite.tracks.entry(depth).or_default().push((frame, key));
                        }
                        _ => {}
                    }
                }
                "symbol" if p.len() >= 3 => {
                    lib.symbols.insert(p[1].to_string(), num(2));
                }
                "root" => lib.root = num(1),
                _ => {}
            }
        }
        for (id, rect, uv, image, color) in quads {
            let piece = ShapePiece {
                mesh: meshes.add(quad_mesh(rect, uv)),
                image: image.and_then(|i| lib.images.get(&i)).map(|i| i.0.clone()),
                color,
                quad: Some((rect, uv)),
            };
            lib.shapes.entry(id).or_default().push(piece);
            lib.defs.insert(id, Def::Shape);
        }
        for (id, points, width, color) in lines {
            let piece = ShapePiece {
                mesh: meshes.add(line_mesh(&points, width)),
                image: None,
                color,
                quad: None,
            };
            lib.shapes.entry(id).or_default().push(piece);
            lib.defs.insert(id, Def::Shape);
        }
        lib.font_names = lib.fonts.keys().cloned().collect();
        lib.font_names.sort();
        Some(lib)
    }

    /// The font a text in `face` is drawn with: that one, else the field's
    /// (`field`), else $NormalFont.
    fn face<'a>(&self, face: Option<&'a str>, field: &'a str) -> Option<&'a str> {
        face.into_iter()
            .chain([field, "$NormalFont"])
            .find(|f| self.fonts.contains_key(*f))
    }

    fn sprite(&self, id: u32) -> Option<&SpriteDef> {
        match self.defs.get(&id) {
            Some(Def::Sprite(s)) => Some(s),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Meshes
// ---------------------------------------------------------------------------

fn mesh_from(positions: Vec<[f32; 3]>, uvs: Vec<[f32; 2]>, indices: Vec<u32>) -> Mesh {
    let normals = vec![[0.0, 0.0, 1.0]; positions.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// A rectangle (x0, y0, x1, y1) in the movie's coordinates, with its uv rectangle.
fn quad_mesh(rect: [f32; 4], uv: [f32; 4]) -> Mesh {
    let [x0, y0, x1, y1] = rect;
    let [u0, v0, u1, v1] = uv;
    mesh_from(
        vec![[x0, y0, 0.0], [x1, y0, 0.0], [x1, y1, 0.0], [x0, y1, 0.0]],
        vec![[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
        vec![0, 1, 2, 0, 2, 3],
    )
}

/// A rectangle cut down to the part inside `clip` (both x0 y0 x1 y1), its uv
/// rectangle cut to match; nothing when they don't overlap.
fn clipped_quad_mesh(rect: [f32; 4], uv: [f32; 4], clip: [f32; 4]) -> Mesh {
    let [x0, y0, x1, y1] = rect;
    let (cx0, cy0) = (x0.max(clip[0]), y0.max(clip[1]));
    let (cx1, cy1) = (x1.min(clip[2]), y1.min(clip[3]));
    if cx0 >= cx1 || cy0 >= cy1 {
        return empty_mesh();
    }
    let u = |x: f32| uv[0] + (uv[2] - uv[0]) * (x - x0) / (x1 - x0).max(1e-6);
    let v = |y: f32| uv[1] + (uv[3] - uv[1]) * (y - y0) / (y1 - y0).max(1e-6);
    quad_mesh([cx0, cy0, cx1, cy1], [u(cx0), v(cy0), u(cx1), v(cy1)])
}

/// A sprite with a scaling grid (DefineScalingGrid), scaled: Flash draws what it
/// shows 9-sliced. The bounds of what it shows scale as ever; the parts of them
/// outside the grid keep their own size (shrunk together only where the whole is
/// smaller than they are), the part inside stretching over the rest. The ammo bar's
/// segments are sliced so, their soft ends a few pixels wide however long the
/// segment (mc_ammoBarSegment sets its parts' _width).
#[derive(Clone, Copy, PartialEq, Debug)]
struct Slice {
    /// The grid and the bounds, x0 y0 x1 y1, in the sprite's coordinates.
    grid: [f32; 4],
    bounds: [f32; 4],
    /// The sprite's own scale (signs and all).
    scale: Vec2,
    /// Where the node whose pieces are drawn is in the sprite (placed in it, or in
    /// what it holds, without turning or scaling).
    offset: Vec2,
}

impl Slice {
    fn scaled(&self) -> bool {
        (self.scale.abs() - Vec2::ONE).abs().max_element() > 1e-4
    }

    /// What a piece's mesh is built for.
    fn key(&self) -> [f32; 4] {
        [self.scale.x, self.scale.y, self.offset.x, self.offset.y]
    }

    /// Along one axis (0: x, 1: y), where a point of the sprite (`v`, in its
    /// coordinates) is drawn, in the sprite's coordinates before its scale (which
    /// then takes it to its 9-sliced place).
    fn place(&self, axis: usize, v: f32) -> f32 {
        let (b0, b1) = (self.bounds[axis], self.bounds[axis + 2]);
        let (g0, g1) = (
            self.grid[axis].clamp(b0, b1),
            self.grid[axis + 2].clamp(b0, b1),
        );
        let scale = self.scale[axis].abs().max(1e-6);
        let (left, right) = (g0 - b0, b1 - g1);
        let size = (b1 - b0) * scale;
        let k = if left + right > size {
            size / (left + right)
        } else {
            1.0
        };
        let start = b0 * scale + left * k;
        let end = b1 * scale - right * k;
        let at = if v <= g0 {
            b0 * scale + (v - b0) * k
        } else if v >= g1 {
            b1 * scale - (b1 - v) * k
        } else {
            start + (end - start) * (v - g0) / (g1 - g0).max(1e-6)
        };
        at / scale
    }
}

/// A rectangle (x0, y0, x1, y1, in its node's coordinates) 9-sliced as `slice`
/// has it, its uv rectangle cut to match: up to nine quads.
fn sliced_quad_mesh(rect: [f32; 4], uv: [f32; 4], slice: &Slice) -> Mesh {
    let [x0, y0, x1, y1] = rect;
    // Each axis's cuts: the rectangle's ends and the grid's lines inside it, in the
    // sprite's coordinates, with the texture coordinate there.
    let cuts = |axis: usize, a: f32, b: f32, t0: f32, t1: f32| {
        let o = slice.offset[axis];
        let (a, b) = (a + o, b + o);
        let mut at = vec![a, b];
        for g in [slice.grid[axis], slice.grid[axis + 2]] {
            if g > a.min(b) && g < a.max(b) {
                at.push(g);
            }
        }
        at.sort_by(f32::total_cmp);
        if a > b {
            at.reverse();
        }
        at.into_iter()
            .map(|v| {
                let f = if (b - a).abs() > 1e-6 {
                    (v - a) / (b - a)
                } else {
                    0.0
                };
                (slice.place(axis, v) - o, t0 + (t1 - t0) * f)
            })
            .collect::<Vec<_>>()
    };
    let xs = cuts(0, x0, x1, uv[0], uv[2]);
    let ys = cuts(1, y0, y1, uv[1], uv[3]);
    let mut pos = Vec::new();
    let mut uvs = Vec::new();
    for &(y, v) in &ys {
        for &(x, u) in &xs {
            pos.push([x, y, 0.0]);
            uvs.push([u, v]);
        }
    }
    let w = xs.len() as u32;
    let mut idx = Vec::new();
    for j in 0..ys.len() as u32 - 1 {
        for i in 0..w - 1 {
            let a = j * w + i;
            idx.extend_from_slice(&[a, a + 1, a + w + 1, a, a + w + 1, a + w]);
        }
    }
    mesh_from(pos, uvs, idx)
}

/// The ability meter's wedge (meter_mc's mask_mc: DrawCircleSegment from 0 to
/// `fill` x 0.98 of a turn, counter-clockwise from the top, the mask turned 3.6
/// degrees anticlockwise) over a shape's rectangle: the part of the rectangle
/// inside it, its texture mapped as the whole rectangle's.
fn pie_quad_mesh(rect: [f32; 4], uv: [f32; 4], fill: f32) -> Mesh {
    let fill = fill.clamp(0.0, 1.0);
    if fill <= 0.0 {
        return empty_mesh();
    }
    let [x0, y0, x1, y1] = rect;
    // (Far enough out to cover the rectangle, with the chords of the arc outside it.)
    let reach = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)]
        .iter()
        .map(|(x, y)| (x * x + y * y).sqrt())
        .fold(1.0f32, f32::max);
    let start = -0.01f32;
    let sweep = -fill * 0.98;
    let pieces = ((sweep.abs() * 16.0).ceil() as usize).max(1);
    let step = sweep / pieces as f32;
    let radius = reach / (std::f32::consts::PI * step.abs()).cos().max(0.5) + 1.0;
    // Flash's y points down: turn a from the top is (sin, -cos), a growing clockwise.
    let point = |a: f32| {
        let t = a * TAU;
        Vec2::new(t.sin(), -t.cos()) * radius
    };
    let u = |p: Vec2| {
        [
            uv[0] + (uv[2] - uv[0]) * (p.x - x0) / (x1 - x0).max(1e-6),
            uv[1] + (uv[3] - uv[1]) * (p.y - y0) / (y1 - y0).max(1e-6),
        ]
    };
    let mut pos = Vec::new();
    let mut uvs = Vec::new();
    let mut idx = Vec::new();
    for k in 0..pieces {
        let a = start + step * k as f32;
        // A thin sector (convex), cut to the rectangle edge by edge.
        let mut poly = vec![Vec2::ZERO, point(a), point(a + step)];
        for (axis, bound, keep_less) in
            [(0, x0, false), (0, x1, true), (1, y0, false), (1, y1, true)]
        {
            let inside = |p: Vec2| {
                let v = if axis == 0 { p.x } else { p.y };
                if keep_less { v <= bound } else { v >= bound }
            };
            let mut out = Vec::new();
            for i in 0..poly.len() {
                let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
                let (pi, qi) = (inside(p), inside(q));
                if pi {
                    out.push(p);
                }
                if pi != qi {
                    let (pv, qv) = if axis == 0 { (p.x, q.x) } else { (p.y, q.y) };
                    let t = (bound - pv) / (qv - pv);
                    out.push(p + (q - p) * t);
                }
            }
            poly = out;
            if poly.len() < 3 {
                break;
            }
        }
        if poly.len() < 3 {
            continue;
        }
        let base = pos.len() as u32;
        for p in &poly {
            pos.push([p.x, p.y, 0.0]);
            uvs.push(u(*p));
        }
        for i in 1..poly.len() as u32 - 1 {
            idx.extend_from_slice(&[base, base + i, base + i + 1]);
        }
    }
    if idx.is_empty() {
        return empty_mesh();
    }
    mesh_from(pos, uvs, idx)
}

/// A rectangle (x0 y0 x1 y1) in a parent's coordinates seen from a child placed
/// at `t` within it, when `t` only moves and scales (else None: masks under
/// rotated or tilted clips are not applied).
fn rect_into(rect: [f32; 4], t: &Transform) -> Option<[f32; 4]> {
    let (sx, sy) = (t.scale.x, t.scale.y);
    let r = t.rotation;
    if r.x.abs() > 1e-4
        || r.y.abs() > 1e-4
        || r.z.abs() > 1e-4
        || sx.abs() < 1e-6
        || sy.abs() < 1e-6
    {
        return None;
    }
    let ax = (rect[0] - t.translation.x) / sx;
    let bx = (rect[2] - t.translation.x) / sx;
    let ay = (rect[1] - t.translation.y) / sy;
    let by = (rect[3] - t.translation.y) / sy;
    Some([ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)])
}

/// The overlap of two rectangles (an empty one when they don't overlap).
fn intersect(a: Option<[f32; 4]>, b: [f32; 4]) -> [f32; 4] {
    let r = match a {
        Some(a) => [
            a[0].max(b[0]),
            a[1].max(b[1]),
            a[2].min(b[2]),
            a[3].min(b[3]),
        ],
        None => b,
    };
    if r[0] >= r[2] || r[1] >= r[3] {
        [0.0; 4]
    } else {
        r
    }
}

/// A polyline `width` wide (at least a pixel).
fn line_mesh(points: &[Vec2], width: f32) -> Mesh {
    let mut pos = Vec::new();
    let mut idx = Vec::new();
    let h = width.max(1.0) / 2.0;
    for seg in points.windows(2) {
        let d = (seg[1] - seg[0]).normalize_or_zero();
        let n = Vec2::new(-d.y, d.x) * h;
        // Run on a little past each end so the joints have no gaps.
        let a = seg[0] - d * h * 0.5;
        let b = seg[1] + d * h * 0.5;
        let base = pos.len() as u32;
        for p in [a + n, b + n, b - n, a - n] {
            pos.push([p.x, p.y, 0.0]);
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    if pos.is_empty() {
        return empty_mesh();
    }
    let uvs = vec![[0.5, 0.5]; pos.len()];
    mesh_from(pos, uvs, idx)
}

/// A mesh that draws nothing (meshes need their attributes even when empty).
fn empty_mesh() -> Mesh {
    mesh_from(vec![[0.0, 0.0, 0.0]; 3], vec![[0.0, 0.0]; 3], vec![0, 1, 2])
}

/// A bar morph at `ratio`: its rectangle grows from the start shape to the end
/// shape with the bitmap fixed in place, so more of the bitmap shows.
fn bar_mesh(bar: &BarDef, ratio: f32) -> Mesh {
    let lerp = |i: usize| bar.start[i] + (bar.end[i] - bar.start[i]) * ratio;
    let (x0, x1, y0, y1) = (lerp(0), lerp(1), lerp(2), lerp(3));
    let [a, b, c, d, tx, ty] = bar.uv;
    let uv = |x: f32, y: f32| [a * x + c * y + tx, b * x + d * y + ty];
    mesh_from(
        vec![[x0, y0, 0.0], [x1, y0, 0.0], [x1, y1, 0.0], [x0, y1, 0.0]],
        vec![uv(x0, y0), uv(x1, y0), uv(x1, y1), uv(x0, y1)],
        vec![0, 1, 2, 0, 2, 3],
    )
}

/// The reload ring: `fraction` of a circle, clockwise from the top.
fn arc_mesh(radius: f32, width: f32, fraction: f32) -> Mesh {
    if fraction <= 0.0 {
        return empty_mesh();
    }
    let steps = ((64.0 * fraction).ceil() as u32).max(1);
    let (r0, r1) = (radius - width / 2.0, radius + width / 2.0);
    let mut pos = Vec::new();
    let mut idx = Vec::new();
    for i in 0..=steps {
        // Flash's y points down: clockwise from 12 o'clock is (sin a, -cos a).
        let a = fraction * TAU * i as f32 / steps as f32;
        let dir = Vec2::new(a.sin(), -a.cos());
        pos.push([dir.x * r0, dir.y * r0, 0.0]);
        pos.push([dir.x * r1, dir.y * r1, 0.0]);
        if i > 0 {
            let b = (i - 1) * 2;
            idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
        }
    }
    let uvs = vec![[0.5, 0.5]; pos.len()];
    mesh_from(pos, uvs, idx)
}

/// A character of a text where its field draws it (lay_out): its pen's x and its
/// line's baseline, its colour, font and size, or the button's picture in its place.
struct Laid<'a> {
    c: char,
    tint: [f32; 4],
    face: &'a str,
    size: f32,
    x: f32,
    base: f32,
    icon: Option<u8>,
}

/// A text laid out in its field like Flash: a 2-pixel gutter, the first baseline
/// one ascent down, lines split at '\n' and aligned in the field. Runs in another
/// font (<font face='...'>) and size take their place in the line in it. A
/// button's picture (its markup: BUTTON_MARKUPS) is a character as wide as it is,
/// and the line's ascent is its baseline's where that is more (the field's taller
/// for it).
fn lay_out<'a>(lib: &'a HudLib, def: &'a TextDef, text: &'a str) -> Vec<Laid<'a>> {
    let Some(field) = lib.face(None, &def.face) else {
        return Vec::new();
    };
    let Some(field_font) = lib.fonts.get(field) else {
        return Vec::new();
    };
    // <font color='#rrggbb' face='...' size='n'> runs (the game's contextual
    // commands colour the item named, a point award its "+N"): the text without
    // tags, and each character's colour, font and size.
    let (plain, runs) = text_runs(text);
    let icons = runs
        .iter()
        .filter_map(|r| r.icon.and_then(|k| button_icon(lib, k)));
    let ascent = icons.fold(field_font.ascent * def.size, |a, (_, _, b)| a.max(b));
    let mut run_of = runs.into_iter();
    let field_rgb = [def.color[0], def.color[1], def.color[2]];
    let glyph = |c: char, f: &str| lib.fonts.get(f).and_then(|font| font.glyphs.get(&c));
    let mut out = Vec::new();
    let [x0, y0, x1, _] = def.rect;
    for (row, line) in plain.split('\n').enumerate() {
        if row > 0 {
            run_of.next(); // the newline
        }
        let base = y0 + 2.0 + ascent + row as f32 * def.size * 1.25;
        // (Each character, and how far on it puts the next.)
        let chars: Vec<(Laid, f32)> = line
            .chars()
            .map(|c| {
                let run = run_of.next().unwrap_or_default();
                let [r, g, b] = run.color.unwrap_or(field_rgb);
                let face = lib.face(run.face, field).unwrap_or(field);
                let size = run.size.unwrap_or(def.size);
                let advance = match run.icon {
                    Some(k) => button_icon(lib, k).map_or(0.0, |(_, s, _)| s.x),
                    None => glyph(c, face).map_or(0.0, |g| g.advance * size + def.spacing),
                };
                let laid = Laid {
                    c,
                    tint: [r, g, b, 1.0],
                    face,
                    size,
                    x: 0.0,
                    base,
                    icon: run.icon,
                };
                (laid, advance)
            })
            .collect();
        let width: f32 = chars.iter().map(|ch| ch.1).sum();
        let mut x = match def.align {
            Align::Left => x0 + 2.0,
            Align::Right => x1 - 2.0 - width,
            Align::Center => (x0 + x1 - width) / 2.0,
        };
        for (laid, advance) in chars {
            out.push(Laid { x, ..laid });
            x += advance;
        }
    }
    out
}

/// A text field's glyphs in one font as a mesh (`face`: an index into the
/// library's font names, None for the field's own font), laid out as lay_out
/// lays them. Runs in another font (<font face='...'>) are drawn by that font's
/// own piece, as each font has its own texture; a button's picture by its own
/// bitmap (text_icons).
fn text_mesh(lib: &HudLib, def: &TextDef, text: &str, face: Option<usize>) -> Mesh {
    let Some(field) = lib.face(None, &def.face) else {
        return empty_mesh();
    };
    let target = match face {
        Some(i) => match lib.font_names.get(i) {
            Some(name) => name.as_str(),
            None => return empty_mesh(),
        },
        None => field,
    };
    // With runs the piece is drawn white, its characters coloured (marked_up).
    let coloured = marked_up(text);
    let mut pos = Vec::new();
    let mut uvs = Vec::new();
    let mut idx = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    for l in lay_out(lib, def, text) {
        if l.icon.is_some() || l.face != target {
            continue;
        }
        let Some(g) = lib.fonts.get(l.face).and_then(|font| font.glyphs.get(&l.c)) else {
            continue;
        };
        colors.extend_from_slice(&[l.tint; 4]);
        let b = pos.len() as u32;
        let (gx0, gy0) = (l.x + g.rect[0] * l.size, l.base + g.rect[1] * l.size);
        let (gx1, gy1) = (l.x + g.rect[2] * l.size, l.base + g.rect[3] * l.size);
        pos.extend_from_slice(&[
            [gx0, gy0, 0.0],
            [gx1, gy0, 0.0],
            [gx1, gy1, 0.0],
            [gx0, gy1, 0.0],
        ]);
        uvs.extend_from_slice(&[
            [g.uv[0], g.uv[1]],
            [g.uv[2], g.uv[1]],
            [g.uv[2], g.uv[3]],
            [g.uv[0], g.uv[3]],
        ]);
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    if pos.is_empty() {
        return empty_mesh();
    }
    let mesh = mesh_from(pos, uvs, idx);
    if coloured {
        // The shader multiplies by these (src/hud.wgsl).
        mesh.with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    } else {
        mesh
    }
}

/// The buttons' pictures in a text laid out in its field (lay_out): each one's
/// name, centre (in the field's coordinates) and scale (of its own size).
fn text_icons(lib: &HudLib, def: &TextDef, text: &str) -> Vec<(&'static str, Vec2, Vec2)> {
    lay_out(lib, def, text)
        .into_iter()
        .filter_map(|l| {
            let (image, size, baseline) = button_icon(lib, l.icon?)?;
            let own = lib.images.get(image)?.1;
            let centre = Vec2::new(l.x + size.x / 2.0, l.base - baseline + size.y / 2.0);
            Some((image, centre, size / own.max(Vec2::ONE)))
        })
        .collect()
}

/// Whether a text has markup (font runs): its piece is then drawn white with the
/// field's alpha, each character in its own colour (text_mesh).
fn marked_up(text: &str) -> bool {
    text.contains("<font")
}

/// How a run of marked-up text is drawn: its colour (sRGB), font and size where its
/// tags set them, else the field's.
#[derive(Clone, Copy, Default)]
struct Run<'a> {
    color: Option<[f32; 3]>,
    face: Option<&'a str>,
    size: Option<f32>,
    /// A button's picture in place of its markup (BUTTON_MARKUPS' index), the
    /// character BUTTON_CHAR.
    icon: Option<u8>,
}

/// Splits `<font color='#rrggbb' face='...' size='n'>...</font>` markup off a text:
/// the plain text, and for each character of it the run it is in. A button's
/// markup ({ButtonX}: BUTTON_MARKUPS) is one character, BUTTON_CHAR, its run's
/// icon.
fn text_runs(text: &str) -> (String, Vec<Run<'_>>) {
    let mut plain = String::new();
    let mut runs = Vec::new();
    let mut stack = vec![Run::default()];
    let mut rest = text;
    while !rest.is_empty() {
        if rest.starts_with('{')
            && let Some(k) = BUTTON_MARKUPS.iter().position(|b| rest.starts_with(b.1))
        {
            plain.push(BUTTON_CHAR);
            runs.push(Run {
                icon: Some(k as u8),
                ..stack.last().copied().unwrap_or_default()
            });
            rest = &rest[BUTTON_MARKUPS[k].1.len()..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("</font>") {
            if stack.len() > 1 {
                stack.pop();
            }
            rest = after;
            continue;
        }
        if rest.starts_with("<font") {
            if let Some(end) = rest.find('>') {
                let tag = &rest[..end];
                let outer = stack.last().copied().unwrap_or_default();
                let color = tag_attr(tag, "color")
                    .and_then(|v| v.strip_prefix('#'))
                    .and_then(|hex| hex.get(..6))
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .map(|v| [(v >> 16) & 255, (v >> 8) & 255, v & 255].map(|x| x as f32 / 255.0))
                    .or(outer.color);
                stack.push(Run {
                    color,
                    face: tag_attr(tag, "face").or(outer.face),
                    size: tag_attr(tag, "size")
                        .and_then(|s| s.parse().ok())
                        .or(outer.size),
                    icon: None,
                });
                rest = &rest[end + 1..];
                continue;
            }
        }
        let c = rest.chars().next().unwrap_or(' ');
        plain.push(c);
        runs.push(stack.last().copied().unwrap_or_default());
        rest = &rest[c.len_utf8()..];
    }
    (plain, runs)
}

/// The width of a text in a field (TextField.textWidth: its glyphs' advances, in
/// their runs' fonts and sizes, without the gutters), its widest line.
fn text_width(lib: &HudLib, def: &TextDef, text: &str) -> f32 {
    let Some(field) = lib.face(None, &def.face) else {
        return 0.0;
    };
    let (plain, runs) = text_runs(text);
    let mut widest: f32 = 0.0;
    let mut width = 0.0;
    for (c, run) in plain.chars().zip(runs) {
        if c == '\n' {
            widest = widest.max(width);
            width = 0.0;
            continue;
        }
        if let Some(k) = run.icon {
            width += button_icon(lib, k).map_or(0.0, |(_, size, _)| size.x);
            continue;
        }
        let f = lib.face(run.face, field).unwrap_or(field);
        if let Some(g) = lib.fonts.get(f).and_then(|font| font.glyphs.get(&c)) {
            width += g.advance * run.size.unwrap_or(def.size) + def.spacing;
        }
    }
    widest.max(width)
}

/// An attribute's value in an HTML tag (`name='value'` or `name="value"`).
fn tag_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(i) = tag[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        // A whole attribute name, not the end of a longer one.
        if at > 0 && !tag[..at].ends_with(char::is_whitespace) {
            continue;
        }
        let rest = tag[from..].trim_start().strip_prefix('=')?.trim_start();
        let quote = rest.chars().next()?;
        if quote != '\'' && quote != '"' {
            return None;
        }
        let value = &rest[1..];
        return value.find(quote).map(|end| &value[..end]);
    }
    None
}

// ---------------------------------------------------------------------------
// Instances: the display list
// ---------------------------------------------------------------------------

/// What a script set on an instance; these win over the timeline's values.
#[derive(Clone, Default)]
struct Props {
    x: Option<f32>,
    y: Option<f32>,
    /// Scale factors (Flash's _xscale / 100).
    xscale: Option<f32>,
    yscale: Option<f32>,
    /// Degrees, clockwise.
    rotation: Option<f32>,
    /// 0..1, in place of the timeline's alpha multiplier.
    alpha: Option<f32>,
    hidden: bool,
    cx: Option<Cx>,
    /// Flash's _xrotation and _yrotation, in degrees.
    rot_x: f32,
    rot_y: f32,
    text: Option<String>,
    /// The reload ring's fill.
    value: Option<f32>,
    /// The ability meter's fill (0..1): this node's shapes (and its children's) are
    /// cut to the wedge meter_mc's mask_mc draws (updateValue's DrawCircleSegment).
    pie: Option<f32>,
}

/// One placed object: a clip, shape, bar, text field, bitmap or ring.
#[derive(Component)]
struct HudNode {
    /// The character placed (0 for bitmaps and rings added by the scripts).
    def: u32,
    name: String,
    /// Depth in its parent clip (Flash draws children in depth order).
    depth: i32,
    key: Key,
    set: Props,
    /// Its HUD's render layer (split screen's guest's HUD's its own).
    layer: usize,
}

/// A clip's timeline.
#[derive(Component)]
struct HudClip {
    frame: usize,
    playing: bool,
    /// Frames to advance (fractional).
    acc: f32,
    /// The frame its children were last laid out for.
    synced: Option<usize>,
}

/// A drawn piece of a node: a mesh with its own material.
#[derive(Component)]
struct Piece {
    color: [f32; 4],
    kind: PieceKind,
    /// What the mesh was last built for (text, bar ratio or ring fill).
    built_text: String,
    built_value: f32,
    /// A shape's rectangle (rect, uv), and the mask rectangle it was last cut to.
    quad: Option<([f32; 4], [f32; 4])>,
    clip: Option<[f32; 4]>,
    /// The 9-slice it was last built for (Slice::key), if any.
    sliced: Option<[f32; 4]>,
}

#[derive(Clone, Copy, PartialEq)]
enum PieceKind {
    Static,
    Bar(u32),
    /// A text field's glyphs in its own font (None) or in another (an index into
    /// the library's font names) for runs marked up in it.
    Text(u32, Option<usize>),
    /// The reload ring: radius and line width.
    Ring(f32, f32),
}

/// Clips removed when they reach their last frame (the movie's damage arrows and
/// hit pings end with removeMovieClip).
#[derive(Component)]
struct RemoveAtEnd;

/// A clip whose frame script loops it back only so many times: the gotos it still
/// takes (mc_levelUpNotify's loopCount: its frame 58 goes back to "loop" while it
/// has been round fewer than 3 times, so twice).
#[derive(Component)]
struct Loops(u32);

#[derive(Component)]
struct HudRoot;

#[derive(Component)]
pub(crate) struct HudCamera;

/// The HUD's camera only draws while the HUD is shown, in a match. (Not in the
/// menus: nothing writes its state there, and what it was left showing came
/// through while the Customize screen was opening, before its black was up.)
fn show_hud(
    state: crate::split::BySeatRef<HudState>,
    pass: Res<HudPass>,
    app_state: Option<Res<State<crate::menu::AppState>>>,
    mut cameras: Query<(&mut Camera, &crate::split::Seat), With<HudCamera>>,
) {
    let in_match = app_state.is_none_or(|s| *s.get() == crate::menu::AppState::Game);
    let active = !state.seat(pass.0).hidden && in_match;
    // (This pass's HUD's: split screen's guest's in his.)
    for (mut camera, seat) in &mut cameras {
        if seat.0 == pass.0 && camera.is_active != active {
            camera.is_active = active;
        }
    }
}

/// Every node by path: names, or "#depth" for unnamed ones
/// ("hud_mc/bottomLeft_mc/#1/healthBar_mc"). Rebuilt by each layout pass.
#[derive(Resource, Default)]
struct HudIndex(HashMap<String, Entity>);

/// The stage's width in HUD units (it follows the window's shape), hud_mc's zoom
/// as it is drawn (HudLook's scale: squashed in the car, zoomed while aiming), and
/// the stage's height (STAGE_H, but for a split screen half narrower than 4:3:
/// layout_hud).
#[derive(Resource)]
struct HudStage(f32, Vec2, f32);

impl Default for HudStage {
    fn default() -> Self {
        HudStage(STAGE_H * 16.0 / 9.0, Vec2::ONE, STAGE_H)
    }
}

/// A split screen half's HUD keeps a stage at least this wide for its height (the
/// movie's 4:3 one, which the corner groups fit: side by side, a half is narrower,
/// and its stage taller instead).
const MIN_SPLIT_ASPECT: f32 = 4.0 / 3.0;

fn segment(name: &str, depth: i32) -> String {
    if name.is_empty() {
        format!("#{depth}")
    } else {
        name.to_string()
    }
}

fn join(path: &str, seg: &str) -> String {
    if path.is_empty() {
        seg.to_string()
    } else {
        format!("{path}/{seg}")
    }
}

/// Creates instances (the display list's attachMovie and timeline placements).
struct Spawner<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    lib: &'a HudLib,
    meshes: &'a mut Assets<Mesh>,
    materials: &'a mut Assets<HudMaterial>,
    /// What the last spawn placed at the paths its overrides set (spawn).
    placed: HashMap<String, Entity>,
    /// The render layer of the HUD it spawns into (hud_layer).
    layer: usize,
}

impl Spawner<'_, '_, '_> {
    fn piece(
        &mut self,
        parent: Entity,
        mesh: Handle<Mesh>,
        texture: Option<Handle<Image>>,
        piece: Piece,
        additive: bool,
    ) {
        let material = self.materials.add(HudMaterial {
            params: Vec4::new(if additive { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0),
            texture,
            colors: HUD_COLORS_BUFFER,
            order: 0.0,
        });
        self.commands.spawn((
            piece,
            // (Its slot among the HUD's colours: given as it is spawned.)
            HudSlot,
            MeshTag(u32::MAX),
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::default(),
            Visibility::Inherited,
            RenderLayers::layer(self.layer),
            ChildOf(parent),
        ));
    }

    fn node(&mut self, parent: Entity, key: Key, depth: i32, set: Props) -> Entity {
        self.commands
            .spawn((
                HudNode {
                    def: key.child,
                    name: key.name.clone(),
                    depth,
                    key,
                    set,
                    layer: self.layer,
                },
                Transform::default(),
                Visibility::Inherited,
                ChildOf(parent),
            ))
            .id()
    }

    /// Places `key`'s character under `parent` at `depth`: a clip with its first
    /// frame's children, a shape, bar or text field. `overrides` sets Props on
    /// descendants by their path below this instance ("glow_mc/glow_mc").
    #[allow(clippy::too_many_arguments)]
    fn spawn(
        &mut self,
        parent: Entity,
        key: Key,
        depth: i32,
        set: Props,
        additive: bool,
        path: &str,
        overrides: &HashMap<String, Props>,
    ) -> Entity {
        let def = key.child;
        let additive = additive || key.additive;
        let ratio = key.ratio;
        let lib = self.lib;
        let entity = self.node(parent, key, depth, set);
        if overrides.contains_key(path) {
            self.placed.insert(path.to_string(), entity);
        }
        match lib.defs.get(&def) {
            Some(Def::Sprite(sprite)) => {
                for &d in sprite.tracks.keys() {
                    if let Some(k) = sprite.key(d, 0) {
                        let child_path = join(path, &segment(&k.name, d));
                        let set = overrides.get(&child_path).cloned().unwrap_or_default();
                        self.spawn(entity, k.clone(), d, set, additive, &child_path, overrides);
                    }
                }
                self.commands.entity(entity).insert(HudClip {
                    frame: 0,
                    playing: !sprite.stops.first().copied().unwrap_or(false),
                    acc: 0.0,
                    synced: Some(0),
                });
            }
            Some(Def::Shape) => {
                for p in self.lib.shapes.get(&def).cloned().unwrap_or_default() {
                    let piece = Piece {
                        color: p.color,
                        kind: PieceKind::Static,
                        built_text: String::new(),
                        built_value: 0.0,
                        quad: p.quad,
                        clip: None,
                        sliced: None,
                    };
                    self.piece(entity, p.mesh, p.image, piece, additive);
                }
            }
            Some(Def::Bar(bar)) => {
                let mesh = self.meshes.add(bar_mesh(bar, ratio));
                let image = self.lib.images.get(&bar.image).map(|i| i.0.clone());
                let piece = Piece {
                    color: [1.0; 4],
                    kind: PieceKind::Bar(def),
                    built_text: String::new(),
                    built_value: ratio,
                    quad: None,
                    clip: None,
                    sliced: None,
                };
                self.piece(entity, mesh, image, piece, additive);
            }
            Some(Def::Text(text)) => {
                // Its own font's piece, and one for each other font in case the
                // text marks runs up in it.
                let field = lib.face(None, &text.face);
                let faces = std::iter::once(None).chain(
                    (0..lib.font_names.len())
                        .filter(|&i| Some(lib.font_names[i].as_str()) != field)
                        .map(Some),
                );
                for face in faces {
                    let mesh = self.meshes.add(text_mesh(lib, text, &text.text, face));
                    let name = match face {
                        Some(i) => Some(lib.font_names[i].as_str()),
                        None => field,
                    };
                    let font = name.and_then(|n| lib.fonts.get(n)).map(|f| f.image.clone());
                    let piece = Piece {
                        color: text.color,
                        kind: PieceKind::Text(def, face),
                        built_text: text.text.clone(),
                        built_value: 0.0,
                        quad: None,
                        clip: None,
                        sliced: None,
                    };
                    self.piece(entity, mesh, font, piece, additive);
                }
            }
            None => {}
        }
        entity
    }

    /// attachMovie(symbol, name, depth) under `parent`.
    fn attach(
        &mut self,
        parent: Entity,
        symbol: &str,
        name: &str,
        depth: i32,
        set: Props,
        additive: bool,
        overrides: &HashMap<String, Props>,
    ) -> Option<Entity> {
        let def = *self.lib.symbols.get(symbol)?;
        Some(self.spawn(
            parent,
            Key::placed(def, name),
            depth,
            set,
            additive,
            "",
            overrides,
        ))
    }

    /// attachBitmap: an image `scale` times its size (mirrored where negative),
    /// centred on `centre`, under `parent` at `depth`.
    fn bitmap(
        &mut self,
        parent: Entity,
        image: &str,
        scale: Vec2,
        centre: Vec2,
        depth: i32,
        additive: bool,
    ) -> Option<Entity> {
        let (handle, size) = self.lib.images.get(image)?.clone();
        let mut key = Key::placed(0, "");
        key.m = [scale.x, 0.0, 0.0, scale.y, centre.x, centre.y];
        let entity = self.node(parent, key, depth, Props::default());
        let mesh = self.meshes.add(quad_mesh(
            [-size.x / 2.0, -size.y / 2.0, size.x / 2.0, size.y / 2.0],
            [0.0, 0.0, 1.0, 1.0],
        ));
        let piece = Piece {
            color: [1.0; 4],
            kind: PieceKind::Static,
            built_text: String::new(),
            built_value: 0.0,
            quad: None,
            clip: None,
            sliced: None,
        };
        self.piece(entity, mesh, Some(handle), piece, additive);
        Some(entity)
    }

    /// A ring drawn by script (the reload ring's DrawCircleSegment).
    fn ring(
        &mut self,
        parent: Entity,
        radius: f32,
        width: f32,
        color: [f32; 4],
        depth: i32,
    ) -> Entity {
        let entity = self.node(parent, Key::placed(0, "ring"), depth, Props::default());
        let mesh = self.meshes.add(empty_mesh());
        let piece = Piece {
            color,
            kind: PieceKind::Ring(radius, width),
            built_text: String::new(),
            built_value: 0.0,
            quad: None,
            clip: None,
            sliced: None,
        };
        self.piece(entity, mesh, None, piece, false);
        entity
    }
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

fn setup_hud(
    mut commands: Commands,
    dir: Res<HudDir>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<HudMaterial>>,
) {
    let hud_dir = dir.0.join("hud");
    let Some(lib) = HudLib::load(&hud_dir, &asset_server, &mut meshes) else {
        info!(
            "No HUD in {:?} (tools/build_hud.py makes it): playing without one",
            hud_dir
        );
        return;
    };
    spawn_hud(&mut commands, &lib, &mut meshes, &mut materials, 0);
    commands.insert_resource(lib);
    info!("HUD loaded from {:?}", hud_dir);
}

/// Seat `seat`'s HUD (split.rs: the first player's from the start, the guest's as
/// he joins): its camera, on its own render layer, and the movie's root.
fn spawn_hud(
    commands: &mut Commands,
    lib: &HudLib,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<HudMaterial>,
    seat: usize,
) {
    let layer = hud_layer(seat);
    commands.spawn((
        HudCamera,
        crate::split::Seat(seat),
        Camera3d::default(),
        Camera {
            // Over the game, without clearing it (split screen's guest's: as
            // graphics.rs's apply_render_scale orders it).
            order: 1 + seat as isize,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Projection::from(PerspectiveProjection {
            fov: 45f32.to_radians(),
            near: 1.0,
            far: 100_000.0,
            ..default()
        }),
        Tonemapping::None,
        // HDR like the game camera it draws over (see setup_camera).
        Hdr,
        Transform::IDENTITY,
        RenderLayers::layer(layer),
    ));
    // The root flips y (Flash's points down) so the movie's coordinates work as
    // they are; layout_hud places it in front of the camera.
    let root = commands
        .spawn((
            HudRoot,
            crate::split::Seat(seat),
            Transform::from_xyz(0.0, 0.0, -1000.0).with_scale(Vec3::new(1.0, -1.0, 1.0)),
            Visibility::Visible,
        ))
        .id();
    let mut spawner = Spawner {
        commands,
        lib,
        meshes,
        materials,
        placed: HashMap::new(),
        layer,
    };
    // hud_mc; its damage-indicator group lies back 50 degrees (_xrotation = -50).
    let mut overrides = HashMap::new();
    overrides.insert(
        "midCenter_mc/#19".to_string(),
        Props {
            rot_x: -50.0,
            ..Default::default()
        },
    );
    let key = Key::placed(lib.root, "hud_mc");
    spawner.spawn(root, key, 0, Props::default(), false, "", &overrides);
    // markers_mc, under hud_mc: the markers over the other players, attached
    // by the scripts.
    spawner.node(root, Key::placed(0, "markers_mc"), -1, Props::default());
}

/// Split screen's guest's HUD (split.rs): made as he joins, gone (camera, movie and
/// all) as he leaves; his HUD's stores started afresh.
#[allow(clippy::too_many_arguments)]
fn guest_hud(
    mut commands: Commands,
    lib: Option<Res<HudLib>>,
    split: Res<crate::split::Split>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<HudMaterial>>,
    parts: Query<(Entity, &crate::split::Seat), Or<(With<HudRoot>, With<HudCamera>)>>,
    mut stores: ResMut<GuestStores>,
    mut guest_state: ResMut<crate::split::Second<HudState>>,
) {
    // (No guest: what the game says to every HUD of this PC's goes nowhere.)
    if !split.active() {
        guest_state.0.events.clear();
    }
    let Some(lib) = lib else {
        guest_state.0.events.clear();
        return;
    };
    let have = parts.iter().any(|(_, s)| s.0 == 1);
    if split.active() && !have {
        spawn_hud(&mut commands, &lib, &mut meshes, &mut materials, 1);
        *stores = GuestStores::default();
    } else if !split.active() && have {
        for (e, s) in &parts {
            if s.0 == 1 {
                commands.entity(e).despawn();
            }
        }
        *stores = GuestStores::default();
    }
}

/// Which seat's HUD the HUD's systems are working on (split screen: the guest's
/// after the first player's): its state (HudState, or split.rs's Second<HudState>
/// for the guest's), and its movie's own stores swapped in for the pass
/// (`GuestStores`).
#[derive(Resource, Default)]
struct HudPass(usize);

/// Split screen's guest's HUD's movie's stores, swapped with the first player's
/// (the resources, only the HUD's systems' own) for his pass: its paths and its
/// stage.
#[derive(Resource, Default)]
struct GuestStores {
    index: HudIndex,
    stage: HudStage,
}

/// The guest's pass begins: his stores in the resources.
fn guest_pass_in(
    mut pass: ResMut<HudPass>,
    mut index: ResMut<HudIndex>,
    mut stage: ResMut<HudStage>,
    mut stores: ResMut<GuestStores>,
) {
    pass.0 = 1;
    std::mem::swap(&mut *index, &mut stores.index);
    std::mem::swap(&mut *stage, &mut stores.stage);
}

/// And ends: the first player's back.
fn guest_pass_out(
    mut pass: ResMut<HudPass>,
    mut index: ResMut<HudIndex>,
    mut stage: ResMut<HudStage>,
    mut stores: ResMut<GuestStores>,
) {
    pass.0 = 0;
    std::mem::swap(&mut *index, &mut stores.index);
    std::mem::swap(&mut *stage, &mut stores.stage);
}

/// The guest's HUD is there (its pass runs).
fn guest_hud_up(split: Res<crate::split::Split>, lib: Option<Res<HudLib>>) -> bool {
    split.active() && lib.is_some()
}

/// Each HUD's camera draws into its player's part of the window (split.rs), at the
/// window's own size (over the Render Scale's picture: graphics.rs).
fn place_hud_views(
    split: Res<crate::split::Split>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cameras: Query<(&mut Camera, &crate::split::Seat), With<HudCamera>>,
) {
    let seats = split.seats();
    for (mut camera, seat) in &mut cameras {
        // (What it draws into: the window, or a test's picture.)
        let size = camera
            .physical_target_size()
            .unwrap_or_else(|| window.physical_size());
        let viewport = (seats > 1 && size.x >= 8 && size.y >= 8).then(|| {
            let (at, size) = crate::split::seat_rect(split.layout, seats, seat.0, size);
            bevy::camera::Viewport {
                physical_position: at,
                physical_size: size,
                ..default()
            }
        });
        let same = match (&camera.viewport, &viewport) {
            (Some(a), Some(b)) => {
                a.physical_position == b.physical_position && a.physical_size == b.physical_size
            }
            (None, None) => true,
            _ => false,
        };
        if !same {
            camera.viewport = viewport;
        }
    }
}

// ---------------------------------------------------------------------------
// Timelines
// ---------------------------------------------------------------------------

type ClipQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut HudNode,
        Option<&'static mut HudClip>,
        Option<&'static Children>,
        Has<RemoveAtEnd>,
    ),
>;

/// Plays every clip at 30 frames a second (stop frames, frame scripts'
/// gotoAndPlay) and keeps each clip's children in step with its timeline.
#[allow(clippy::too_many_arguments)]
fn step_clips(
    mut commands: Commands,
    time: Res<Time>,
    lib: Option<Res<HudLib>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<HudMaterial>>,
    mut q: ClipQuery,
    mut loops: Query<&mut Loops>,
    parents: Query<&ChildOf>,
) {
    let Some(lib) = lib else {
        return;
    };
    let dt = time.delta_secs().min(0.25);
    let mut to_sync = Vec::new();
    let mut to_remove = Vec::new();
    for (entity, node, clip, _, remove_at_end) in &mut q {
        let (Some(mut clip), Some(sprite)) = (clip, lib.sprite(node.def)) else {
            continue;
        };
        if clip.playing {
            clip.acc += dt * FPS;
            while clip.acc >= 1.0 && clip.playing {
                clip.acc -= 1.0;
                let mut next = clip.frame + 1;
                if next >= sprite.frames {
                    if remove_at_end {
                        to_remove.push(entity);
                        clip.playing = false;
                        break;
                    }
                    next = 0;
                }
                if let Some(&f) = sprite.gotos.get(&next).and_then(|l| sprite.labels.get(l)) {
                    // (A clip that loops a set number of times plays on once
                    // they're used up.)
                    let go = match loops.get_mut(entity) {
                        Ok(mut left) => {
                            let more = left.0 > 0;
                            left.0 = left.0.saturating_sub(1);
                            more
                        }
                        Err(_) => true,
                    };
                    if go {
                        next = f;
                    }
                }
                clip.frame = next;
                if sprite.stops.get(next).copied().unwrap_or(false) {
                    clip.playing = false;
                }
            }
        } else {
            clip.acc = 0.0;
        }
        if clip.synced != Some(clip.frame) {
            to_sync.push(entity);
        }
    }
    for e in &to_remove {
        commands.entity(*e).despawn();
    }
    // Clips despawned this pass (with everything under them): nothing more is
    // placed inside them, or Bevy would be asked to parent new pieces to entities
    // that are gone.
    let mut gone: HashSet<Entity> = to_remove.iter().copied().collect();
    let inside_gone = |e: Entity, gone: &HashSet<Entity>| {
        let mut at = Some(e);
        while let Some(x) = at {
            if gone.contains(&x) {
                return true;
            }
            at = parents.get(x).ok().map(|c| c.parent());
        }
        false
    };

    let mut spawner = Spawner {
        commands: &mut commands,
        lib: &lib,
        meshes: &mut meshes,
        materials: &mut materials,
        placed: HashMap::new(),
        layer: HUD_LAYER,
    };
    let none = HashMap::new();
    for entity in to_sync {
        if inside_gone(entity, &gone) {
            continue;
        }
        // The clip's frame and its children by depth (timeline depths only).
        let (def, frame, additive, children) = {
            let Ok((_, node, Some(mut clip), children, _)) = q.get_mut(entity) else {
                continue;
            };
            clip.synced = Some(clip.frame);
            // (Into its own HUD: split screen's guest's on its layer.)
            spawner.layer = node.layer;
            let kids: Vec<Entity> = children.map(|c| c.to_vec()).unwrap_or_default();
            (node.def, clip.frame, node.key.additive, kids)
        };
        let Some(sprite) = lib.sprite(def) else {
            continue;
        };
        let mut existing: HashMap<i32, (Entity, u32)> = HashMap::new();
        for child in children {
            if let Ok((_, n, ..)) = q.get(child) {
                if n.depth < ATTACH {
                    existing.insert(n.depth, (child, n.def));
                }
            }
        }
        for (&depth, _) in &sprite.tracks {
            let key = sprite.key(depth, frame);
            match (existing.get(&depth), key) {
                // The same character just takes the new key (and keeps what scripts set).
                (Some(&(child, child_def)), Some(k)) if child_def == k.child => {
                    if let Ok((_, mut n, ..)) = q.get_mut(child) {
                        n.key = k.clone();
                    }
                }
                (old, k) => {
                    if let Some(&(child, _)) = old {
                        spawner.commands.entity(child).despawn();
                        gone.insert(child);
                    }
                    if let Some(k) = k {
                        spawner.spawn(
                            entity,
                            k.clone(),
                            depth,
                            Props::default(),
                            additive,
                            "",
                            &none,
                        );
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Layout: transforms, colours, visibility and draw order
// ---------------------------------------------------------------------------

/// hud_mc's perspective, scale, warp and corner inset, eased between states.
#[derive(Default)]
struct HudLook {
    fov: f32,
    scale: Vec2,
    warp: f32,
    inset: f32,
}

type NodeQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut HudNode,
        &'static mut Transform,
        &'static mut Visibility,
        Has<HudClip>,
        Option<&'static Children>,
    ),
    (Without<HudRoot>, Without<HudCamera>, Without<Piece>),
>;
type PieceQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Piece,
        &'static MeshMaterial3d<HudMaterial>,
        &'static mut Mesh3d,
        &'static MeshTag,
    ),
>;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn layout_hud(
    lib: Option<Res<HudLib>>,
    (window, split, pass): (
        Single<&Window, With<PrimaryWindow>>,
        Res<crate::split::Split>,
        Res<HudPass>,
    ),
    state: crate::split::BySeatRef<HudState>,
    time: Res<Time>,
    mut index: ResMut<HudIndex>,
    mut stage: ResMut<HudStage>,
    mut look: Local<HudLook>,
    mut roots: Query<
        (&mut Transform, &Children, &crate::split::Seat),
        (With<HudRoot>, Without<HudNode>, Without<HudCamera>),
    >,
    mut cameras: Query<(&mut Projection, &Camera, &crate::split::Seat), With<HudCamera>>,
    mut nodes: NodeQuery,
    mut pieces: PieceQuery,
    mut materials: ResMut<Assets<HudMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    display: Res<crate::menu::DisplaySettings>,
    mut colors: ResMut<HudColors>,
) {
    let Some(lib) = lib else {
        return;
    };
    // (This pass's HUD: split screen's guest's in his.)
    let state = state.seat(pass.0);
    let Some((mut root_transform, root_children, _)) =
        roots.iter_mut().find(|(.., s)| s.0 == pass.0)
    else {
        return;
    };
    let Some((mut projection, camera, _)) = cameras.iter_mut().find(|(.., s)| s.0 == pass.0) else {
        return;
    };
    // (What its camera draws into: the window, or a test's picture.)
    let size = camera
        .physical_target_size()
        .unwrap_or_else(|| window.physical_size());

    // hud_mc's look (updateHudOffset): 45 degrees and a 6-degree warp normally;
    // zoomed in, corners pulled in and a 16-degree warp while aiming; squashed
    // with a 12-degree warp in the car. Eased like the movie's tweens.
    let (fov, scale, warp, inset, rate) = if state.vehicle {
        (75.0, Vec2::new(0.96, 0.90), 12.0, 0.0, 6.0)
    } else if state.fine_aim {
        (75.0, Vec2::splat(1.02), 16.0, 0.04, 12.0)
    } else {
        (45.0, Vec2::ONE, 6.0, 0.0, 6.0)
    };
    if look.fov == 0.0 {
        *look = HudLook {
            fov,
            scale,
            warp,
            inset,
        };
    }
    // A plain reference: Vec2's += through Local's DerefMut won't borrow-check.
    let look: &mut HudLook = &mut look;
    let k = (time.delta_secs() * rate).min(1.0);
    look.fov += (fov - look.fov) * k;
    let step = (scale - look.scale) * k;
    look.scale += step;
    look.warp += (warp - look.warp) * k;
    look.inset += (inset - look.inset) * k;

    // The stage is STAGE_H units tall and as wide as the window's shape (split
    // screen: its player's part of it). Flash 3D spreads the field of view across
    // the stage width; the root's plane, at that distance, shows exactly the stage.
    // (A split screen half narrower than 4:3 keeps a 4:3 stage's width, its stage
    // taller instead: MIN_SPLIT_ASPECT.)
    let part = crate::split::seat_rect(split.layout, split.seats(), pass.0, size).1;
    let aspect = (part.x as f32 / part.y.max(1) as f32).max(0.1);
    let (stage_w, stage_h) = if split.active() && aspect < MIN_SPLIT_ASPECT {
        let w = STAGE_H * MIN_SPLIT_ASPECT;
        (w, w / aspect)
    } else {
        (STAGE_H * aspect, STAGE_H)
    };
    stage.0 = stage_w;
    stage.2 = stage_h;
    let dist = (stage_w / 2.0) / (look.fov.to_radians() / 2.0).tan();
    if let Projection::Perspective(p) = &mut *projection {
        p.fov = 2.0 * ((stage_h / 2.0) / dist).atan();
        p.near = 1.0;
        p.far = dist * 10.0;
    }
    root_transform.translation = Vec3::new(0.0, 0.0, -dist);
    root_transform.scale = Vec3::new(look.scale.x, -look.scale.y, 1.0);
    stage.1 = look.scale;

    // Corner groups at the safe frame, pulled in while aiming, and turned by the
    // warp (updateStageScaling, updateScreenWarp: -warp on the left, +warp on the right).
    // ScreenSafeFrameX/Y (Settings > Display > Safe Frame) are the share of the
    // screen's width and height left around the corner groups, half on each side:
    // 0 on PC until it is set (HudData's DefaultScreenSafeFrame), putting the
    // corners at the window's edges.
    let frame = display.safe_frame;
    let cx = stage_w * (1.0 - frame.x) / 2.0 - stage_w * look.inset;
    let cy = stage_h * (1.0 - frame.y) / 2.0 - stage_h * look.inset;
    let w = look.warp;
    let places: [(&str, f32, f32, f32); 10] = [
        ("hud_mc/topLeft_mc", -cx, -cy, -w),
        ("hud_mc/midLeft_mc", -cx, 0.0, -w),
        ("hud_mc/bottomLeft_mc", -cx, cy, -w),
        ("hud_mc/topRight_mc", cx, -cy, w),
        ("hud_mc/midRight_mc", cx, 0.0, w),
        ("hud_mc/bottomRight_mc", cx, cy, w),
        ("hud_mc/topCenter_mc", 0.0, -cy, 0.0),
        ("hud_mc/bottomCenter_mc", 0.0, cy, 0.0),
        (
            "hud_mc/lowGlowBottomLeft_mc",
            -stage_w / 2.0,
            stage_h / 2.0,
            0.0,
        ),
        (
            "hud_mc/lowGlowBottomRight_mc",
            stage_w / 2.0,
            stage_h / 2.0,
            0.0,
        ),
    ];
    for (path, x, y, rot_y) in places {
        if let Some(&e) = index.0.get(path) {
            if let Ok((mut n, ..)) = nodes.get_mut(e) {
                n.set.x = Some(x);
                n.set.y = Some(y);
                n.set.rot_y = rot_y;
            }
        }
    }
    // A shot's camera shake jolts hud_mc with the view (updateCameraShake: 8 stage
    // pixels a degree of yaw and of pitch, a tenth of a degree a degree of roll),
    // and the scope back by 0.4 of that.
    let shake = state.camera_shake;
    if let Some(&e) = index.0.get("hud_mc")
        && let Ok((mut n, ..)) = nodes.get_mut(e)
    {
        let [a, b, _, _, tx, ty] = n.key.m;
        n.set.x = Some(tx + shake.y * 8.0);
        n.set.y = Some(ty + shake.x * 8.0);
        n.set.rotation = Some(b.atan2(a).to_degrees() + shake.z * 0.1);
    }
    if let Some(&e) = index.0.get(paths::SCOPE_MANAGER)
        && let Ok((mut n, ..)) = nodes.get_mut(e)
    {
        let [_, _, _, _, tx, ty] = n.key.m;
        n.set.x = Some(tx - shake.y * 8.0 * 0.4);
        n.set.y = Some(ty - shake.x * 8.0 * 0.4);
    }
    // markers_mc keeps to the screen: hud_mc's zoom isn't its.
    if let Some(&e) = index.0.get(paths::MARKERS)
        && let Ok((mut n, ..)) = nodes.get_mut(e)
    {
        n.set.xscale = Some(1.0 / look.scale.x);
        n.set.yscale = Some(1.0 / look.scale.y);
    }

    // Walk the display list in drawing order.
    let mut walk = Walk {
        index: HashMap::new(),
        pieces: Vec::new(),
    };
    let tops: Vec<Entity> = root_children.to_vec();
    for e in sort_by_depth(&nodes, tops) {
        visit(
            &mut walk,
            e,
            Cx::IDENT,
            None,
            None,
            None,
            "",
            &mut nodes,
            &mut pieces,
            &mut meshes,
            &lib,
        );
    }
    index.0 = walk.index;

    // The pieces' colours (HudColors, by their slots) and draw orders (their
    // materials'). A material changed is one prepared again for the GPU, so the
    // orders the pieces have are kept where they still rise along the list: a clip
    // attached or removed mid-list (a damage arrow, a hit ping) leaves the rest as
    // they were.
    let old: Vec<f32> = walk
        .pieces
        .iter()
        .map(|(id, ..)| materials.get(*id).map(|m| m.order).unwrap_or(0.0))
        .collect();
    let orders = draw_orders(&old);
    let mut recolored = false;
    for (&(id, slot, mul, add), order) in walk.pieces.iter().zip(orders) {
        recolored |= colors.bypass_change_detection().set(slot, mul, add);
        let changed = materials.get(id).is_some_and(|m| m.order != order);
        if changed && let Some(m) = materials.get_mut(id) {
            m.order = order;
        }
    }
    if recolored {
        colors.set_changed();
    }
}

struct Walk {
    index: HashMap<String, Entity>,
    /// The pieces in drawing order: their materials, slots (HudColors), colour
    /// multiplies and adds.
    pieces: Vec<(AssetId<HudMaterial>, u32, Vec4, Vec4)>,
}

/// Draw orders for the pieces, in drawing order, from the ones they have (0 for
/// none yet): whole numbers rising along the list by ORDER_STEP or more, as many
/// of the pieces' own kept as can be.
///
/// Pieces can keep theirs if each is far enough past the last kept one for the
/// pieces between: old[j] - old[i] >= (j - i) * ORDER_STEP, so old[i] - i *
/// ORDER_STEP never falls along them (and the first is ORDER_STEP or more past
/// 0). The most that can is the longest such run; the rest are spread evenly
/// between the kept ones round them, and those after the last kept one spaced
/// afresh after it.
fn draw_orders(old: &[f32]) -> Vec<f32> {
    let n = old.len();
    let held: Vec<Option<i64>> = old
        .iter()
        .map(|&o| (o.fract() == 0.0 && o > 0.0 && o < ORDER_LIMIT as f32).then_some(o as i64))
        .collect();
    let w = |i: usize| held[i].map(|o| o - i as i64 * ORDER_STEP);
    // The longest run with w not falling (patience sorting): tails[k] ends the
    // run of k + 1 found so far whose last w is least.
    let mut tails: Vec<usize> = Vec::new();
    let mut before: Vec<Option<usize>> = vec![None; n];
    for (i, link) in before.iter_mut().enumerate() {
        let Some(wi) = w(i).filter(|&wi| wi >= ORDER_STEP) else {
            continue;
        };
        let k = tails.partition_point(|&t| w(t).is_some_and(|wt| wt <= wi));
        *link = k.checked_sub(1).map(|k| tails[k]);
        if k == tails.len() {
            tails.push(i);
        } else {
            tails[k] = i;
        }
    }
    let mut keep = vec![false; n];
    let mut at = tails.last().copied();
    while let Some(i) = at {
        keep[i] = true;
        at = before[i];
    }
    let mut orders = vec![0i64; n];
    let mut lo = 0i64;
    let mut start = 0;
    for i in 0..n {
        if !keep[i] {
            continue;
        }
        let hi = held[i].unwrap_or(lo);
        let m = (i - start) as i64;
        for (j, o) in orders[start..i].iter_mut().enumerate() {
            *o = lo + (hi - lo) * (j as i64 + 1) / (m + 1);
        }
        orders[i] = hi;
        lo = hi;
        start = i + 1;
    }
    for (j, o) in orders[start..].iter_mut().enumerate() {
        *o = lo + ORDER_SPACING * (j as i64 + 1);
    }
    // (Run past the limit, by pieces added at the end: all afresh.)
    if orders.last().is_some_and(|&o| o >= ORDER_LIMIT) {
        let step = (ORDER_LIMIT / (n as i64 + 1)).clamp(1, ORDER_SPACING);
        for (i, o) in orders.iter_mut().enumerate() {
            *o = step * (i as i64 + 1);
        }
    }
    orders.into_iter().map(|o| o as f32).collect()
}

/// Node children in depth order (Flash's drawing order).
fn sort_by_depth(nodes: &NodeQuery, children: Vec<Entity>) -> Vec<Entity> {
    let mut list: Vec<(i32, usize, Entity)> = children
        .into_iter()
        .enumerate()
        .filter_map(|(i, e)| nodes.get(e).ok().map(|(n, ..)| (n.depth, i, e)))
        .collect();
    list.sort();
    list.into_iter().map(|(_, _, e)| e).collect()
}

#[allow(clippy::too_many_arguments)]
fn visit(
    walk: &mut Walk,
    entity: Entity,
    parent_cx: Cx,
    // A mask's rectangle over this node, in its parent's coordinates.
    parent_clip: Option<[f32; 4]>,
    // The ability meter's wedge over it (Props::pie, from it or a parent).
    parent_pie: Option<f32>,
    // A scaled sprite with a scaling grid it is in (Slice).
    parent_slice: Option<Slice>,
    parent_path: &str,
    nodes: &mut NodeQuery,
    pieces: &mut PieceQuery,
    meshes: &mut Assets<Mesh>,
    lib: &HudLib,
) {
    let (cx, clip, def, path, children, text, value, ratio, pie, slice) = {
        let Ok((node, mut transform, mut visibility, _, children)) = nodes.get_mut(entity) else {
            return;
        };
        // The timeline's matrix as translation, rotation and scale, with what
        // scripts set on top.
        let [a, b, c, d, tx, ty] = node.key.m;
        let sx = (a * a + b * b).sqrt();
        let det = a * d - b * c;
        let sy = if sx > 1e-6 {
            det / sx
        } else {
            (c * c + d * d).sqrt()
        };
        let rot = b.atan2(a);
        let set = &node.set;
        // Flash's 3D frame has z into the screen; here z comes out of it, so its x
        // and y rotations turn the other way.
        let tilt = Quat::from_rotation_y(-set.rot_y.to_radians())
            * Quat::from_rotation_x(-set.rot_x.to_radians());
        let t = Transform {
            translation: Vec3::new(set.x.unwrap_or(tx), set.y.unwrap_or(ty), 0.0),
            rotation: tilt
                * Quat::from_rotation_z(set.rotation.map(f32::to_radians).unwrap_or(rot)),
            scale: Vec3::new(set.xscale.unwrap_or(sx), set.yscale.unwrap_or(sy), 1.0),
        };
        let clip = parent_clip.and_then(|c| rect_into(c, &t));
        // A sprite with a scaling grid draws what it holds 9-sliced by its own
        // scale; what it holds placed in it without turning or scaling, with it.
        let slice = match lib.sprite(node.def).and_then(|s| s.grid) {
            Some((grid, bounds)) => Some(Slice {
                grid,
                bounds,
                scale: t.scale.truncate(),
                offset: Vec2::ZERO,
            }),
            None => parent_slice
                .filter(|_| {
                    t.rotation.angle_between(Quat::IDENTITY) < 1e-4
                        && (t.scale - Vec3::ONE).abs().max_element() < 1e-4
                })
                .map(|s| Slice {
                    offset: s.offset + t.translation.truncate(),
                    ..s
                }),
        };
        if *transform != t {
            *transform = t;
        }
        let want = if set.hidden {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *visibility != want {
            *visibility = want;
        }
        let mut cx = set.cx.unwrap_or(node.key.cx);
        if let Some(alpha) = set.alpha {
            cx.mul[3] = alpha;
        }
        let path = join(parent_path, &segment(&node.name, node.depth));
        let kids: Vec<Entity> = children.map(|c| c.to_vec()).unwrap_or_default();
        (
            cx.then(parent_cx),
            clip,
            node.def,
            path,
            kids,
            set.text.clone(),
            set.value,
            node.key.ratio,
            set.pie.or(parent_pie),
            slice,
        )
    };
    walk.index.insert(path.clone(), entity);

    // This node's own pieces (shapes, bars, text, bitmaps, rings).
    for &pe in &children {
        let Ok((mut piece, material, mut mesh, tag)) = pieces.get_mut(pe) else {
            continue;
        };
        match piece.kind {
            PieceKind::Text(def, face) => {
                if let Some(Def::Text(td)) = lib.defs.get(&def) {
                    let want = text.as_deref().unwrap_or(&td.text);
                    if piece.built_text != want {
                        // (A piece for another font stays empty while no run
                        // is in one.)
                        let empty = |t: &str| !t.contains("face=");
                        if face.is_none() || !(empty(&piece.built_text) && empty(want)) {
                            mesh.0 = meshes.add(text_mesh(lib, td, want, face));
                        }
                        piece.built_text = want.to_string();
                        piece.color = if marked_up(want) {
                            [1.0, 1.0, 1.0, td.color[3]]
                        } else {
                            td.color
                        };
                    }
                }
            }
            PieceKind::Bar(def) => {
                if (piece.built_value - ratio).abs() > 1e-4 {
                    if let Some(Def::Bar(bd)) = lib.defs.get(&def) {
                        mesh.0 = meshes.add(bar_mesh(bd, ratio));
                        piece.built_value = ratio;
                    }
                }
            }
            PieceKind::Ring(radius, width) => {
                let v = value.unwrap_or(0.0).clamp(0.0, 1.0);
                if (piece.built_value - v).abs() > 1e-3 {
                    mesh.0 = meshes.add(arc_mesh(radius, width, v));
                    piece.built_value = v;
                }
            }
            PieceKind::Static => {
                // A shape under a mask is cut down to the mask's rectangle (or
                // under the ability meter's wedge, to the wedge).
                if let (Some((rect, uv)), Some(fill)) = (piece.quad, pie) {
                    if piece.built_text != "pie" || (piece.built_value - fill).abs() > 1e-3 {
                        mesh.0 = meshes.add(pie_quad_mesh(rect, uv, fill));
                        piece.built_value = fill;
                        piece.built_text = "pie".to_string();
                        piece.clip = None;
                    }
                } else if let Some((rect, uv)) = piece.quad {
                    // (9-sliced in a scaled sprite with a scaling grid, unmasked.)
                    let sliced = slice.filter(|s| clip.is_none() && s.scaled());
                    let key = sliced.map(|s| s.key());
                    if piece.clip != clip || piece.built_text == "pie" || piece.sliced != key {
                        piece.built_text.clear();
                        mesh.0 = meshes.add(match (clip, sliced) {
                            (Some(c), _) => clipped_quad_mesh(rect, uv, c),
                            (None, Some(s)) => sliced_quad_mesh(rect, uv, &s),
                            (None, None) => quad_mesh(rect, uv),
                        });
                        piece.clip = clip;
                        piece.sliced = key;
                    }
                }
            }
        }
        let mul = Vec4::new(
            piece.color[0] * cx.mul[0],
            piece.color[1] * cx.mul[1],
            piece.color[2] * cx.mul[2],
            piece.color[3] * cx.mul[3],
        );
        let add = Vec4::from_array(cx.add);
        walk.pieces.push((material.id(), tag.0, mul, add));
    }
    // Then its child instances, in depth order; those its masks cover are clipped.
    let masks = lib.sprite(def).map(|s| s.masks.as_slice()).unwrap_or(&[]);
    for kid in sort_by_depth(nodes, children) {
        let depth = nodes.get(kid).map(|(n, ..)| n.depth).unwrap_or(0);
        let kid_clip = match masks.iter().find(|(m, c, _)| depth > *m && depth <= *c) {
            Some((_, _, rect)) => Some(intersect(clip, *rect)),
            None => clip,
        };
        visit(
            walk, kid, cx, kid_clip, pie, slice, &path, nodes, pieces, meshes, lib,
        );
    }
}

// ---------------------------------------------------------------------------
// The movie's scripts
// ---------------------------------------------------------------------------

/// Paths of the nodes the scripts drive.
mod paths {
    /// The corner groups hidden while down, dead or watching (updateHudVisibility):
    /// bottomLeft_mc (health, shield, the team's lives), bottomRight_mc (weapon,
    /// ammo, equipment, ability) and midCenter_mc (CENTRE: the crosshair).
    pub const BOTTOM_LEFT: &str = "hud_mc/bottomLeft_mc";
    pub const BOTTOM_RIGHT: &str = "hud_mc/bottomRight_mc";
    /// The team's lives (bottomLeft_mc's sprite 799: lifeMarkers_mc, its marker_1
    /// to marker_3 from the left).
    pub const LIVES: &str = "hud_mc/bottomLeft_mc/#13/lifeMarkers_mc";
    /// The downed meter (bottomCenter_mc's sprite 943), its wedge (meter_mc's
    /// highlight) and its self destruct line (sprite 940).
    pub const DOWNED: &str = "hud_mc/bottomCenter_mc/#13";
    pub const DOWNED_METER: &str = "hud_mc/bottomCenter_mc/#13/meter_mc/highlight_mc";
    pub const SELF_DESTRUCT: &str = "hud_mc/bottomCenter_mc/#13/#15/text_txt";
    /// The revive's bar (bottomCenter_mc's returnFlagNotifyTime_mc, sprite 936).
    pub const REVIVE_BAR: &str = "hud_mc/bottomCenter_mc/returnFlagNotifyTime_mc";
    /// "An ally is downed" (topCenter_mc's sprite 900) and its "Find and Revive
    /// your Ally" (sprite 899).
    pub const ALLY_DOWNED: &str = "hud_mc/topCenter_mc/#9";
    pub const FIND_ALLY: &str = "hud_mc/topCenter_mc/#9/#1/text_txt";
    pub const HEALTH_BAR: &str = "hud_mc/bottomLeft_mc/#1/healthBar_mc";
    pub const HEALTH_NUMBER: &str = "hud_mc/bottomLeft_mc/#1/healthBar_mc/number_txt";
    pub const SHIELD_BAR: &str = "hud_mc/bottomLeft_mc/#1/shieldBar_mc";
    pub const HEALTH_GLOW: &str = "hud_mc/bottomLeft_mc/#1/glow_mc";
    pub const EDGE_LEFT: &str = "hud_mc/lowGlowBottomLeft_mc";
    pub const EDGE_RIGHT: &str = "hud_mc/lowGlowBottomRight_mc";
    pub const WEAPON: &str = "hud_mc/bottomRight_mc/#1/currentWeapon_mc";
    pub const WEAPON_LABEL: &str = "hud_mc/bottomRight_mc/#1/currentWeapon_mc/label_mc";
    pub const WEAPON_LABEL_TEXT: &str =
        "hud_mc/bottomRight_mc/#1/currentWeapon_mc/label_mc/text_mc/text_txt";
    pub const WEAPON_ICON: &str = "hud_mc/bottomRight_mc/#1/currentWeapon_mc/iconAnchor_mc";
    pub const AMMO_BAR: &str = "hud_mc/bottomRight_mc/#1/ammoBar_mc";
    pub const AMMO_ANCHOR: &str = "hud_mc/bottomRight_mc/#1/ammoBar_mc/barAnchor_mc";
    pub const AMMO_NUMBER: &str = "hud_mc/bottomRight_mc/#1/ammoBar_mc/number_txt";
    pub const AMMO_GLOW: &str = "hud_mc/bottomRight_mc/#1/glow_mc";
    pub const AMMO_ADDED: &str = "hud_mc/bottomRight_mc/#1/#3";
    pub const EQUIPMENT: &str = "hud_mc/bottomRight_mc/#1/equipment_mc";
    /// The equipment slots: the offensive one's and the defensive one's.
    pub const EQUIPMENT_SLOTS: [&str; 2] = [
        "hud_mc/bottomRight_mc/#1/equipment_mc/offensive_mc",
        "hud_mc/bottomRight_mc/#1/equipment_mc/defensive_mc",
    ];
    pub const ABILITY: &str = "hud_mc/bottomRight_mc/ability_mc";
    pub const ROBOT_ABILITY: &str = "hud_mc/bottomRight_mc/ability_mc/robotAbility_mc";
    /// Where the item bought shows (topRight_mc's sprite 811) and the game's
    /// messages go (topLeft_mc's sprite 785).
    pub const ITEM_UNLOCKED: &str = "hud_mc/topRight_mc/#9";
    pub const GAME_MESSAGES: &str = "hud_mc/topLeft_mc/#3";
    pub const CENTRE: &str = "hud_mc/midCenter_mc";
    pub const CROSSHAIR_ANCHOR: &str = "hud_mc/midCenter_mc/#1/crosshairAnchor_mc";
    pub const RELOADING: &str = "hud_mc/midCenter_mc/#1/reloading_mc";
    pub const WARNINGS: &str = "hud_mc/midCenter_mc/#1/#3";
    pub const PINGS: &str = "hud_mc/midCenter_mc/#1/#13";
    pub const DAMAGE: &str = "hud_mc/midCenter_mc/#19/damageIndicator_mc";
    /// Escalation's wave (sprite 812) and the countdown to the next (sprite 816),
    /// src/waves.rs.
    pub const WAVE_LINE: &str = "hud_mc/topRight_mc/#11";
    pub const WAVE_SCOREBOARD: &str = "hud_mc/topRight_mc/#15";
    pub const MONEY_GROUP: &str = "hud_mc/topRight_mc/#13";
    pub const MONEY: &str = "hud_mc/topRight_mc/#13/displayMoney_mc";
    pub const MONEY_TEXT: &str = "hud_mc/topRight_mc/#13/displayMoney_mc/money_txt";
    pub const COUNTER: &str = "hud_mc/topCenter_mc/#21";
    /// Where the point transactions and the level-up notices go (topCenter_mc's
    /// sprite 894, midRight_mc's sprite 923).
    pub const POINTS: &str = "hud_mc/topCenter_mc/#7";
    pub const LEVEL_UP: &str = "hud_mc/midRight_mc/#1";
    pub const ANNOUNCEMENT: &str = "hud_mc/topCenter_mc/#3/announcement_mc/text_txt";
    pub const HINT: &str = "hud_mc/topCenter_mc/#3/reward_mc/text_txt";
    pub const SCOPE_MANAGER: &str = "hud_mc/scopeManager_mc";
    pub const NOTIFY: &str = "hud_mc/bottomCenter_mc/notifyManager_mc";
    pub const NOTIFY_GLOW: &str = "hud_mc/bottomCenter_mc/notifyManager_mc/glow_mc";
    pub const MARKERS: &str = "markers_mc";
}

/// mc_markerPawn's parts the demo has no use for (heal target, buffs, the class
/// icon, the off-screen arrow, low ammo, head count), hidden as the game hides them
/// for a player with health and nothing else to show; the low-health glow until it
/// is needed, and the downed icon till he is down. (A revealed sentry's:
/// ENEMY_MARKER_HIDDEN.)
const MARKER_HIDDEN: [&str; 8] = [
    "background_mc",
    "healTarget_mc",
    "buffs_mc",
    "container_mc/icon_mc",
    "container_mc/arrow_mc",
    "downed_mc",
    "lowAmmo_mc",
    "headCount_mc",
];

/// The parts of mc_markerPawn an enemy's marker hides (updateSpecialArguments for
/// an enemy with no name, health or shield to show - the game shows an enemy's
/// health only to a player with a fine-aim marker upgrade - nor low ammo, healing
/// or a head count): only its arrow (container_mc's icon_mc) and its buffs show.
const ENEMY_MARKER_HIDDEN: [&str; 9] = [
    "background_mc",
    "healTarget_mc",
    "health_mc",
    "shields_mc",
    "container_mc/arrow_mc",
    "container_mc/description_txt",
    "downed_mc",
    "lowAmmo_mc",
    "headCount_mc",
];
/// buffs_mc's place in mc_markerPawn (its initPosY), and how far down it goes when
/// the marker has no name (updateBuffsPosition: 19).
const MARKER_BUFFS_Y: f32 = -62.7;
const MARKER_NO_NAME_DROP: f32 = 19.0;
/// A buff's icon on a marker (updateBuffs): at IconScale 65%, the next IconSpacing
/// (18) along.
const BUFF_ICON_SCALE: f32 = 0.65;
/// The Orbital Pistol's marks' icons (getBuffIconLinkageId: TnBuffHardLocked, 2, 3).
const MARK_ICONS: [&str; 3] = [
    "BuffIconHardLocked",
    "BuffIconHardLocked2",
    "BuffIconHardLocked3",
];

/// mc_tacticalScope's range finder (MaxDistance, DisplayHeight) and compass (its
/// strip is 1728 wide: a full turn is half of it).
const SCOPE_MAX_DISTANCE: f32 = 150.0;
const SCOPE_DISPLAY_HEIGHT: f32 = 194.0;
const COMPASS_MULTIPLIER: f32 = 1728.0 / 4.0;
/// The contextual command (TransGame.int ContextualCommands): for a weapon on the
/// ground PickupWeapon, "Hold <<Pickup>> to pickup `p" with the weapon's name in
/// cyan; for something for sale PickupPurchasable, "Hold <<Pickup>> to purchase `p
/// for *`c" with the price in the energon numbers' font (the star is their energon
/// sign) in gold, the name as its ItemName has it (a weapon's in its type's colour,
/// equipment's with its slot's key: <<OffensiveEquipment>>, <<DefensiveEquipment>>);
/// for a downed teammate Revive, "Hold <<UseInteract>> to Revive" (Use/Interact is
/// the demo's one Interact key, as Pickup is); for an Armory Recreator LootBox,
/// "Press <<UseInteract>> to Activate Armory Recreator *`c", its price as a sale's;
/// for an unlock console its own line (doors.rs), `i the deposit and `c the price
/// in the numbers' font (the deposit gold, the price cyan, as the line has them).
/// A key is marked up as the game's TnInputCommandToBindingMapper does: a key in
/// #00FFD4 (KeyboardMarkupString), a mouse button in #FF9333 (its FontStr), a
/// controller's button (`pad`: the name pad.rs gives it) as its markup, which the
/// line shows as the button's picture (BUTTON_MARKUPS; a button without one by its
/// name, as a key).
fn command_text(
    key: &str,
    mouse: bool,
    pad: bool,
    equipment: &[(String, bool); 2],
    prompt: Prompt,
) -> String {
    let mark = |key: &str, mouse: bool| {
        if let Some(&(_, markup, _)) = BUTTON_MARKUPS.iter().find(|b| pad && b.0 == key) {
            return markup.to_string();
        }
        let color = if mouse { "#FF9333" } else { "#00FFD4" };
        format!("<font color='{color}'>{key}</font>")
    };
    let key = mark(key, mouse);
    match prompt {
        Prompt::Pickup(name) => {
            format!("Hold {key} to pickup <font color='#00FFFF'>{name}</font>")
        }
        Prompt::Purchase(name, cost) => {
            let name = name
                .replace(
                    "<<OffensiveEquipment>>",
                    &mark(&equipment[0].0, equipment[0].1),
                )
                .replace(
                    "<<DefensiveEquipment>>",
                    &mark(&equipment[1].0, equipment[1].1),
                );
            format!(
                "Hold {key} to purchase {name} for <font face='$NumberFontWide' color='#FFCC00'>*{cost}</font>"
            )
        }
        Prompt::Revive => format!("Hold {key} to Revive"),
        Prompt::Recreator(cost) => format!(
            "Press {key} to Activate Armory Recreator <font face='$NumberFontWide' color='#FFCC00'>*{cost}</font>"
        ),
        Prompt::Deposit(line, deposit, cost) => line
            .replace("<<UseInteract>>", &key)
            .replace("`i", &deposit.to_string())
            .replace("`c", &cost.to_string()),
    }
}

/// A controller's button as TnInputCommandToBindingMapper marks it up in a line
/// (its KeyToActionScriptBindings: XboxTypeS_X's "{ButtonX}" and so on, without a
/// key's colour), by the name pad.rs gives it, and the picture a line that
/// substitutes the markups shows in its place (ButtonIconData's IconDefs:
/// "Gamepad_" and the icon's id): the buttons whose pictures the HUD has (its movie
/// imports them from the button icons movie). The contextual command line has the
/// Interact button's, X, at a weapon, a sale, a console, an Armory Recreator or a
/// downed teammate.
const BUTTON_MARKUPS: &[(&str, &str, &str)] = &[
    ("X", "{ButtonX}", "Gamepad_X"),
    ("LB", "{ButtonL1}", "Gamepad_L1"),
    ("LT", "{ButtonL2}", "Gamepad_L2"),
    ("RB", "{ButtonR1}", "Gamepad_R1"),
    ("RT", "{ButtonR2}", "Gamepad_R2"),
    ("D-PAD UP", "{DpadUp}", "Gamepad_DUP"),
];
/// A button's picture in a line (mc_contextualCommandLine's text field:
/// ButtonIconData.CreateImageSubstitutionArray(29, 18, 1), its iconSize,
/// textHeight and iconOffset): 29 pixels high and as wide as that makes its
/// picture, its baseline (29 + 18 - 8) / 2 pixels down it and one of the picture's
/// own past that (its baseLineY, in the picture's pixels). It takes its place in
/// the line as a character as wide as it is, and makes its line's ascent its own
/// where that is more.
const BUTTON_ICON_SIZE: f32 = 29.0;
const BUTTON_ICON_TEXT_HEIGHT: f32 = 18.0;
const BUTTON_ICON_OFFSET: f32 = 1.0;
/// The character a button's markup stands as in a text's plain characters
/// (text_runs), its run saying which.
const BUTTON_CHAR: char = '\u{FFFC}';

/// A button's picture in a line (BUTTON_MARKUPS' `k`th): its name, its size in the
/// line and its baseline's distance down it.
fn button_icon(lib: &HudLib, k: u8) -> Option<(&'static str, Vec2, f32)> {
    let &(_, _, image) = BUTTON_MARKUPS.get(k as usize)?;
    let own = lib.images.get(image)?.1;
    let scale = BUTTON_ICON_SIZE / own.y.max(1.0);
    let size = Vec2::new(own.x * scale, BUTTON_ICON_SIZE);
    let baseline =
        (BUTTON_ICON_SIZE + BUTTON_ICON_TEXT_HEIGHT - 8.0) / 2.0 + BUTTON_ICON_OFFSET * scale;
    Some((image, size, baseline))
}

/// The contextual command line's text field (mc_contextualCommandLine's
/// commandText_mc's text_txt), below the line.
const COMMAND_FIELD: &str = "commandText_mc/text_txt";

/// The contextual command line's text field: its definition and its place down
/// commandText_mc.
fn command_field(lib: &HudLib) -> Option<(&TextDef, f32)> {
    let line = lib.sprite(*lib.symbols.get("mc_contextualCommandLine")?)?;
    let text = lib.sprite(line.key(1, 0)?.child)?;
    let key = text.key(1, 0)?;
    match lib.defs.get(&key.child) {
        Some(Def::Text(td)) => Some((td, key.m[5])),
        _ => None,
    }
}

/// How much higher a text's buttons' pictures make its line's ascent than its
/// font's (lay_out), if at all.
fn button_lift(lib: &HudLib, def: &TextDef, text: &str) -> f32 {
    let ascent = lib
        .face(None, &def.face)
        .and_then(|f| lib.fonts.get(f))
        .map_or(0.0, |f| f.ascent * def.size);
    let (_, runs) = text_runs(text);
    runs.iter()
        .filter_map(|r| r.icon.and_then(|k| button_icon(lib, k)))
        .map(|(_, _, baseline)| baseline - ascent)
        .fold(0.0, f32::max)
}

/// A jet's sticky rocket stuck in him (TransGame.int ContextualCommands.FlyerRocket:
/// "<<Transform>> to Disable Sticky Rocket"), the key marked up as command_text's.
fn sticky_rocket_text(key: &str, mouse: bool) -> String {
    let color = if mouse { "#FF9333" } else { "#00FFD4" };
    format!("<font color='{color}'>{key}</font> to Disable Sticky Rocket")
}

/// A note popping up for a while (ammo added, energon earned or spent).
struct Note {
    entity: Entity,
    age: f32,
    kind: NoteKind,
}

enum NoteKind {
    /// Ammo added: its slot, whether it was for the weapon in hand.
    Ammo { slot: usize, current: bool },
    /// The change it shows (summed while it shows).
    Energon(i64),
}

/// What the scripts remember between frames (of the movie under `root`: a new one,
/// split screen's guest's made again, starts afresh).
#[derive(Default)]
struct Logic {
    root: Option<Entity>,
    started: bool,
    weapon: Option<HudWeapon>,
    health_shown: f32,
    health_warning: bool,
    health_flash: f32,
    /// The shield as last shown, whether it is filling, its fill flash, and
    /// whether its bar is red.
    shield_shown: f32,
    shield_filling: bool,
    shield_flash: f32,
    shield_red: bool,
    low_ammo: bool,
    clip_shown: u32,
    ammo_flash: f32,
    ammo_bar: Option<Entity>,
    /// The clip size the ammo bar was made for.
    bar_cap: u32,
    segmented: bool,
    segment_count: u32,
    crosshair: Option<Entity>,
    crosshair_spin: f32,
    /// The reticle is to go straight to the end of its timeline (a mortar or
    /// saw-blade reticle not coming in: see MORTAR_CROSSHAIR).
    crosshair_end: bool,
    prong: f32,
    icon: Option<Entity>,
    label_timer: f32,
    reload_left: f32,
    reload_time: f32,
    ring: Option<Entity>,
    prompt_in: f32,
    prompt: Option<Entity>,
    warning: Option<(&'static str, Entity)>,
    money_shown: f32,
    money_target: u32,
    /// The energon counter's pop (revealMoney): seconds left, its length, and the
    /// colour it eases back from.
    money_pop: (f32, f32, Cx),
    /// The ability's icon attached (and which), whether it shows ready, whether it
    /// has been shown at all yet, and whether it shows jammed.
    ability_icon: Option<(Entity, &'static str)>,
    ability_ready: bool,
    ability_shown: bool,
    ability_jammed: bool,
    /// The Hover's icon's arrow's turn (degrees), and the Heal Beam's icon's juice
    /// last shown (it flashes as the juice starts over).
    ability_arrow: f32,
    ability_juice: f32,
    /// Each equipment slot's icon attached (and which).
    equipment_icons: [Option<(Entity, &'static str)>; 2],
    notes: Vec<Note>,
    low_health: Option<(Entity, Entity)>,
    /// The weapon icon waits a frame for its clip's anchor to exist.
    pending_icon: Option<&'static str>,
    /// The scope up (scope_mc) and which it is, the one playing out, the tactical
    /// scope's range finder's smoothed distance and over-the-maximum state, and
    /// whether the sniper scope shows its full charge ("ready") and has shown one.
    scope: Option<Entity>,
    scope_kind: Scope,
    scope_out: Option<Entity>,
    scope_distance: f32,
    scope_over_max: bool,
    scope_charged: bool,
    scope_charge_seen: bool,
    /// The sniper scope's charge-upgrade lights as last set (their `state`s: on or
    /// off; None before the first), and the upgrade's value they were set from.
    scope_lights: [Option<bool>; 3],
    scope_upgrade: Option<Option<u8>>,
    /// The tactical scope's target panel shown (its ShowTarget) and the name it
    /// was last given.
    scope_target: bool,
    scope_name: String,
    /// The ammo bar's segment flashing for the special shot (each segment's
    /// specialShot as last set: true for the one, None before the first).
    special_shown: Vec<Option<bool>>,
    /// The contextual command line shown (and its text), lines counted for names.
    command: Option<(Entity, String)>,
    command_count: u32,
    command_glow: bool,
    /// The game announcement showing (upper case) and how long it has been up.
    game_announcement: Option<(Vec<char>, f32)>,
    /// Each other player's marker, by his id, and whether it shows low health.
    markers: HashMap<usize, (Entity, bool)>,
    /// Each revealed sentry's marker, by its id: the marker, the mark its icon is
    /// for and the icon (mc_markerPawnBuffIcon); and the depth the next one takes.
    revealed: HashMap<u64, (Entity, u8, Option<Entity>)>,
    revealed_depth: i32,
    /// The lock-on marker (mc_markerLockOn) and its script's state.
    lock_on: Option<LockMarker>,
    /// Each enemy grenade's marker (mc_markerGrenade), by the grenade's id, and
    /// whether its icon is in yet; and the depth the next one takes.
    grenade_markers: HashMap<u64, (Entity, bool)>,
    grenade_depth: i32,
    /// The prompts under the crosshair (mc_activeGrenadePromptText,
    /// mc_clusterBombPromptText), while they show.
    grenade_prompt: Option<Entity>,
    cluster_prompt: Option<Entity>,
    /// The point transactions over the crosshair (topCenter_mc's sprite 894).
    points: Points,
    /// The level-up notice showing (midRight_mc's sprite 923: its LevelUpClip), and
    /// the notices attached so far (LevelUpCounter).
    level_up: Option<LevelUpNotice>,
    level_ups: u32,
    /// The item bought showing (sprite 811's ItemUnlockedClip).
    item_unlocked: Option<ItemPopup>,
    /// The game's messages showing, oldest first (sprite 785's MessageArray), and
    /// the lines attached so far (MessageCount).
    messages: Vec<MessageLine>,
    message_count: u32,
    /// The lines' places still to set (updateDisplay, which the game runs a moment
    /// after a line comes or goes): a line attached this frame isn't there to move
    /// until the next.
    messages_moved: bool,
    /// The reward announcement showing (upper case) and how long it has been up.
    reward_announcement: Option<(Vec<char>, f32)>,
    /// The downed meter on (sprite 943's currentState 'on'; it starts at 'init').
    downed_on: bool,
    /// "An ally is downed" on (sprite 900's currentState), and its "Find and Revive
    /// your Ally" as placed (sprite 899's field) and how long it has typed in.
    ally_notice_on: bool,
    find_ally: Option<(Entity, f32)>,
    /// Each downed player's marker's strobe (sprite 103's doStrobe), by his id: how
    /// long it has run.
    strobes: HashMap<usize, f32>,
    /// The respawn screen while it is up (MultiplayerRespawn_GFX).
    respawn: Option<RespawnScreen>,
    /// Escalation's countdown clock (src/waves.rs), once attached, and the wave's
    /// line showing (its clip, its text, how long it has typed in) and the wave it
    /// was last shown for.
    wave_clock: Option<WaveClock>,
    wave_line: Option<(Entity, String, f32)>,
    wave_shown: u32,
}

/// The countdown's clock as its script keeps it (mc_scoreboardSingleTeam's sprite
/// 229): whether it shows (ShowCountdown) and the seconds it last took
/// (CountdownTime); its _alpha and _x, its fade in or out (from what alpha and x, how
/// far in), its red flash (how far in) and how red it is.
struct WaveClock {
    shows: bool,
    value: Option<i32>,
    alpha: f32,
    x: f32,
    fade: Option<(f32, f32, f32)>,
    flash: Option<f32>,
    red: f32,
}

/// secondsToMinutesAndSeconds: the clock's white time (":30", "1:05") or its dim
/// digits under it ("00:30", "01:05").
fn clock_text(seconds: i32, time: bool) -> String {
    let seconds = seconds.max(0);
    let (minutes, seconds) = (seconds / 60, seconds % 60);
    let minutes = match (time, minutes) {
        (true, 0) => ":".to_string(),
        (true, m) => format!("{m}:"),
        (false, 0) => "00:".to_string(),
        (false, m) if m < 10 => format!("0{m}:"),
        (false, m) => format!("{m}:"),
    };
    format!("{minutes}{seconds:02}")
}

/// Flash's Regular.easeInOut.
fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - 2.0 * (1.0 - t) * (1.0 - t)
    }
}

/// The respawn screen (UI_GFxRespawn_p.MultiplayerRespawn_GFX) as its scripts keep
/// it: its three parts attached over the HUD (screenSoftEdges_mc, respawnTimer_mc,
/// and the spectating group, sprite 13), how long it has been up (their fades in),
/// and the spectating group's currentlySpectating_mc: whether it shows a player
/// (ShowSpectateTarget), its alpha easing from and to and for how long
/// (SetSpectateTarget's interp), and the name it shows.
struct RespawnScreen {
    edges: Entity,
    timer: Entity,
    spectate: Entity,
    age: f32,
    shows_target: bool,
    fade: (f32, f32, f32),
    name: String,
    /// The bumpers' pictures in the hint, while it shows a controller's.
    pad_icons: Option<Vec<Entity>>,
}

/// Sprite 894's point transactions as its script keeps them (updatePointEvent and
/// UpdateTransactions): the awards waiting (TransactionQueue), whether the queue is
/// running (TransactionsAreQueued) and the time to its next run (its setInterval),
/// the transaction showing (CurrentTransactionClip), those fading out, and the
/// clips attached so far (TransactionClipCounter).
#[derive(Default)]
struct Points {
    queue: VecDeque<PointAward>,
    running: bool,
    next: Option<f32>,
    shown: Option<Transaction>,
    fading: Vec<Fading>,
    count: u32,
}

/// An award waiting to be shown (a pointEvent).
struct PointAward {
    amount: u32,
    announcement: String,
    description: String,
    transaction: i32,
}

/// An mc_pointTransaction as its script keeps it: its clip and path, its
/// transactionId and TotalAwardAmmount, its label (mc_pointTransactionLabel) and
/// the labels attached so far (TransactionLabelCounter).
struct Transaction {
    entity: Entity,
    path: String,
    id: i32,
    total: u32,
    label: Option<Label>,
    labels: u32,
}

/// A transaction's label: its clip and path, and its announcement (upper case),
/// typed in for this long.
struct Label {
    entity: Entity,
    path: String,
    announcement: Vec<char>,
    age: f32,
}

/// A transaction fading out (its fadeOut), for this long.
struct Fading {
    entity: Entity,
    path: String,
    age: f32,
}

/// An mc_levelUpNotify as midRight_mc's NotifyLevelUp and its own frame scripts
/// keep it: its clip and path, whether it has been seen in place (it is attached a
/// frame before it exists), its level (the text), the class's icon still to attach
/// under its icon_mc (its first frame's), and whether its sparkles are placed (its
/// frame 21's).
struct LevelUpNotice {
    entity: Entity,
    path: String,
    seen: bool,
    level: String,
    icon: Option<String>,
    sparkles: bool,
}

/// An mc_itemUnlocked as sprite 811 and its own scripts keep it: its clip and path,
/// the item's picture (sprite 714 draws it once it is placed) and name (sprite 722
/// types it in once it is placed: tweenFrom over 0.05 s a letter), how long it has
/// been up (its FadeInterval sends it out after 6 s) and whether it has gone out.
struct ItemPopup {
    entity: Entity,
    path: String,
    image: &'static str,
    picture: Option<Entity>,
    name: Vec<char>,
    typing: Option<f32>,
    age: f32,
    out: bool,
}

/// mc_itemUnlocked's scripts: it goes out after ITEM_UNLOCKED_SECS (its
/// FadeInterval), the item's picture is drawn mirrored at ITEM_PICTURE_SCALE of its
/// size (sprite 714: _xscale -65, _yscale 65, its middle at the clip's height), its
/// name typed in at ITEM_NAME_TYPING a letter, and the whole of it in BlueColor
/// (sprite 811: ColorTransform(0, 0, 0, 1, 100, 255, 255, 0)).
const ITEM_UNLOCKED_SECS: f32 = 6.0;
const ITEM_PICTURE_SCALE: f32 = 0.65;
const ITEM_NAME_TYPING: f32 = 0.05;
const ITEM_BLUE: Cx = Cx::new([0.0, 0.0, 0.0, 1.0], [100.0, 255.0, 255.0, 0.0]);

/// An mc_gameMessage as sprite 785 keeps it: its clip and how long it has been up
/// (its FadeInterval takes it away after MESSAGE_SECS).
struct MessageLine {
    entity: Entity,
    age: f32,
}

/// Sprite 785's message lines: at most MESSAGE_LINES (the oldest goes as one more
/// comes), MESSAGE_SPACING apart; and each line's (mc_gameMessage's) time up, its
/// parts' alpha when any is neutral (60), and their colours (getColorFromIndex:
/// NormalColor, BlueColor, RedColor and PurpleColor).
const MESSAGE_LINES: usize = 5;
const MESSAGE_SPACING: f32 = 22.0;
const MESSAGE_SECS: f32 = 5.0;
const MESSAGE_NEUTRAL_ALPHA: f32 = 0.6;
const MESSAGE_BLUE: Cx = Cx::new([0.0, 1.0, 0.83, 1.0], [0.0; 4]);
const MESSAGE_RED: Cx = Cx::new([1.0, 0.53, 0.53, 1.0], [0.0; 4]);
const MESSAGE_PURPLE: Cx = Cx::new([0.82, 0.75, 1.0, 1.0], [0.0; 4]);
/// The icons in a message's text (GameMessageIconSubstitutionArray: each image at
/// 80% of its size, its baseline 23 down it, the text's baseline).
const MESSAGE_ICON_SCALE: f32 = 0.8;
const MESSAGE_ICON_BASELINE: f32 = 23.0;

/// A message part's colour (mc_gameMessage's getColorFromIndex): the player's and a
/// friend's (0 and 1) blue with Team Coloring Mode on Ally vs Enemy, else his side's
/// (an Autobot's red, a Decepticon's purple); an enemy's (2) red, or the enemy
/// side's; any other (neutral, 3) as it is.
fn message_color(index: u8, faction_colors: bool, decepticons: bool) -> Cx {
    match (index, faction_colors) {
        (0 | 1, false) => MESSAGE_BLUE,
        (0 | 1, true) if decepticons => MESSAGE_PURPLE,
        (0 | 1, true) => MESSAGE_RED,
        (2, false) => MESSAGE_RED,
        (2, true) if decepticons => MESSAGE_RED,
        (2, true) => MESSAGE_PURPLE,
        _ => NORMAL,
    }
}

/// Sprite 894's timing: the next award waits 2 s while one shows, 0.2 s after one
/// has gone (TransactionDisplayRate).
const POINTS_HOLD: f32 = 2.0;
const POINTS_GAP: f32 = 0.2;
/// mc_pointTransaction's fadeOut: its points_mc rises to 100 above the clip
/// (Regular.easeOut) as the clip fades away (Strong.easeOut), over a second.
const POINTS_FADE: f32 = 1.0;
const POINTS_RISE: f32 = 100.0;
/// An award's announcement types in at 0.025 s a letter (tweenFrom {text: '_'}).
const POINTS_TYPING: f32 = 0.025;
/// The sound of each award's label (MP_REWARD_DIALOG_BOX).
const SND_REWARD: &str = "BL_LVL_MP.MP_REWARD_DIALOG_BOX";
/// mc_levelUpNotify's frame 21 (its sparkles go either side of the level, 20 out)
/// and its "fadeOut" (where a notice in its way goes on: no sparkles from there).
const LEVEL_UP_SPARKLES: usize = 20;
const LEVEL_UP_SPARKLE_GAP: f32 = 20.0;

/// Strong.easeOut (quintic).
fn strong_ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(5)
}

/// mc_markerLockOn as its script keeps it (sprite 141): the sentry it is for, where
/// it is shown (currentPosX and Y, in markers_mc's place: the screen's middle at 0),
/// how fast it closes on the sentry once the lock is made (LockStickiness), whether
/// its locked_mc shows (ShowLocked), and the lock's progress it last showed.
struct LockMarker {
    entity: Entity,
    id: u64,
    pos: Vec2,
    stickiness: f32,
    locked: bool,
    last: f32,
}

/// How often a lock-on marker's script runs (its updateSpecialArguments, with each of
/// the markers' UpdateMarker: a frame of the game's, at 60 a second).
const MARKER_UPDATES: f32 = 60.0;

/// An enemy grenade's marker: its arrow's colour (sprite 539's
/// getGrenadeColorTransform: RedColor for any grenade but a healing one) and its
/// icon (getGrenadIconLinkageId: GrenadeIconFrag for a frag grenade) at 65%
/// (GrenadeIconClip's scale).
const GRENADE_RED: Cx = Cx::new([1.0, 0.53, 0.53, 1.0], [0.0; 4]);
const GRENADE_ICON: &str = "GrenadeIconFrag";
const GRENADE_ICON_SCALE: f32 = 0.65;
/// How far in from the safe frame markers kept on the screen stay
/// (ScreenClampEdgePadding: 90 of the movie's pixels, of its 1120 across and 720
/// down, InitStageWidth and InitStageHeight).
const CLAMP_PAD: f32 = 90.0;
const INIT_STAGE_W: f32 = 1120.0;

/// The box markers kept on the screen stay in (markers_mc's updateStageScaling:
/// ScreenClampXMin, ScreenClampXMax and so on), as half its width and height about
/// the screen's middle, in the stage's units: inside the safe frame (its share of
/// the screen, half on each side) by CLAMP_PAD.
///
/// On a stage `stage_h` tall (STAGE_H, or a split screen half's: HudStage).
fn marker_clamp_in(stage_w: f32, stage_h: f32, safe_frame: Vec2) -> Vec2 {
    let x = stage_w * safe_frame.x / 2.0 + CLAMP_PAD * stage_w / INIT_STAGE_W;
    let y = stage_h * safe_frame.y / 2.0 + CLAMP_PAD;
    Vec2::new((stage_w / 2.0 - x).max(0.0), (stage_h / 2.0 - y).max(0.0))
}

/// Where a marker kept on the screen goes (UpdateMarker, for a type
/// shouldMarkerTypeClampToScreen keeps there): `at` (normalized device coordinates,
/// as projected) in the stage's units about its middle (`half` its size), y down,
/// turned back through the middle when it is behind the camera. Off the screen
/// (outside `edge`, the clamp box's half size) or behind the camera, it is moved in
/// along its line from the middle to the box's edge, and its arrow turned to point
/// out along it (degrees, clockwise; 0 points down); None: no arrow.
fn clamp_marker(at: Vec2, in_front: bool, half: Vec2, edge: Vec2) -> (Vec2, Option<f32>) {
    let mut p = Vec2::new(at.x, -at.y) * half;
    if !in_front {
        p = -p;
    }
    if in_front && p.x.abs() <= edge.x && p.y.abs() <= edge.y {
        return (p, None);
    }
    // Its line through the middle, y = m x, met with each side in turn.
    let m = p.y / p.x;
    let within = |v: f32, lim: f32| v > -lim && v < lim;
    if p.x < 0.0 && within(-m * edge.x, edge.y) {
        p = Vec2::new(-edge.x, -m * edge.x);
    }
    if p.x > 0.0 && within(m * edge.x, edge.y) {
        p = Vec2::new(edge.x, m * edge.x);
    }
    if p.y < 0.0 && within(-edge.y / m, edge.x) {
        p = Vec2::new(-edge.y / m, -edge.y);
    }
    if p.y > 0.0 && within(edge.y / m, edge.x) {
        p = Vec2::new(edge.y / m, edge.y);
    }
    (p, Some((-p.x).atan2(p.y).to_degrees()))
}

/// Where the prompts under the crosshair go (midCenter_mc's sprites 872 and 873:
/// updateActiveGrenadeCount and updateCanClusterBomb attach them there).
const GRENADE_PROMPTS: &str = "hud_mc/midCenter_mc/#1/#9";
const CLUSTER_PROMPTS: &str = "hud_mc/midCenter_mc/#1/#11";

/// Flash's Regular.easeOut.
fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

/// A line of topCenter_mc's sprite 893 (its updateGameAnnouncement and
/// updateRewardAnnouncement): typed in behind an underscore at `letter_secs` a
/// letter (tweenFrom {text: '_'}, linear), held till 5 s after it came up, then
/// erased from the left over a second (tweenTo {text: ''}, Regular.easeOut); gone
/// (None) after that.
fn typed_line(line: &mut Option<(Vec<char>, f32)>, dt: f32, letter_secs: f32) -> String {
    let Some((text, age)) = line.as_mut() else {
        return String::new();
    };
    *age += dt;
    let n = text.len();
    let type_secs = n as f32 * letter_secs;
    let mut out = String::new();
    if *age < type_secs {
        let k = *age / type_secs;
        out = text[..((k * n as f32) as usize).min(n)].iter().collect();
        out.push('_');
    } else if *age < 5.0 {
        out = text.iter().collect();
    } else if *age < 6.0 {
        let k = ease_out(*age - 5.0);
        out = text[((k * n as f32) as usize).min(n)..].iter().collect();
    } else {
        *line = None;
    }
    out
}

/// `text` typed in `k` of the way (tweenFrom {text: '_'}, linear): its first
/// letters behind an underscore, all of it at the end.
fn typed_in(text: &str, k: f32) -> String {
    let letters: Vec<char> = text.chars().collect();
    let n = letters.len();
    let mut out: String = letters[..((k * n as f32) as usize).min(n)].iter().collect();
    if k < 1.0 {
        out.push('_');
    }
    out
}

/// The respawn screen (MultiplayerRespawn_GFX, a 1120 x 720 stage of its own shown
/// over the HUD's, centre to centre): its parts above everything the HUD attaches;
/// the spectating group (sprite 13) at (560, 560) and respawnTimer_mc at (560,
/// 528), from the stage's centre; screenSoftEdges_mc's square (128 across) stretched
/// over the stage.
const RESPAWN_DEPTH: i32 = ATTACH * 2;
const RESPAWN_SPECTATE_Y: f32 = 560.0 - 360.0;
const RESPAWN_TIMER_Y: f32 = 528.0 - 360.0;
const RESPAWN_EDGES_SIZE: f32 = 128.0;
/// respawn_txt in Escalation (GTS_SingleTeamGame: $UIText.Respawn.RespawnNextWave,
/// upper case).
const RESPAWN_NEXT_WAVE: &str = "WAITING FOR NEXT WAVE TO RESPAWN";
/// buttonHints_txt on PC with no gamepad ($UIText.Respawn.CyclePlayers between the
/// keys, [ and ], in #FF9333: the keys themselves, SwitchSpectatorTarget's
/// LeftBracket and RightBracket).
const CYCLE_PLAYERS_HINT: &str =
    "<font color='#FF9333'>[</font>  Cycle Players  <font color='#FF9333'>]</font>";
/// With a controller ("{lb}  Cycle Players  {rb}"): the words, with its bumpers'
/// pictures in place of {lb} and {rb} (SwitchSpectatorTarget on RB and LB:
/// pad.rs), CYCLE_PLAYERS_ICONS.
const CYCLE_PLAYERS_HINT_PAD: &str = "  Cycle Players  ";
/// The bumpers' pictures there (its setImageSubstitutions: GamepadButtonLB and
/// GamepadButtonRB, UI_GFxGamepadButtons_p's, each 34 pixels square, its
/// baseLineY 44 down the picture, in the picture's own pixels: 64 square).
const CYCLE_PLAYERS_ICONS: [&str; 2] = ["GamepadButtonLB", "GamepadButtonRB"];
const CYCLE_PLAYERS_ICON_SIZE: f32 = 34.0;
const CYCLE_PLAYERS_ICON_BASELINE: f32 = 44.0;
/// The spectating group's hint (buttonHints_txt) as respawnSpectate_mc lays it out:
/// its text, its field's place down the group (its timeline's, lower by what a
/// controller's pictures add to its line's ascent) and, with a controller, its
/// bumpers' pictures' scale and centres (each 34 pixels square at an end of the
/// line, centred in the group as the field's autoSize does, its baseline
/// CYCLE_PLAYERS_ICON_BASELINE of its own pixels down it, the words' baseline).
struct CycleHint {
    text: &'static str,
    y: f32,
    icon_scale: f32,
    icons: [Vec2; 2],
}

fn cycle_players_hint(lib: &HudLib, pad: bool) -> CycleHint {
    let field = lib
        .symbols
        .get("respawn_spectate")
        .and_then(|&d| lib.sprite(d))
        .and_then(|s| s.key(1, 0))
        .and_then(|k| match lib.defs.get(&k.child) {
            Some(Def::Text(td)) => Some((td, k.m[4], k.m[5])),
            _ => None,
        });
    let Some((td, x, y)) = field else {
        let text = if pad {
            CYCLE_PLAYERS_HINT_PAD
        } else {
            CYCLE_PLAYERS_HINT
        };
        return CycleHint {
            text,
            y: 0.0,
            icon_scale: 1.0,
            icons: [Vec2::ZERO; 2],
        };
    };
    if !pad {
        return CycleHint {
            text: CYCLE_PLAYERS_HINT,
            y,
            icon_scale: 1.0,
            icons: [Vec2::ZERO; 2],
        };
    }
    let ascent = lib
        .face(None, &td.face)
        .and_then(|f| lib.fonts.get(f))
        .map_or(0.0, |f| f.ascent * td.size);
    // (The pictures' own size: 64 square.)
    let own = lib
        .images
        .get(CYCLE_PLAYERS_ICONS[0])
        .map_or(64.0, |i| i.1.y.max(1.0));
    let k = CYCLE_PLAYERS_ICON_SIZE / own;
    let above = CYCLE_PLAYERS_ICON_BASELINE * k;
    let lift = (above - ascent).max(0.0);
    // The line: the words and a picture at each end, centred on the field's
    // middle; its baseline past the field's top gutter by its ascent.
    let words = text_width(lib, td, CYCLE_PLAYERS_HINT_PAD);
    let half = words / 2.0 + CYCLE_PLAYERS_ICON_SIZE;
    let middle = x + (td.rect[0] + td.rect[2]) / 2.0;
    let baseline = y + td.rect[1] + 2.0 + ascent.max(above);
    let centre_y = baseline - above + CYCLE_PLAYERS_ICON_SIZE / 2.0;
    let side = half - CYCLE_PLAYERS_ICON_SIZE / 2.0;
    CycleHint {
        text: CYCLE_PLAYERS_HINT_PAD,
        y: y + lift,
        icon_scale: k,
        icons: [
            Vec2::new(middle - side, centre_y),
            Vec2::new(middle + side, centre_y),
        ],
    }
}

/// Sprite 940's line on PC: $UIText.Hud.SelfDestruct ("Hold `b to Self Destruct")
/// with `b as " G " (DownedSuicide's key), upper case.
const SELF_DESTRUCT_TEXT: &str = "HOLD  G  TO SELF DESTRUCT";
/// With a controller: DownedSuicide is on B.
const SELF_DESTRUCT_TEXT_PAD: &str = "HOLD  B  TO SELF DESTRUCT";
/// Sprite 899's line: $UIText.Hud.ReviveAlly, upper case.
const FIND_ALLY_TEXT: &str = "FIND AND REVIVE YOUR ALLY";
/// A life lost (lifeMarkers_mc's RedColor).
const LIFE_LOST: Cx = Cx::new([0.25, 0.25, 0.25, 1.0], [175.0, 0.0, 0.0, 0.0]);
/// The downed icon on a downed player's marker (sprite 103's doStrobe): every half
/// second (StrobeDuration) it eases (Regular.easeOut) to the marker's colour at 75%
/// (TargetColor, TargetScale), then back to WarningColor, red at 75% alpha, at
/// 100% (WarningScale), starting from its own colours at full size.
const STROBE_SECS: f32 = 0.5;
const STROBE_WARNING: Cx = Cx::new([1.0, 0.0, 0.0, 0.75], [0.0; 4]);
const STROBE_SCALE: f32 = 0.75;

/// The downed icon's scale and colours `t` seconds into its strobe, towards the
/// marker's colour `target`.
fn strobe(t: f32, target: Cx) -> (f32, Cx) {
    let step = (t / STROBE_SECS).floor().max(0.0);
    let f = ease_out((t - step * STROBE_SECS) / STROBE_SECS);
    let (from, to) = if (step as u32).is_multiple_of(2) {
        let from = if step == 0.0 { NORMAL } else { STROBE_WARNING };
        ((1.0, from), (STROBE_SCALE, target))
    } else {
        ((STROBE_SCALE, target), (1.0, STROBE_WARNING))
    };
    (from.0 + (to.0 - from.0) * f, from.1.lerp(to.1, f))
}

type LogicQuery<'w, 's> = Query<'w, 's, (&'static mut HudNode, Option<&'static mut HudClip>)>;

/// Runs `f` on a node's script-set properties.
fn with(nodes: &mut LogicQuery, e: Option<Entity>, f: impl FnOnce(&mut Props)) {
    if let Some(e) = e {
        if let Ok((mut n, _)) = nodes.get_mut(e) {
            f(&mut n.set);
        }
    }
}

/// gotoAndStop / gotoAndPlay on a clip, by frame number (0-based) or label.
fn goto(
    nodes: &mut LogicQuery,
    lib: &HudLib,
    e: Option<Entity>,
    frame: Result<usize, &str>,
    play: bool,
) {
    let Some(e) = e else {
        return;
    };
    let Ok((node, Some(mut clip))) = nodes.get_mut(e) else {
        return;
    };
    let Some(sprite) = lib.sprite(node.def) else {
        return;
    };
    let Some((f, play)) = landing(sprite, frame, play) else {
        return;
    };
    if clip.frame != f || clip.playing != play {
        clip.frame = f;
        clip.playing = play;
        clip.acc = 0.0;
    }
}

/// Where gotoAndStop / gotoAndPlay (`play`) takes a clip of `sprite`, by frame
/// number (0-based) or label, and whether it plays on from there: as in Flash, the
/// frame it lands on runs its actions, so a stop() there halts it even after
/// gotoAndPlay (jammed_mc's "off", its empty first frame, where JAMMED is gone).
/// None for a label it doesn't have.
fn landing(sprite: &SpriteDef, frame: Result<usize, &str>, play: bool) -> Option<(usize, bool)> {
    let f = match frame {
        Ok(f) => f.min(sprite.frames - 1),
        Err(label) => *sprite.labels.get(label)?,
    };
    let stops = sprite.stops.get(f).copied().unwrap_or(false);
    Some((f, play && !stops))
}

fn frame_of(nodes: &LogicQuery, e: Option<Entity>) -> Option<usize> {
    let (_, clip) = nodes.get(e?).ok()?;
    clip.map(|c| c.frame)
}

/// Sprite 894's UpdateTransactions: the next award waiting goes to the transaction
/// showing if it is the same one (and shows 2 s), else the one showing fades out
/// first (the queue runs again 0.2 s on); with none showing, a new transaction clip
/// takes it (2 s; one with a transactionId under 0 fades out at once, 0.2 s). With
/// none waiting, the one showing fades out and the queue stops.
fn update_transactions(
    points: &mut Points,
    sp: &mut Spawner,
    anchor: Option<Entity>,
    sfx: &mut Option<ResMut<crate::sound::Sfx>>,
) {
    points.next = None;
    let Some(award) = points.queue.front() else {
        points.running = false;
        if let Some(t) = points.shown.take() {
            fade_transaction(points, sp, t);
        }
        return;
    };
    points.running = true;
    let same = points
        .shown
        .as_ref()
        .map(|t| t.id >= 0 && t.id == award.transaction);
    match same {
        Some(true) => {
            if let (Some(award), Some(t)) = (points.queue.pop_front(), points.shown.as_mut()) {
                update_transaction(t, sp, &award, sfx);
            }
            points.next = Some(POINTS_HOLD);
            return;
        }
        Some(false) => {
            if let Some(t) = points.shown.take() {
                fade_transaction(points, sp, t);
            }
            points.next = Some(POINTS_GAP);
            return;
        }
        None => {}
    }
    let Some(anchor) = anchor else {
        // (Not in place yet: try again shortly.)
        points.next = Some(POINTS_GAP);
        return;
    };
    let Some(award) = points.queue.pop_front() else {
        return;
    };
    let name = format!("transaction{}", points.count);
    let depth = ATTACH + points.count as i32;
    points.count += 1;
    // (Its frame script adds the award to its total: "+N" once there is some.)
    let mut ov = HashMap::new();
    if award.amount > 0 {
        ov.insert(
            "points_mc/points_txt".to_string(),
            Props {
                text: Some(format!("+{}", award.amount)),
                ..Default::default()
            },
        );
    }
    let Some(entity) = sp.attach(
        anchor,
        "mc_pointTransaction",
        &name,
        depth,
        Props::default(),
        false,
        &ov,
    ) else {
        return;
    };
    let mut t = Transaction {
        entity,
        path: format!("{}/{name}", paths::POINTS),
        id: award.transaction,
        total: 0,
        label: None,
        labels: 0,
    };
    update_transaction(&mut t, sp, &award, sfx);
    if t.id >= 0 {
        points.shown = Some(t);
        points.next = Some(POINTS_HOLD);
    } else {
        fade_transaction(points, sp, t);
        points.next = Some(POINTS_GAP);
    }
}

/// mc_pointTransaction's updateTransaction with an award: its amount added to the
/// total, and (the award having an announcement and a description, as every
/// award's are, if empty) a new label in place of the last: the announcement in
/// upper case typed in behind an underscore, the description with the award's own
/// amount after it once the total is more than it, and the label's sound.
fn update_transaction(
    t: &mut Transaction,
    sp: &mut Spawner,
    award: &PointAward,
    sfx: &mut Option<ResMut<crate::sound::Sfx>>,
) {
    t.total += award.amount;
    if let Some(old) = t.label.take() {
        sp.commands.entity(old.entity).try_despawn();
    }
    let announcement: Vec<char> = award.announcement.to_uppercase().chars().collect();
    let mut description = award.description.clone();
    if award.amount > 0 && t.total > award.amount {
        description.push_str(&format!(
            " (<font face='$NumberFont' size='14' color='#FFFFFF'>+{}</font>)",
            award.amount
        ));
    }
    let mut ov = HashMap::new();
    let typed = if announcement.is_empty() { "" } else { "_" };
    ov.insert(
        "announcement_txt".to_string(),
        Props {
            text: Some(typed.to_string()),
            ..Default::default()
        },
    );
    ov.insert(
        "description_txt".to_string(),
        Props {
            text: Some(description),
            ..Default::default()
        },
    );
    let name = format!("label{}", t.labels);
    let depth = ATTACH + t.labels as i32;
    t.labels += 1;
    if let Some(e) = sp.attach(
        t.entity,
        "mc_pointTransactionLabel",
        &name,
        depth,
        Props::default(),
        false,
        &ov,
    ) {
        t.label = Some(Label {
            entity: e,
            path: format!("{}/{name}", t.path),
            announcement,
            age: 0.0,
        });
    }
    if let Some(sfx) = sfx.as_mut() {
        sfx.play(SND_REWARD);
    }
}

/// mc_pointTransaction's fadeOut: its label goes (the label's fadeOut removes it),
/// and the clip rises and fades away (hud_logic, then removed).
fn fade_transaction(points: &mut Points, sp: &mut Spawner, mut t: Transaction) {
    if let Some(label) = t.label.take() {
        sp.commands.entity(label.entity).try_despawn();
    }
    points.fading.push(Fading {
        entity: t.entity,
        path: t.path,
        age: 0.0,
    });
}

/// A screen effect drawn in hud_mc (the game's are post processes, over the whole
/// screen) `size` stage units across the screen's middle, however hud_mc is drawn:
/// its place, scale (stage units, for a unit-square bitmap) and rotation (degrees)
/// in hud_mc, undoing the root's zoom (`zoom`, squashed in the car) and the camera
/// shake's jolt to hud_mc (layout_hud: 8 stage units a degree of yaw and of pitch,
/// a tenth of a degree a degree of roll).
fn on_screen(size: Vec2, zoom: Vec2, shake: Vec3) -> (Vec2, Vec2, f32) {
    let zoom = zoom.max(Vec2::splat(0.1));
    let jolt = Vec2::new(shake.y, shake.x) * 8.0;
    let turn = -shake.z * 0.1;
    // (Back by the jolt, as hud_mc's turn would carry it.)
    let at = Vec2::from_angle(turn.to_radians()).rotate(-jolt);
    (at, size / zoom, turn)
}

#[allow(clippy::too_many_arguments)]
fn hud_logic(
    mut commands: Commands,
    (time, pass): (Res<Time>, Res<HudPass>),
    lib: Option<Res<HudLib>>,
    mut huds: crate::split::BySeat<HudState>,
    index: Res<HudIndex>,
    stage: Res<HudStage>,
    mut logic: Local<Logic>,
    roots: Query<(Entity, &crate::split::Seat), With<HudRoot>>,
    mut nodes: LogicQuery,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<HudMaterial>>,
    mut sfx: Option<ResMut<crate::sound::Sfx>>,
) {
    // (This pass's HUD's state: split screen's guest's in his.)
    let state = huds.seat(pass.0);
    let root = roots.iter().find(|(_, s)| s.0 == pass.0).map(|(e, _)| e);
    if logic.root != root {
        *logic = Logic { root, ..default() };
    }
    let Some(lib) = lib else {
        state.events.clear();
        return;
    };
    // The first layout pass names the nodes.
    if !index.0.contains_key("hud_mc") {
        return;
    }
    let lib: &HudLib = &lib;
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    let at = |path: &str| index.0.get(path).copied();
    let none = HashMap::new();
    let mut sp = Spawner {
        commands: &mut commands,
        lib,
        meshes: &mut meshes,
        materials: &mut materials,
        placed: HashMap::new(),
        layer: hud_layer(pass.0),
    };

    // -- health and shield (bottomLeft: updateShieldHealth) --
    let frac = (state.health / state.max_health.max(1.0)).clamp(0.0, 1.0);
    let has_shield = state.max_shield > 0.0;
    let shield = state.shield.max(0.0);
    if !logic.started {
        logic.health_shown = state.health;
        logic.shield_shown = shield;
    }
    with(&mut nodes, at(paths::SHIELD_BAR), |p| {
        p.hidden = !has_shield
    });
    if has_shield {
        let s = (shield / state.max_shield).clamp(0.0, 1.0);
        let frame = ((s * 100.0).ceil() as usize).clamp(1, 100) - 1;
        goto(&mut nodes, lib, at(paths::SHIELD_BAR), Ok(frame), false);
    }
    let frame = ((frac * 100.0).ceil() as usize).clamp(1, 100) - 1;
    goto(&mut nodes, lib, at(paths::HEALTH_BAR), Ok(frame), false);
    let shown = format!("{}", state.health.max(0.0).ceil() as i32);
    with(&mut nodes, at(paths::HEALTH_NUMBER), |p| {
        p.text = Some(shown)
    });
    if (state.health - logic.health_shown).abs() > 0.01 {
        // Each change flashes the bar white and squashes it back over 0.2 s.
        logic.health_flash = 0.2;
        logic.health_shown = state.health;
    }
    // With the shield down, under half health (three quarters with a shield to
    // lose): red bars, and the glows pulse red. Once that's over, an empty shield's
    // bar stays red.
    let warning = (frac < 0.5 || (frac < 0.75 && has_shield)) && shield <= 0.0;
    if warning != logic.health_warning {
        logic.health_warning = warning;
        logic.shield_red = warning || (has_shield && shield <= 0.0);
        let label = if warning { "low" } else { "normal" };
        goto(&mut nodes, lib, at(paths::HEALTH_GLOW), Err(label), warning);
        goto(&mut nodes, lib, at(paths::EDGE_LEFT), Err(label), true);
    }
    logic.health_flash = (logic.health_flash - dt).max(0.0);
    let f = ease_out(1.0 - logic.health_flash / 0.2);
    let base = if warning { RED } else { NORMAL };
    with(&mut nodes, at(paths::HEALTH_BAR), |p| {
        p.cx = Some(WHITE.lerp(base, f));
        p.yscale = Some(2.0 - f);
    });
    // The shield filling up again flashes its bar white at four times its height,
    // easing back over 0.3 s (once each time it starts to fill).
    if (shield - logic.shield_shown).abs() > 0.01 {
        if shield > logic.shield_shown {
            if !logic.shield_filling {
                logic.shield_filling = true;
                logic.shield_flash = 0.3;
                logic.shield_red = warning;
            }
        } else {
            logic.shield_filling = false;
        }
        logic.shield_shown = shield;
    }
    logic.shield_flash = (logic.shield_flash - dt).max(0.0);
    let f = ease_out(1.0 - logic.shield_flash / 0.3);
    let base = if logic.shield_red { RED } else { NORMAL };
    with(&mut nodes, at(paths::SHIELD_BAR), |p| {
        p.cx = Some(WHITE.lerp(base, f));
        p.yscale = Some(4.0 - 3.0 * f);
    });

    // -- the low-health screen effect (LowHealthScreenEffect: tunnel vision and
    // red scanlines) --
    if logic.low_health.is_none() {
        if let Some(hud) = at("hud_mc") {
            let mask = sp.bitmap(hud, "LowHealthVignette", Vec2::ONE, Vec2::ZERO, -2, false);
            let scan = sp.bitmap(hud, "LowHealthScanlines", Vec2::ONE, Vec2::ZERO, -1, true);
            if let (Some(m), Some(s)) = (mask, scan) {
                logic.low_health = Some((m, s));
            }
        }
    }
    if let Some((mask, scan)) = logic.low_health {
        // On below 75% health with the shield down (the movie's HealthDanger),
        // strengthening as it falls; off while down, dead or watching
        // (updateHudVisibility's DeactivatePostProcess).
        let off = state.dead || state.downed.is_some() || state.spectating.is_some();
        let danger = if off || shield > 0.0 {
            0.0
        } else {
            ((0.75 - frac) / 0.5).clamp(0.0, 1.0)
        };
        // (The game's is a post process, over the whole screen: drawn in hud_mc, it
        // undoes hud_mc's zoom and shake, so it fills the screen in the car too.)
        let size = Vec2::new(stage.0, stage.2) * 1.02;
        let (at, scale, turn) = on_screen(size, stage.1, state.camera_shake);
        let pulse = 0.04 * (t * TAU / 0.4).sin();
        // The game multiplies the scene by clamp(mask + 0.3 + pulse); drawn over the
        // scene that is black with alpha 0.7 - mask - pulse. LowHealthVignette's
        // alpha is 1 - mask.
        with(&mut nodes, Some(mask), |p| {
            p.hidden = danger <= 0.0;
            p.x = Some(at.x);
            p.y = Some(at.y);
            p.rotation = Some(turn);
            p.xscale = Some(scale.x / 128.0);
            p.yscale = Some(scale.y / 128.0);
            p.cx = Some(Cx {
                mul: [0.0, 0.0, 0.0, danger],
                add: [0.0, 0.0, 0.0, -(0.3 + pulse) * danger],
            });
        });
        // Red scanlines (colour 1, 0.04, 0.06), flickering like the material's
        // panning noise.
        let flicker = 0.7 + 0.3 * (t * 23.0).sin() * (t * 7.3).cos();
        with(&mut nodes, Some(scan), |p| {
            p.hidden = danger <= 0.0;
            p.x = Some(at.x);
            p.y = Some(at.y);
            p.rotation = Some(turn);
            p.xscale = Some(scale.x / 128.0);
            // (Its 80 lines down the screen exactly, as the material tiles them.)
            p.yscale = Some(scale.y / 1.02 / 1280.0);
            p.cx = Some(Cx {
                mul: [1.0, 0.04, 0.06, danger * flicker],
                add: [0.0; 4],
            });
        });
    }

    // -- weapon, ammo and equipment (bottomRight: updateCurrentWeapon,
    // updateAmmoBarClip, updateAmmoDisplay) --
    let cap = state.clip_size.max(1);
    let weapon_changed = logic.weapon != Some(state.weapon);
    if weapon_changed {
        logic.weapon = Some(state.weapon);
        // The name for 6 s (weapons without one, like the car's, show none).
        if state.weapon.label.is_empty() {
            goto(&mut nodes, lib, at(paths::WEAPON_LABEL), Ok(0), false);
            logic.label_timer = 0.0;
        } else {
            let name = state.weapon.label.to_uppercase();
            with(&mut nodes, at(paths::WEAPON_LABEL_TEXT), |p| {
                p.text = Some(name)
            });
            goto(&mut nodes, lib, at(paths::WEAPON_LABEL), Err("on"), true);
            logic.label_timer = 6.0;
        }
        // Icon: ItemWireframe<weapon>, mirrored at 65%, sliding in.
        if let Some(old) = logic.icon.take() {
            sp.commands.entity(old).despawn();
        }
        if let (Some(anchor), Some(icon)) = (at(paths::WEAPON_ICON), state.weapon.icon) {
            if let Some(clip) = sp.attach(
                anchor,
                "mc_currentWeaponIcon",
                "currentWeaponIcon_mc",
                ATTACH,
                Props::default(),
                false,
                &none,
            ) {
                logic.icon = Some(clip);
                // The bitmap goes in the clip's own iconAnchor_mc; it is added next
                // frame, once that exists.
                logic.pending_icon = Some(icon);
            }
        }
    }
    // Ammo bar: a segment per round for clips of up to 40, else a solid bar; made
    // again for a new weapon or a new clip size (an upgrade's: AmmoBarIsSegmented's
    // watch, mc_ammoBarSegmented's updateAmmo as WeaponClipCapacity changes).
    if weapon_changed || logic.bar_cap != cap {
        logic.bar_cap = cap;
        if let Some(old) = logic.ammo_bar.take() {
            sp.commands.entity(old).despawn();
        }
        logic.segmented = cap <= 40;
        logic.segment_count = 0;
        logic.special_shown.clear();
        if let Some(anchor) = at(paths::AMMO_ANCHOR) {
            if logic.segmented {
                logic.ammo_bar = sp.attach(
                    anchor,
                    "mc_ammoBarSegmented",
                    "ammoBar_mc",
                    ATTACH,
                    Props::default(),
                    false,
                    &none,
                );
                if let Some(holder) = logic.ammo_bar {
                    // AmmoSegmentTotalWidth 170, AmmoSegmentPadding -15; shading from
                    // white to light blue.
                    let n = cap as f32;
                    let width = (170.0 + 15.0 * n) / n;
                    let step = width - 15.0;
                    let mut ov = HashMap::new();
                    for part in ["background_mc", "highlight_mc"] {
                        ov.insert(
                            part.to_string(),
                            Props {
                                xscale: Some(width / 32.0),
                                ..Default::default()
                            },
                        );
                    }
                    for i in 0..cap {
                        let shade = if cap > 1 {
                            i as f32 / (cap - 1) as f32
                        } else {
                            0.0
                        };
                        let set = Props {
                            x: Some(-(i as f32) * step),
                            cx: Some(NORMAL.lerp(LIGHT_BLUE, shade)),
                            ..Default::default()
                        };
                        sp.attach(
                            holder,
                            "mc_ammoBarSegment",
                            &format!("segment{i}_mc"),
                            ATTACH + i as i32,
                            set,
                            false,
                            &ov,
                        );
                    }
                    logic.segment_count = cap;
                }
            } else {
                logic.ammo_bar = sp.attach(
                    anchor,
                    "mc_ammoBarSolid",
                    "ammoBar_mc",
                    ATTACH,
                    Props::default(),
                    false,
                    &none,
                );
            }
        }
    }
    if weapon_changed {
        // Crosshair: the weapon's reticle (the rifle's for the Neutron Assault Rifle,
        // the Subsonic Repeater and the car; the tactical one for the Photon Burst
        // Rifle; the shotgun one for the Scatter Blaster; the pistol one for the Path
        // Blaster; none for the Nucleon Charge Rifle), spinning in from 60 degrees
        // when a weapon is drawn.
        if let Some(old) = logic.crosshair.take() {
            sp.commands.entity(old).despawn();
        }
        if let (Some(anchor), false) = (
            at(paths::CROSSHAIR_ANCHOR),
            state.weapon.crosshair.is_empty(),
        ) {
            // (The mortar and saw-blade reticles play their own way in, animIn:
            // their timeline, or straight to its end, gotoAndStop(_totalframes),
            // when the weapon hasn't changed: the match's first.)
            let timeline = matches!(
                state.weapon.crosshair,
                MORTAR_CROSSHAIR | SAWBLADE_CROSSHAIR
            );
            let spins = logic.started && !timeline;
            let set = Props {
                rotation: Some(if spins { 60.0 } else { 0.0 }),
                ..Default::default()
            };
            logic.crosshair = sp.attach(
                anchor,
                state.weapon.crosshair,
                "crosshair_mc",
                ATTACH,
                set,
                false,
                &none,
            );
            logic.crosshair_spin = if spins { 0.3 } else { 0.0 };
            logic.crosshair_end = timeline && !logic.started;
        }
        logic.reload_left = 0.0;
        logic.clip_shown = state.clip;
    }
    // The icon's bitmap, once its clip is in place.
    // (The layout pass has to have seen the new clip: until then these paths still
    // lead into the old one, which is gone.)
    let icon_clip = at(&format!("{}/currentWeaponIcon_mc", paths::WEAPON_ICON));
    if let (Some(icon), true) = (
        logic.pending_icon,
        icon_clip.is_some() && icon_clip == logic.icon,
    ) {
        if let Some(a) = at(&format!(
            "{}/currentWeaponIcon_mc/iconAnchor_mc",
            paths::WEAPON_ICON
        )) {
            sp.bitmap(a, icon, Vec2::new(-0.65, 0.65), Vec2::ZERO, ATTACH, false);
            logic.pending_icon = None;
        }
    }
    if logic.label_timer > 0.0 {
        logic.label_timer -= dt;
        if logic.label_timer <= 0.0 && frame_of(&nodes, at(paths::WEAPON_LABEL)).unwrap_or(0) > 0 {
            goto(&mut nodes, lib, at(paths::WEAPON_LABEL), Err("off"), true);
        }
    }
    // The number is everything left: the clip and the reserve.
    let total = state.clip + state.reserve;
    with(&mut nodes, at(paths::AMMO_NUMBER), |p| {
        p.text = Some(total.to_string())
    });
    let frame = ((state.clip as f32 / cap as f32 * 100.0).ceil() as usize).clamp(1, 100) - 1;
    goto(&mut nodes, lib, at(paths::AMMO_BAR), Ok(frame), false);
    if logic.segmented {
        // Each segment's highlight shows a round still in the clip; the special
        // shot's flashes (mc_ammoBarSegment's updateSpecialShot: its highlight plays
        // "flashing" as it becomes the special one and "normal" as it stops being
        // it, as updateAmmo sets each one's specialShot).
        let count = logic.segment_count as usize;
        logic.special_shown.resize(count, None);
        for i in 0..logic.segment_count {
            let path = format!(
                "{}/ammoBar_mc/segment{i}_mc/highlight_mc",
                paths::AMMO_ANCHOR
            );
            let highlight = at(&path);
            let hidden = i >= state.clip;
            with(&mut nodes, highlight, |p| p.hidden = hidden);
            let special = state.special_shot == Some(i);
            let shown = &mut logic.special_shown[i as usize];
            if highlight.is_some() && *shown != Some(special) {
                // (Its first value only plays when it is the special one: false
                // for one never set is no change to its watch.)
                if special || shown.is_some() {
                    let label = if special { "flashing" } else { "normal" };
                    goto(&mut nodes, lib, highlight, Err(label), true);
                }
                *shown = Some(special);
            }
        }
    } else {
        let bar = at(&format!("{}/ammoBar_mc/bar_mc", paths::AMMO_ANCHOR));
        let frame = ((state.clip as f32 / cap as f32 * 200.0).ceil() as usize).clamp(1, 200) - 1;
        goto(&mut nodes, lib, bar, Ok(frame), false);
    }
    // Low ammo (updateAmmoDisplay's LowAmmo): no more than a clip's worth left in
    // all (a quarter of a clip, for a gun with no reserve); and a low clip
    // (LowClip): a quarter of it or less left in it.
    let low = low_ammo(state.clip, state.reserve, cap, state.reserve_max);
    let low_clip = state.clip as f32 / cap as f32 <= LOW_CLIP;
    if low != logic.low_ammo {
        logic.low_ammo = low;
        let label = if low { "low" } else { "normal" };
        goto(&mut nodes, lib, at(paths::AMMO_GLOW), Err(label), low);
        goto(&mut nodes, lib, at(paths::EDGE_RIGHT), Err(label), true);
        with(&mut nodes, at(paths::WEAPON), |p| {
            p.cx = Some(if low { RED } else { NORMAL })
        });
    }
    // A fuller clip (a reload) flashes the bar.
    if state.clip > logic.clip_shown {
        logic.ammo_flash = 0.2;
    }
    logic.clip_shown = state.clip;
    logic.ammo_flash = (logic.ammo_flash - dt).max(0.0);
    let f = ease_out(1.0 - logic.ammo_flash / 0.2);
    // The bar and its number go red with either (the gun's name only with low
    // ammo: above).
    let base = if low || low_clip { RED } else { NORMAL };
    with(&mut nodes, at(paths::AMMO_BAR), |p| {
        p.cx = Some(WHITE.lerp(base, f))
    });

    // -- equipment (equipment_mc's offensive_mc and defensive_mc: updateEquipment,
    // updateEquipmentState; the ammo display's setEquipmentVisibility shows them in
    // robot form only) --
    with(&mut nodes, at(paths::EQUIPMENT), |p| {
        p.alpha = Some(if state.vehicle { 0.0 } else { 1.0 })
    });
    for (slot, root) in paths::EQUIPMENT_SLOTS.into_iter().enumerate() {
        let path = |p: &str| format!("{root}/{p}");
        let item = state.equipment[slot];
        // The defensive slot's icon is on the other side of its key (its
        // iconAnchor_mc._x and arrow_mc._xscale turned round).
        if slot == 1 {
            with(&mut nodes, at(&path("iconAnchor_mc")), |p| p.x = Some(30.0));
            with(&mut nodes, at(&path("arrow_mc")), |p| p.xscale = Some(-1.0));
        }
        // The key's name in its box, or with a controller the D-pad's arrow.
        let pad = state.pad;
        with(&mut nodes, at(&path("arrow_mc")), |p| {
            p.hidden = item.is_none() || !pad
        });
        with(&mut nodes, at(&path("key_mc")), |p| {
            p.hidden = item.is_none() || pad
        });
        let key = state.equipment_keys[slot].0.clone();
        with(&mut nodes, at(&path("key_mc/txt")), |p| p.text = Some(key));
        // The icon: AbilityIcon<name> at 60%, centred.
        let icon = item.map(|e| e.icon);
        if logic.equipment_icons[slot].map(|(_, i)| i) != icon {
            if let Some((e, _)) = logic.equipment_icons[slot].take() {
                sp.commands.entity(e).try_despawn();
            }
            if let (Some(anchor), Some(icon)) = (at(&path("iconAnchor_mc")), icon) {
                logic.equipment_icons[slot] = sp
                    .bitmap(anchor, icon, Vec2::splat(0.6), Vec2::ZERO, ATTACH, false)
                    .map(|e| (e, icon));
            }
        }
        // Ready: its own colours and the icon at full size; else red, at 90%.
        let ready = item.is_none_or(|e| e.ready);
        with(&mut nodes, at(root), |p| {
            p.cx = Some(if ready { NORMAL } else { RED })
        });
        let s = if ready { 1.0 } else { 0.9 };
        with(&mut nodes, at(&path("iconAnchor_mc")), |p| {
            p.xscale = Some(s);
            p.yscale = Some(s);
        });
    }

    // -- ability (robotAbility_mc: updateAbility, updateAbilityState, updateRbIcon;
    // ability_mc's updateCurrentForm shows it in robot form only) --
    let ability = state.ability.clone().filter(|_| !state.vehicle);
    with(&mut nodes, at(paths::ABILITY), |p| {
        p.alpha = Some(if ability.is_some() { 1.0 } else { 0.0 })
    });
    if let Some(a) = ability {
        let root = paths::ROBOT_ABILITY;
        let path = |p: &str| format!("{root}/{p}");
        // The icon: AbilityIcon<name>, three quarters its size, centred; or the
        // movie clip checkForComplexIcon puts in its place (the Heal Beam's and the
        // Hover's), as it is.
        if logic.ability_icon.map(|(_, icon)| icon) != Some(a.icon) {
            if let Some((e, _)) = logic.ability_icon.take() {
                sp.commands.entity(e).try_despawn();
            }
            if let Some(anchor) = at(&path("iconAnchor_mc")) {
                logic.ability_icon = if a.icon.starts_with("mc_") {
                    sp.attach(
                        anchor,
                        a.icon,
                        "abilityIcon_mc",
                        ATTACH,
                        Props::default(),
                        false,
                        &none,
                    )
                } else {
                    sp.bitmap(anchor, a.icon, Vec2::splat(0.75), Vec2::ZERO, ATTACH, false)
                }
                .map(|e| (e, a.icon));
                logic.ability_arrow = 0.0;
                logic.ability_juice = 0.0;
            }
        }
        // The cancel icon (its alpha by abilityActive, for the abilities that show it).
        with(&mut nodes, at(&path("cancel_mc")), |p| {
            p.alpha = Some(if a.cancel { 1.0 } else { 0.0 })
        });
        // The Hover's icon: its arrow turns down while it is going (tweenTo 0.8 s,
        // Strong.easeOut).
        if a.icon == "mc_abilityIconHover" {
            let to = if a.active { -180.0 } else { 0.0 };
            logic.ability_arrow += (to - logic.ability_arrow) * (dt * 6.0).min(1.0);
            let r = logic.ability_arrow;
            with(
                &mut nodes,
                at(&path("iconAnchor_mc/abilityIcon_mc/arrow_mc")),
                |p| p.rotation = Some(r),
            );
        }
        // The Heal Beam's icon: its progress_mc at the juice's frame
        // (updateHealBeamJuice: gotoAndStop(ceil(juice * _totalframes))), its
        // flash_mc flashing as the juice starts over.
        if a.icon == "mc_abilityIconHealBeam" {
            let progress = at(&path("iconAnchor_mc/abilityIcon_mc/progress_mc"));
            if let Some(total) = progress
                .and_then(|e| nodes.get(e).ok())
                .and_then(|(n, _)| lib.sprite(n.def))
                .map(|s| s.frames)
            {
                let f = (a.juice * total as f32).ceil() as usize;
                goto(&mut nodes, lib, progress, Ok(f.max(1) - 1), false);
            }
            if a.juice < logic.ability_juice {
                goto(
                    &mut nodes,
                    lib,
                    at(&path("iconAnchor_mc/abilityIcon_mc/flash_mc")),
                    Err("flash"),
                    true,
                );
            }
            logic.ability_juice = a.juice;
        }
        // The key (pc_all: its key_mc with the key's name; with a controller, xbox:
        // its RB).
        let frame = if state.pad { "xbox" } else { "pc_all" };
        goto(&mut nodes, lib, at(&path("rb_mc")), Err(frame), false);
        let key = a.key.clone();
        with(&mut nodes, at(&path("rb_mc/key_mc/txt")), |p| {
            p.text = Some(key)
        });
        // The meter: the highlight shown round as far as it is full.
        let fill = a.meter.clamp(0.0, 1.0);
        with(&mut nodes, at(&path("meter_mc/highlight_mc")), |p| {
            p.pie = Some(fill)
        });
        // Jammed, ready (enough for a use, and not in use) or not
        // (updateAbilityState's states 3, 0 and 1): its colours, size and glow.
        let ready = fill + 1e-4 >= a.cost && !a.in_use && !a.jammed;
        let tint = if a.jammed {
            AMBER
        } else if ready {
            NORMAL
        } else {
            LIGHT_BLUE
        };
        for part in [
            "meter_mc/highlight_mc",
            "meter_mc/background_mc",
            "iconAnchor_mc",
            "glow_mc",
        ] {
            with(&mut nodes, at(&path(part)), |p| p.cx = Some(tint));
        }
        if !logic.ability_shown || ready != logic.ability_ready {
            let s = if ready { 1.0 } else { 0.9 };
            with(&mut nodes, at(root), |p| {
                p.xscale = Some(s);
                p.yscale = Some(s);
            });
            let label = if ready { "on" } else { "spinloop" };
            goto(&mut nodes, lib, at(&path("glow_mc")), Err(label), true);
            logic.ability_ready = ready;
            logic.ability_shown = true;
        }
        // JAMMED under it as it is jammed, gone as that ends (jammed_mc: on, off).
        if a.jammed != logic.ability_jammed {
            let label = if a.jammed { "on" } else { "off" };
            goto(&mut nodes, lib, at(&path("jammed_mc")), Err(label), true);
            logic.ability_jammed = a.jammed;
        }
        with(&mut nodes, at(&path("glow_mc")), |p| {
            p.alpha = Some(if ready { 1.0 } else { 0.0 })
        });
    }

    // -- crosshair (midCenter: updateCrosshair, updateCrosshairColor,
    // updateWeaponSpread) --
    if let Some(cross) = logic.crosshair {
        // Spread pushes the three prongs out (SpreadMultiplier 500, 0.2 s ease, as
        // the rifle's and rocket reticles' scripts have it; the pistol and cannon
        // reticles' use 400). The shotgun reticle's uses
        // 400 too and also stretches each arc to 10 + spread x 400 pixels across
        // (its _width).
        let shotgun = state.weapon.crosshair == SHOTGUN_CROSSHAIR;
        let mortar = state.weapon.crosshair == MORTAR_CROSSHAIR;
        let sawblade = state.weapon.crosshair == SAWBLADE_CROSSHAIR;
        let multiplier = if mortar {
            300.0
        } else if shotgun
            || sawblade
            || state.weapon.crosshair == PISTOL_CROSSHAIR
            || state.weapon.crosshair == CANNON_CROSSHAIR
        {
            400.0
        } else {
            500.0
        };
        let target = -state.spread * multiplier;
        logic.prong += (target - logic.prong) * (dt / 0.07).min(1.0);
        let anchor = paths::CROSSHAIR_ANCHOR;
        let r = logic.prong;
        if mortar || sawblade {
            // Sideways: the prongs' _x (the saw-blade's stretched to 90 - r
            // percent), the mortar's lines a fifth as far.
            for prong in ["prong1_mc", "prong2_mc"] {
                let path = format!("{anchor}/crosshair_mc/{prong}/prong_mc");
                with(&mut nodes, at(&path), |p| {
                    p.x = Some(r);
                    if sawblade {
                        p.yscale = Some((90.0 - r) / 100.0);
                    }
                });
            }
            if mortar {
                for lines in ["lines1_mc", "lines2_mc"] {
                    let path = format!("{anchor}/crosshair_mc/{lines}/lines_mc");
                    with(&mut nodes, at(&path), |p| p.x = Some(r * 0.2));
                }
            }
        } else {
            for prong in ["prong1_mc", "prong2_mc", "prong3_mc"] {
                let path = format!("{anchor}/crosshair_mc/{prong}/prong_mc");
                with(&mut nodes, at(&path), |p| {
                    p.y = Some(r);
                    if shotgun {
                        p.xscale = Some((10.0 - r) / SHOTGUN_ARC_WIDTH);
                    }
                });
            }
        }
        // (Straight to the end of its timeline, once its clip is in.)
        if logic.crosshair_end && nodes.contains(cross) {
            goto(&mut nodes, lib, Some(cross), Ok(usize::MAX), false);
            logic.crosshair_end = false;
        }
        logic.crosshair_spin = (logic.crosshair_spin - dt).max(0.0);
        let spin = 60.0 * (1.0 - ease_out(1.0 - logic.crosshair_spin / 0.3));
        let colour = if state.reloading {
            CROSSHAIR_BLUE
        } else if state.on_target {
            CROSSHAIR_RED
        } else {
            NORMAL
        };
        with(&mut nodes, Some(cross), |p| {
            p.rotation = Some(spin);
            p.cx = Some(colour);
        });
    }

    // -- reload ring (reloading_mc: fills over the reload, then fades) --
    for ev in &state.events {
        if let HudEvent::Reload(secs) = ev {
            logic.reload_left = *secs;
            logic.reload_time = secs.max(0.01);
        }
    }
    if logic.ring.is_none() {
        if let Some(r) = at(paths::RELOADING) {
            // DrawCircleSegment: radius 30, 1.5 px, #00FFD4.
            logic.ring = Some(sp.ring(r, 30.0, 1.5, [0.0, 1.0, 212.0 / 255.0, 1.0], ATTACH));
        }
    }
    if !state.reloading {
        logic.reload_left = 0.0;
    }
    logic.reload_left = (logic.reload_left - dt).max(0.0);
    let reloading = state.reloading && logic.reload_time > 0.0;
    let fill = if reloading {
        1.0 - logic.reload_left / logic.reload_time
    } else {
        0.0
    };
    with(&mut nodes, at(paths::RELOADING), |p| {
        p.alpha = Some(if reloading { 1.0 } else { 0.0 });
        let s = if reloading { 1.0 } else { 0.9 };
        p.xscale = Some(s);
        p.yscale = Some(s);
    });
    with(&mut nodes, logic.ring, |p| p.value = Some(fill));

    // -- ammo warnings (right of the crosshair: updateAmmoWarnings) --
    let warning = if total == 0 {
        Some("mc_ammoWarningEmpty")
    } else if low {
        Some("mc_ammoWarningLow")
    } else {
        None
    };
    if logic.warning.map(|w| w.0) != warning {
        if let Some((_, e)) = logic.warning.take() {
            sp.commands.entity(e).despawn();
        }
        if let (Some(id), Some(anchor)) = (warning, at(paths::WARNINGS)) {
            let set = Props {
                y: Some(15.0),
                ..Default::default()
            };
            // Attached with blendMode "add".
            if let Some(e) = sp.attach(anchor, id, "ammoWarning_mc", ATTACH, set, true, &none) {
                logic.warning = Some((id, e));
            }
        }
    }
    // "RELOAD" with the R key, half a second after the clip runs low.
    let want_prompt = !state.reloading
        && state.reserve > 0
        && cap > 1
        && (state.clip as f32 / cap as f32) <= 0.25;
    if want_prompt {
        logic.prompt_in += dt;
        if logic.prompt.is_none() && logic.prompt_in >= 0.5 {
            if let Some(anchor) = at(paths::WARNINGS) {
                logic.prompt = sp.attach(
                    anchor,
                    "mc_ammoWarningReload",
                    "ammoReload_mc",
                    ATTACH + 1,
                    Props::default(),
                    false,
                    &none,
                );
            }
        }
    } else {
        logic.prompt_in = 0.0;
        if let Some(e) = logic.prompt.take() {
            sp.commands.entity(e).despawn();
        }
    }
    if logic.prompt.is_some() {
        // reloadIcon_mc's third frame is the keyboard key; with a controller
        // (setReloadIcon: PreferGamepad), its first, the X button.
        let icon = format!("{}/ammoReload_mc/reloadIcon_mc", paths::WARNINGS);
        let frame = if state.pad { 0 } else { 2 };
        goto(&mut nodes, lib, at(&icon), Ok(frame), false);
        with(&mut nodes, at(&format!("{icon}/key_mc/txt")), |p| {
            p.text = Some("R".to_string())
        });
    }

    // -- hit pings, damage arrows, ammo pickups --
    let events = std::mem::take(&mut state.events);
    for ev in &events {
        match ev {
            HudEvent::Hit => {
                if let Some(anchor) = at(paths::PINGS) {
                    if let Some(e) = sp.attach(
                        anchor,
                        "mc_causedDamagePing",
                        "",
                        ATTACH,
                        Props::default(),
                        false,
                        &none,
                    ) {
                        sp.commands.entity(e).insert(RemoveAtEnd);
                    }
                }
            }
            HudEvent::Damaged { yaw, amount } => {
                if let Some(anchor) = at(paths::DAMAGE) {
                    let set = Props {
                        rotation: Some(yaw.to_degrees()),
                        ..Default::default()
                    };
                    // glow_mc.glow_mc._alpha = damageAmount.
                    let mut ov = HashMap::new();
                    ov.insert(
                        "glow_mc/glow_mc".to_string(),
                        Props {
                            alpha: Some((amount * 3.0).clamp(0.25, 1.0)),
                            ..Default::default()
                        },
                    );
                    if let Some(e) = sp.attach(
                        anchor,
                        "mc_damageIndicatorArrow",
                        "",
                        ATTACH,
                        set,
                        false,
                        &ov,
                    ) {
                        sp.commands.entity(e).insert(RemoveAtEnd);
                    }
                }
            }
            HudEvent::AmmoAdded {
                icon,
                amount,
                current,
            } => {
                if let Some(anchor) = at(paths::AMMO_ADDED) {
                    let slot = logic
                        .notes
                        .iter()
                        .filter(|n| matches!(n.kind, NoteKind::Ammo { .. }))
                        .count();
                    let set = Props {
                        x: Some(-20.0),
                        y: Some(-24.0 * slot as f32),
                        cx: Some(WHITE),
                        ..Default::default()
                    };
                    let mut ov = HashMap::new();
                    ov.insert(
                        "text_txt".to_string(),
                        Props {
                            text: Some(format!("+{amount}")),
                            ..Default::default()
                        },
                    );
                    if let Some(e) = sp.attach(
                        anchor,
                        "mc_ammoAddedNotification",
                        "",
                        ATTACH + slot as i32,
                        set,
                        false,
                        &ov,
                    ) {
                        if let Some(icon) = icon {
                            // The weapon's small icon (its GameMessageIcon: main.rs's
                            // ammo_icon), mirrored at 80%, left of the number.
                            let w = lib.images.get(*icon).map(|i| i.1.x).unwrap_or(0.0) * 0.8;
                            sp.bitmap(
                                e,
                                icon,
                                Vec2::new(-0.8, 0.8),
                                Vec2::new(-w / 2.0, 0.0),
                                ATTACH,
                                false,
                            );
                        }
                        logic.notes.push(Note {
                            entity: e,
                            age: 0.0,
                            kind: NoteKind::Ammo {
                                slot,
                                current: *current,
                            },
                        });
                    }
                }
            }
            HudEvent::GameAnnouncement(text) => {
                logic.game_announcement = Some((text.to_uppercase().chars().collect(), 0.0));
            }
            HudEvent::RewardAnnouncement(text) => {
                logic.reward_announcement = Some((text.to_uppercase().chars().collect(), 0.0));
            }
            HudEvent::Points {
                amount,
                announcement,
                description,
                transaction,
            } => {
                // (updatePointEvent: queued, the queue run if it isn't running.)
                logic.points.queue.push_back(PointAward {
                    amount: *amount,
                    announcement: announcement.clone(),
                    description: description.clone(),
                    transaction: *transaction,
                });
                if !logic.points.running {
                    update_transactions(&mut logic.points, &mut sp, at(paths::POINTS), &mut sfx);
                }
            }
            HudEvent::LevelUp { level, class } => {
                // (NotifyLevelUp: a notice still up plays out from "fadeOut", the new
                // one attached over it. Its stinger is xp.rs's.)
                if let Some(old) = logic.level_up.take() {
                    goto(&mut nodes, lib, Some(old.entity), Err("fadeOut"), true);
                }
                if let Some(anchor) = at(paths::LEVEL_UP) {
                    let name = format!("levelUp{}_mc", logic.level_ups);
                    let depth = ATTACH + logic.level_ups as i32;
                    logic.level_ups += 1;
                    let text = level.to_string();
                    let mut ov = HashMap::new();
                    ov.insert(
                        "level_mc/level_txt".to_string(),
                        Props {
                            text: Some(text.clone()),
                            ..Default::default()
                        },
                    );
                    let set = Props::default();
                    if let Some(e) =
                        sp.attach(anchor, "mc_levelUpNotify", &name, depth, set, false, &ov)
                    {
                        sp.commands.entity(e).insert((RemoveAtEnd, Loops(2)));
                        let specialty = crate::customize::SPECIALTIES.get(*class);
                        logic.level_up = Some(LevelUpNotice {
                            entity: e,
                            path: format!("{}/{name}", paths::LEVEL_UP),
                            seen: false,
                            level: text,
                            icon: specialty.map(|s| format!("mc_specialtyIcon{s}")),
                            sparkles: false,
                        });
                    }
                }
            }
            HudEvent::ItemUnlocked { image, name } => {
                // (updateStoreItemUnlocked: the one up is taken away and the new one
                // attached in its place, all of it in BlueColor.)
                if let Some(old) = logic.item_unlocked.take() {
                    sp.commands.entity(old.entity).try_despawn();
                }
                if let Some(anchor) = at(paths::ITEM_UNLOCKED) {
                    let set = Props {
                        cx: Some(ITEM_BLUE),
                        ..Default::default()
                    };
                    if let Some(e) = sp.attach(
                        anchor,
                        "mc_itemUnlocked",
                        "itemUnlocked_mc",
                        ATTACH,
                        set,
                        false,
                        &none,
                    ) {
                        sp.commands.entity(e).insert(RemoveAtEnd);
                        logic.item_unlocked = Some(ItemPopup {
                            entity: e,
                            path: format!("{}/itemUnlocked_mc", paths::ITEM_UNLOCKED),
                            image,
                            picture: None,
                            name: name.to_uppercase().chars().collect(),
                            typing: None,
                            age: 0.0,
                            out: false,
                        });
                    }
                }
            }
            HudEvent::CurrencyNotify { low } => {
                // (NotifyInteractCurrency's revealMoney: the counter taller, white,
                // or red when there isn't enough, easing back over half a second, or
                // five.)
                logic.money_pop = if *low {
                    (MONEY_LOW_SECS, MONEY_LOW_SECS, MONEY_RED)
                } else {
                    (MONEY_POP_SECS, MONEY_POP_SECS, WHITE)
                };
            }
            HudEvent::GameMessage(parts) => {
                // (updateGameMessage: a line under the others, its parts side by side
                // in their colours, all at 60% when one is neutral and none the
                // player's: mc_gameMessage's initMessage; the oldest go once there
                // are more than it shows.)
                // (Its text field and where it sits in mc_gameMessageColor.)
                let field = lib
                    .symbols
                    .get("mc_gameMessageColor")
                    .and_then(|&d| lib.sprite(d))
                    .and_then(|s| s.key(1, 0))
                    .and_then(|k| match lib.defs.get(&k.child) {
                        Some(Def::Text(td)) => Some((td, k.m[4], k.m[5])),
                        _ => None,
                    });
                let text_def = field.map(|(td, _, _)| td);
                // (Its text's first baseline in the clip, and where it starts: past
                // the field's 2-pixel gutter, an ascent down: text_mesh.)
                let ascent = text_def
                    .and_then(|td| lib.face(None, &td.face).and_then(|f| lib.fonts.get(f)))
                    .zip(text_def)
                    .map_or(0.0, |(f, td)| f.ascent * td.size);
                let (text_x, baseline) = field.map_or((0.0, 0.0), |(td, tx, ty)| {
                    (tx + td.rect[0] + 2.0, ty + td.rect[1] + 2.0 + ascent)
                });
                if let Some(anchor) = at(paths::GAME_MESSAGES) {
                    let name = format!("message{}_mc", logic.message_count);
                    let depth = ATTACH + logic.message_count as i32;
                    logic.message_count += 1;
                    // (Under the others: the last place, once the oldest has gone.)
                    let set = Props {
                        y: Some(
                            MESSAGE_SPACING * logic.messages.len().min(MESSAGE_LINES - 1) as f32,
                        ),
                        ..Default::default()
                    };
                    if let Some(line) =
                        sp.attach(anchor, "mc_gameMessage", &name, depth, set, false, &none)
                    {
                        let neutral = !parts.iter().any(|(c, _)| *c == 0)
                            && parts.iter().any(|(c, _)| *c == 3);
                        // (Each part's middle on the line: _y = -_height / 2.)
                        let height = text_def.map_or(0.0, |td| td.rect[3] - td.rect[1]);
                        let mut x = 0.0;
                        for (k, (color, text)) in parts.iter().enumerate() {
                            // Its icons in place of their words (the kill feed's
                            // weapons: killfeed.rs). They make its line's ascent
                            // theirs, so its field is taller by the difference and,
                            // centred, its text lower by half of it; each piece its
                            // own clip in the part's colour, side by side.
                            let pieces = crate::killfeed::pieces(text);
                            let icons = pieces
                                .iter()
                                .any(|p| matches!(p, crate::killfeed::Piece::Icon(_)));
                            let lift = if icons {
                                (MESSAGE_ICON_BASELINE - ascent).max(0.0)
                            } else {
                                0.0
                            };
                            let cx = message_color(*color, state.faction_colors, state.decepticons);
                            for (n, piece) in pieces.iter().enumerate() {
                                let (shown, width, icon) = match *piece {
                                    crate::killfeed::Piece::Text(t) => (
                                        t.to_string(),
                                        text_def.map_or(0.0, |td| text_width(lib, td, t)),
                                        None,
                                    ),
                                    crate::killfeed::Piece::Icon(image) => {
                                        let size = lib
                                            .images
                                            .get(image)
                                            .map_or(Vec2::ZERO, |i| i.1 * MESSAGE_ICON_SCALE);
                                        (String::new(), size.x, Some((image, size)))
                                    }
                                };
                                let mut ov = HashMap::new();
                                ov.insert(
                                    "text_txt".to_string(),
                                    Props {
                                        text: Some(shown),
                                        ..Default::default()
                                    },
                                );
                                let set = Props {
                                    x: Some(x),
                                    y: Some((lift - height) / 2.0),
                                    cx: Some(cx),
                                    alpha: neutral.then_some(MESSAGE_NEUTRAL_ALPHA),
                                    ..Default::default()
                                };
                                let name = if n == 0 {
                                    format!("color{k}_mc")
                                } else {
                                    format!("color{k}_{n}_mc")
                                };
                                let clip = sp.attach(
                                    line,
                                    "mc_gameMessageColor",
                                    &name,
                                    ATTACH + (k * 16 + n) as i32,
                                    set,
                                    false,
                                    &ov,
                                );
                                if let (Some(clip), Some((image, size))) = (clip, icon) {
                                    sp.bitmap(
                                        clip,
                                        image,
                                        Vec2::splat(MESSAGE_ICON_SCALE),
                                        Vec2::new(
                                            text_x + size.x / 2.0,
                                            baseline - MESSAGE_ICON_BASELINE + size.y / 2.0,
                                        ),
                                        ATTACH,
                                        false,
                                    );
                                }
                                // (The next starts where it ends: its _width, its
                                // field's gutters, less 4.)
                                x += width;
                            }
                        }
                        logic.messages.push(MessageLine {
                            entity: line,
                            age: 0.0,
                        });
                        while logic.messages.len() > MESSAGE_LINES {
                            let old = logic.messages.remove(0);
                            sp.commands.entity(old.entity).try_despawn();
                        }
                        logic.messages_moved = true;
                    }
                }
            }
            HudEvent::Reload(_) => {}
        }
    }

    // -- point transactions (sprite 894: its queue runs on its interval) and the
    // transactions' own scripts: the "+N" of the one showing, its label's
    // announcement typing in, and those fading out (rising, then removed) --
    if let Some(left) = logic.points.next.as_mut() {
        *left -= dt;
        if *left <= 0.0 {
            update_transactions(&mut logic.points, &mut sp, at(paths::POINTS), &mut sfx);
        }
    }
    if let Some(t) = logic.points.shown.as_mut() {
        if t.total > 0 {
            let text = format!("+{}", t.total);
            with(
                &mut nodes,
                at(&format!("{}/points_mc/points_txt", t.path)),
                |p| p.text = Some(text),
            );
        }
        if let Some(label) = t.label.as_mut() {
            // (From when the field is in place.)
            let n = label.announcement.len();
            let typing = n as f32 * POINTS_TYPING;
            let field = at(&format!("{}/announcement_txt", label.path));
            if n > 0 && label.age < typing && field.is_some() {
                label.age += dt;
                let text: String = if label.age < typing {
                    let k = ((label.age / typing) * n as f32) as usize;
                    let mut s: String = label.announcement[..k.min(n)].iter().collect();
                    s.push('_');
                    s
                } else {
                    label.announcement.iter().collect()
                };
                with(&mut nodes, field, |p| p.text = Some(text));
            }
        }
    }
    let mut faded = Vec::new();
    for f in &mut logic.points.fading {
        f.age += dt;
        let k = f.age / POINTS_FADE;
        with(&mut nodes, Some(f.entity), |p| {
            p.alpha = Some(1.0 - strong_ease_out(k))
        });
        // (points_mc tweens from where it stands to 100 above the clip, which
        // stands at 0.)
        if let Some(e) = at(&format!("{}/points_mc", f.path)) {
            if let Ok((mut n, _)) = nodes.get_mut(e) {
                let from = n.key.m[5];
                n.set.y = Some(from + (-POINTS_RISE - from) * ease_out(k));
            }
        }
        if k >= 1.0 {
            faded.push(f.entity);
        }
    }
    for e in faded {
        sp.commands.entity(e).try_despawn();
        logic.points.fading.retain(|f| f.entity != e);
    }

    // -- the level-up notice's scripts: the class's icon under its icon_mc at twice
    // its size (its first frame), and its sparkles either side of the level, shown
    // (frame 21); dropped once it has played out --
    if let Some(n) = logic.level_up.as_mut() {
        let alive = nodes.get(n.entity).is_ok();
        n.seen |= alive;
        if alive {
            if let Some(icon) = n.icon.as_deref() {
                if let Some(holder) = at(&format!("{}/icon_mc", n.path)) {
                    let set = Props {
                        xscale: Some(2.0),
                        yscale: Some(2.0),
                        ..Default::default()
                    };
                    sp.attach(holder, icon, "icon_mc", ATTACH, set, false, &none);
                    n.icon = None;
                }
            }
            let frame = frame_of(&nodes, Some(n.entity)).unwrap_or(0);
            let fade = lib
                .sprite(nodes.get(n.entity).map(|(node, _)| node.def).unwrap_or(0))
                .and_then(|s| s.labels.get("fadeOut").copied())
                .unwrap_or(usize::MAX);
            let level_txt = at(&format!("{}/level_mc/level_txt", n.path));
            if !n.sparkles && (LEVEL_UP_SPARKLES..fade).contains(&frame) && level_txt.is_some() {
                let width = level_txt
                    .and_then(|e| nodes.get(e).ok())
                    .and_then(|(node, _)| match lib.defs.get(&node.def) {
                        Some(Def::Text(td)) => Some(text_width(lib, td, &n.level)),
                        _ => None,
                    })
                    .unwrap_or(0.0);
                let x = width / 2.0 + LEVEL_UP_SPARKLE_GAP;
                for (sparkle, x) in [("sparkle_mc", -x), ("sparkle2_mc", x)] {
                    with(&mut nodes, at(&format!("{}/{sparkle}", n.path)), |p| {
                        p.x = Some(x);
                        p.alpha = Some(1.0);
                    });
                }
                n.sparkles = true;
            }
        } else if n.seen {
            logic.level_up = None;
        }
    }
    // -- the item bought (mc_itemUnlocked's scripts): its picture drawn as sprite
    // 714 is placed, no first line (its textOverride is empty) and the name typed in
    // as sprites 720 and 722 are, and out after its time --
    if let Some(p) = logic.item_unlocked.as_mut() {
        p.age += dt;
        if p.picture.is_none()
            && let Some(holder) = at(&format!("{}/#3", p.path))
        {
            // (Mirrored, its right edge on the clip's middle: _x 0 with _xscale -65.)
            let width = lib.images.get(p.image).map_or(0.0, |(_, size)| size.x);
            let scale = Vec2::new(-ITEM_PICTURE_SCALE, ITEM_PICTURE_SCALE);
            let centre = Vec2::new(-width * ITEM_PICTURE_SCALE / 2.0, 0.0);
            p.picture = Some(
                sp.bitmap(holder, p.image, scale, centre, ATTACH, false)
                    .unwrap_or(Entity::PLACEHOLDER),
            );
        }
        with(&mut nodes, at(&format!("{}/#9/#1/text_txt", p.path)), |q| {
            q.text = Some(String::new())
        });
        if let Some(field) = at(&format!("{}/#9/#3/text_txt", p.path)) {
            let n = p.name.len();
            let typing = p.typing.get_or_insert(0.0);
            let k = ((*typing / (n as f32 * ITEM_NAME_TYPING).max(1e-4)) * n as f32) as usize;
            let text: String = p.name[..k.min(n)].iter().collect();
            *typing += dt;
            with(&mut nodes, Some(field), |q| q.text = Some(text));
        }
        if !p.out && p.age >= ITEM_UNLOCKED_SECS {
            p.out = true;
            goto(&mut nodes, lib, Some(p.entity), Err("out"), true);
        }
        // (Gone once it has played out: removeMovieClip.)
        if p.age > 1.0 && nodes.get(p.entity).is_err() {
            logic.item_unlocked = None;
        }
    }
    // -- the game's messages: each taken away after its time (fadeOut), the others
    // closing up (updateDisplay) --
    let before = logic.messages.len();
    for m in &mut logic.messages {
        m.age += dt;
        if m.age >= MESSAGE_SECS {
            sp.commands.entity(m.entity).try_despawn();
        }
    }
    logic.messages.retain(|m| m.age < MESSAGE_SECS);
    if logic.messages.len() != before {
        logic.messages_moved = true;
    }
    // (Each line in its place, the oldest at the top; again next frame for one not
    // there yet.)
    if logic.messages_moved {
        let mut waiting = false;
        for (i, m) in logic.messages.iter().enumerate() {
            if nodes.get(m.entity).is_err() {
                waiting = true;
                continue;
            }
            with(&mut nodes, Some(m.entity), |p| {
                p.y = Some(i as f32 * MESSAGE_SPACING)
            });
        }
        logic.messages_moved = waiting;
    }

    // The damage ring turns with the view, so its arrows keep pointing at the attacker.
    let view = -state.view_yaw.to_degrees();
    with(&mut nodes, at(paths::DAMAGE), |p| p.rotation = Some(view));

    // -- energon (the currency counter, top right: updateMoney) --
    if !logic.started {
        logic.money_shown = state.energon as f32;
        logic.money_target = state.energon;
    }
    if state.energon != logic.money_target {
        // The change under the counter (mc_currencyEarnedRightAlign at -50, 23): "+N"
        // earned, "-N" spent, a change while one shows added to it and its second
        // begun again (updateMoney's CurrencyEarnedClip).
        let mut amount = state.energon as i64 - logic.money_target as i64;
        if let Some(i) = logic
            .notes
            .iter()
            .position(|n| matches!(n.kind, NoteKind::Energon(_)) && n.age < 1.0)
        {
            let shown = logic.notes.remove(i);
            if let NoteKind::Energon(before) = shown.kind {
                amount += before;
            }
            sp.commands.entity(shown.entity).despawn();
        }
        if let Some(anchor) = at(paths::MONEY_GROUP) {
            let set = Props {
                x: Some(-50.0),
                y: Some(23.0),
                ..Default::default()
            };
            let text = if amount < 0 {
                amount.to_string()
            } else {
                format!("+{amount}")
            };
            let mut ov = HashMap::new();
            ov.insert(
                "earned_txt".to_string(),
                Props {
                    text: Some(text),
                    ..Default::default()
                },
            );
            if let Some(e) = sp.attach(
                anchor,
                "mc_currencyEarnedRightAlign",
                "",
                ATTACH,
                set,
                false,
                &ov,
            ) {
                logic.notes.push(Note {
                    entity: e,
                    age: 0.0,
                    kind: NoteKind::Energon(amount),
                });
            }
        }
        logic.money_target = state.energon;
        logic.money_pop = (MONEY_POP_SECS, MONEY_POP_SECS, WHITE);
    }
    // Counts up over 0.3 s, and pops (taller, white) easing back over 0.5 s (red
    // over 5 s when there wasn't enough: CurrencyNotify).
    logic.money_shown += (logic.money_target as f32 - logic.money_shown) * (dt / 0.1).min(1.0);
    let (left, secs, from) = logic.money_pop;
    let left = (left - dt).max(0.0);
    logic.money_pop = (left, secs, from);
    let pf = ease_out(1.0 - left / secs.max(1e-3));
    with(&mut nodes, at(paths::MONEY), |p| {
        p.yscale = Some(1.8 - 0.8 * pf);
        p.cx = Some(from.lerp(NORMAL, pf));
    });
    let money = format!("*{}", logic.money_shown.round() as u32);
    with(&mut nodes, at(paths::MONEY_TEXT), |p| p.text = Some(money));

    // -- Escalation's waves (src/waves.rs): the countdown to the next (sprite 816
    // attaches the single-team game's scoreboard: its clock, sprite 229, and its
    // script's updateShowCountdown and updateCountdownTime) and the wave's line
    // (sprite 812's updateCurrentWave) --
    if logic.wave_clock.is_none()
        && let Some(anchor) = at(paths::WAVE_SCOREBOARD)
    {
        // (Its first updateShowCountdown leaves it out of sight, 10 to the right.)
        let mut ov = HashMap::new();
        ov.insert(
            "#1".to_string(),
            Props {
                x: Some(WAVE_CLOCK_X + WAVE_CLOCK_SLIDE),
                alpha: Some(0.0),
                ..Default::default()
            },
        );
        sp.attach(
            anchor,
            "mc_scoreboardSingleTeam",
            "scoreboard_mc",
            ATTACH,
            Props::default(),
            false,
            &ov,
        );
        logic.wave_clock = Some(WaveClock {
            shows: false,
            value: None,
            alpha: 0.0,
            x: WAVE_CLOCK_X + WAVE_CLOCK_SLIDE,
            fade: None,
            flash: None,
            red: 0.0,
        });
    }
    if let Some(c) = logic.wave_clock.as_mut() {
        let clock = format!("{}/scoreboard_mc/#1", paths::WAVE_SCOREBOARD);
        // In, or out (coming in, with its time at once).
        if c.shows != state.counting {
            c.shows = state.counting;
            c.fade = Some((c.alpha, c.x, 0.0));
            if c.shows {
                c.value = None;
            }
        }
        // A new number while it shows: its time, and from 10 down its red flash.
        if c.value != Some(state.countdown) {
            c.value = Some(state.countdown);
            if c.shows && state.countdown >= 0 {
                let (time, digits) = (
                    clock_text(state.countdown, true),
                    clock_text(state.countdown, false),
                );
                with(&mut nodes, at(&format!("{clock}/time_txt")), |p| {
                    p.text = Some(time)
                });
                with(&mut nodes, at(&format!("{clock}/digits_txt")), |p| {
                    p.text = Some(digits)
                });
                if state.countdown <= WAVE_CLOCK_FLASH_FROM {
                    c.flash = Some(0.0);
                }
            }
        }
        if let Some(t) = c.flash.as_mut() {
            *t += dt;
            let k = ease_in_out(*t / WAVE_CLOCK_SECS);
            c.red = 1.0 - k;
            c.alpha = 1.0 + (WAVE_CLOCK_ALPHA - 1.0) * k;
            if *t >= WAVE_CLOCK_SECS {
                c.flash = None;
            }
        }
        if let Some((alpha, x, t)) = c.fade.as_mut() {
            *t += dt;
            let k = ease_out(*t / WAVE_CLOCK_SECS);
            let (to_alpha, to_x) = if c.shows {
                (1.0, WAVE_CLOCK_X)
            } else {
                (0.0, WAVE_CLOCK_X + WAVE_CLOCK_SLIDE)
            };
            c.alpha = *alpha + (to_alpha - *alpha) * k;
            c.x = *x + (to_x - *x) * k;
            if *t >= WAVE_CLOCK_SECS {
                c.fade = None;
            }
        }
        let mul = WAVE_CLOCK_RED.map(|v| v + (1.0 - v) * (1.0 - c.red));
        let (alpha, x) = (c.alpha, c.x);
        with(&mut nodes, at(&clock), |p| {
            p.x = Some(x);
            p.alpha = Some(alpha);
            p.cx = Some(Cx::new([mul[0], mul[1], mul[2], 1.0], [0.0; 4]));
        });
    }
    if state.wave != logic.wave_shown {
        logic.wave_shown = state.wave;
        if let Some((e, _, _)) = logic.wave_line.take() {
            sp.commands.entity(e).try_despawn();
        }
        if (1..=crate::waves::WAVES_SHOWN).contains(&state.wave)
            && let Some(anchor) = at(paths::WAVE_LINE)
        {
            let text = WAVE_TEXT
                .replace("`r", &state.wave.to_string())
                .replace("`t", &crate::waves::WAVES_SHOWN.to_string())
                .to_uppercase();
            let mut ov = HashMap::new();
            ov.insert(
                "currentWaveText_mc/message_txt".to_string(),
                Props {
                    text: Some(String::new()),
                    ..Default::default()
                },
            );
            if let Some(e) = sp.attach(
                anchor,
                "mc_currentWaveMessage",
                "message_mc",
                ATTACH,
                Props::default(),
                false,
                &ov,
            ) {
                logic.wave_line = Some((e, text, 0.0));
            }
        }
    }
    if let Some((_, text, age)) = logic.wave_line.as_mut() {
        let typing = text.chars().count() as f32 * WAVE_TYPING;
        let k = if typing > 0.0 { *age / typing } else { 1.0 };
        *age += dt;
        let shown = typed_in(text, k.min(1.0));
        let field = format!(
            "{}/message_mc/currentWaveText_mc/message_txt",
            paths::WAVE_LINE
        );
        with(&mut nodes, at(&field), |p| p.text = Some(shown));
    }

    // Notes: ammo added (fading after 4 s) and energon earned or spent (after 1 s).
    let mut done = Vec::new();
    for note in &mut logic.notes {
        note.age += dt;
        let (fade_at, base, slide) = match note.kind {
            NoteKind::Ammo { current, .. } => (
                4.0,
                if current { NORMAL } else { PICKUP_BLUE },
                Vec2::new(0.0, 50.0),
            ),
            NoteKind::Energon(_) => (1.0, NORMAL, Vec2::new(-20.0, 0.0)),
        };
        let fade = ease_out((note.age - fade_at) / 0.5);
        let mut cx = WHITE.lerp(base, ease_out(note.age / 0.3));
        cx.mul[3] *= 1.0 - fade;
        let (x0, y0) = match note.kind {
            NoteKind::Ammo { slot, .. } => (-20.0, -24.0 * slot as f32),
            NoteKind::Energon(_) => (-50.0, 23.0),
        };
        if let Ok((mut n, _)) = nodes.get_mut(note.entity) {
            n.set.cx = Some(cx);
            n.set.x = Some(x0 + slide.x * fade);
            n.set.y = Some(y0 + slide.y * fade);
        }
        if note.age > fade_at + 0.5 {
            done.push(note.entity);
        }
    }
    for e in done {
        sp.commands.entity(e).despawn();
        logic.notes.retain(|n| n.entity != e);
    }

    // -- the head-count counter (top centre): not shown --
    with(&mut nodes, at(paths::COUNTER), |p| p.hidden = true);

    // -- the weapon scope (scopeManager_mc: updateWeaponScope attaches the fine-aiming
    // weapon's scope, mc_tacticalScope for the Burst Rifle and mc_sniperScope for the
    // Nucleon Charge Rifle, taking away one that was up, and plays it out after) --
    let want = if state.dead { Scope::None } else { state.scope };
    if want != logic.scope_kind {
        if let Some(e) = logic.scope.take() {
            if want == Scope::None {
                goto(&mut nodes, lib, Some(e), Err("out"), true);
                sp.commands.entity(e).insert(RemoveAtEnd);
                logic.scope_out = Some(e);
            } else {
                sp.commands.entity(e).try_despawn();
            }
        }
        if want != Scope::None {
            if let Some(e) = logic.scope_out.take() {
                sp.commands.entity(e).try_despawn();
            }
            if let Some(manager) = at(paths::SCOPE_MANAGER) {
                // (mc_sniperScope's first frame scales it to 120%.)
                let (symbol, scale) = match want {
                    Scope::Sniper => ("mc_sniperScope", Some(1.2)),
                    _ => ("mc_tacticalScope", None),
                };
                let set = Props {
                    xscale: scale,
                    yscale: scale,
                    ..Default::default()
                };
                logic.scope = sp.attach(manager, symbol, "scope_mc", 3, set, false, &none);
                logic.scope_distance = SCOPE_MAX_DISTANCE;
                logic.scope_over_max = false;
                logic.scope_charged = false;
                logic.scope_charge_seen = false;
                logic.scope_lights = [None; 3];
                logic.scope_upgrade = None;
                logic.scope_target = false;
                logic.scope_name.clear();
            }
        }
        logic.scope_kind = want;
    }
    let scope = format!("{}/scope_mc", paths::SCOPE_MANAGER);
    // The Nucleon Charge Rifle's (its updateSniperChargeLevel and
    // updateCrosshairColor), once the clip is in place: the charge as a percentage
    // and on the four bars round the scope (their frames), the readout, inner line
    // and rim playing "ready" as it reaches full and "charging" when it drops back;
    // the rim and the lines red over an enemy. The charge upgrade's lights (#35)
    // stay hidden, as for a rifle without it.
    if logic.scope_kind == Scope::Sniper && logic.scope.is_some() && at(&scope).is_some() {
        let charge = state.charge.clamp(0.0, 1.0);
        let percent = (charge * 100.0).ceil();
        with(
            &mut nodes,
            at(&format!("{scope}/text_mc/charge_txt")),
            |p| p.text = Some(format!("{percent}%")),
        );
        let frame = (percent as usize).max(1) - 1;
        for bar in ["bar1_mc", "bar2_mc", "bar3_mc", "bar4_mc"] {
            let path = format!("{scope}/chargeBar_mc/{bar}");
            goto(&mut nodes, lib, at(&path), Ok(frame), false);
        }
        let full = charge >= 1.0;
        if full != logic.scope_charged || !logic.scope_charge_seen {
            // (Its first frame only changes over to "ready" if it starts full.)
            if full || logic.scope_charge_seen {
                let label = if full { "ready" } else { "charging" };
                for part in ["text_mc", "innerLine_mc", "rimGlow_mc"] {
                    goto(
                        &mut nodes,
                        lib,
                        at(&format!("{scope}/{part}")),
                        Err(label),
                        true,
                    );
                }
            }
            logic.scope_charged = full;
            logic.scope_charge_seen = true;
        }
        let colour = if state.on_target {
            CROSSHAIR_RED
        } else {
            NORMAL
        };
        for part in ["rimGlow_mc", "crosshairLines_mc"] {
            with(&mut nodes, at(&format!("{scope}/{part}")), |p| {
                p.cx = Some(colour)
            });
        }
        // The charge upgrade's lights (sprite 286's updateSniperChargeUpgrade, on
        // its first frame and as the value changes): hidden without the upgrade,
        // else each light (highlight1 to 3) "on" up to the value and "off" past it,
        // a light playing its state as it changes (sprite 285's updateState: "on"
        // flashes it in, "off" fades it out).
        let lights = format!("{scope}/#35");
        if logic.scope_upgrade != Some(state.sniper_upgrade) && at(&lights).is_some() {
            logic.scope_upgrade = Some(state.sniper_upgrade);
            with(&mut nodes, at(&lights), |p| {
                p.hidden = state.sniper_upgrade.is_none()
            });
            if let Some(level) = state.sniper_upgrade {
                for i in 1..=3u8 {
                    let on = i <= level;
                    let k = i as usize - 1;
                    if logic.scope_lights[k] != Some(on) {
                        logic.scope_lights[k] = Some(on);
                        let light = at(&format!("{lights}/highlight{i}"));
                        goto(
                            &mut nodes,
                            lib,
                            light,
                            Err(if on { "on" } else { "off" }),
                            true,
                        );
                    }
                }
            }
        }
    }
    // (Once the clip is in place: it is attached a frame before its parts exist.)
    if logic.scope_kind == Scope::Tactical && logic.scope.is_some() && at(&scope).is_some() {
        // Range finder (sprite 349): the distance eases in (30% a frame at 30 fps),
        // the readout slides up its scale as it grows, "MAX" past 150 m; "—m" with
        // nothing under the crosshair.
        let finder = format!("{scope}/#18/#9");
        let text = if state.target_distance < 0.0 {
            logic.scope_distance = SCOPE_MAX_DISTANCE;
            "\u{2014}m".to_string()
        } else {
            let k = 1.0 - 0.7f32.powf(dt * FPS);
            logic.scope_distance += (state.target_distance - logic.scope_distance) * k;
            format!("{:.1}m", (logic.scope_distance * 10.0).trunc() / 10.0)
        };
        with(&mut nodes, at(&format!("{finder}/distance_txt")), |p| {
            p.text = Some(text)
        });
        let d = logic.scope_distance;
        let y = SCOPE_DISPLAY_HEIGHT * (1.0 - (d / SCOPE_MAX_DISTANCE).min(1.0))
            - SCOPE_DISPLAY_HEIGHT / 2.0;
        with(&mut nodes, at(&finder), |p| p.y = Some(y));
        // (MAX blinks while on; "off" is a stop frame.)
        let over = d >= SCOPE_MAX_DISTANCE;
        let indicator = at(&format!("{finder}/maxIndicator_mc"));
        if over != logic.scope_over_max && indicator.is_some() {
            logic.scope_over_max = over;
            let label = if over { "on" } else { "off" };
            goto(&mut nodes, lib, indicator, Err(label), over);
        }
        // The target panel (sprite 336's updateTargetNormalizedHealth and
        // updateTargetName), which multiplayer shows only with the Cyber Tactical
        // Scope upgrade (TnWeaponBurstRifle.ToggleWeaponSpecificHud; without it the
        // health stays at none): hidden with nothing under the crosshair, else
        // shown with its health on both bars (their frames) and its name (upper
        // case; kept while the new one is empty, as the movie does).
        let panel = format!("{scope}/#34/targetHealth_mc");
        if at(&panel).is_some() {
            let show = state.target_health >= 0.0;
            if show != logic.scope_target {
                logic.scope_target = show;
                with(&mut nodes, at(&panel), |p| {
                    p.alpha = Some(if show { 1.0 } else { 0.0 })
                });
            }
            if show {
                let k = state.target_health.clamp(0.0, 1.0);
                // (The bars are 100 frames long: sprite 323.)
                let frame = ((k * 100.0).ceil() as usize).clamp(1, 100) - 1;
                for bar in ["bar1_mc", "bar2_mc"] {
                    let clip = at(&format!("{panel}/{bar}"));
                    goto(&mut nodes, lib, clip, Ok(frame), false);
                }
            }
            if !state.target_name.is_empty() && state.target_name != logic.scope_name {
                logic.scope_name = state.target_name.clone();
                let text = state.target_name.to_uppercase();
                with(
                    &mut nodes,
                    at(&format!("{panel}/targetName_mc/name_txt")),
                    |p| p.text = Some(text),
                );
            }
        }
        // Compass (sprite 316): tipped back 30 degrees (sprite 317's script), the
        // strip sliding with the view's heading.
        with(&mut nodes, at(&format!("{scope}/#28")), |p| p.rot_x = -30.0);
        let heading = state.view_yaw.to_degrees();
        with(&mut nodes, at(&format!("{scope}/#28/#3/compass_mc")), |p| {
            p.x = Some(-heading / 180.0 * COMPASS_MULTIPLIER)
        });
    }

    // -- contextual commands (notifyManager_mc: displayContextualCommand) --
    let want = if state.sticky_rocket && !state.dead {
        Some(sticky_rocket_text(
            &state.transform_key,
            state.transform_key_mouse,
        ))
    } else {
        state.prompt.filter(|_| !state.dead).map(|prompt| {
            command_text(
                &state.pickup_key,
                state.pickup_key_mouse,
                state.pad,
                &state.equipment_keys,
                prompt,
            )
        })
    };
    if want.as_deref() != logic.command.as_ref().map(|c| c.1.as_str()) {
        if let Some((e, _)) = logic.command.take() {
            goto(&mut nodes, lib, Some(e), Err("out"), true);
            sp.commands.entity(e).insert(RemoveAtEnd);
        }
        if let (Some(text), Some(manager)) = (want, at(paths::NOTIFY)) {
            // Its buttons' pictures in place of their markups (its text field's
            // setImageSubstitutions): the field taller by what they add to its
            // line's ascent and, centred on its clip (_y = -_height / 2), higher by
            // half of it; each picture a bitmap over its place in the line.
            let field = command_field(lib);
            let lift = field.map_or(0.0, |(td, _)| button_lift(lib, td, &text));
            let mut ov = HashMap::new();
            ov.insert(
                COMMAND_FIELD.to_string(),
                Props {
                    text: Some(text.clone()),
                    y: field.filter(|_| lift > 0.0).map(|(_, y)| y - lift / 2.0),
                    ..Default::default()
                },
            );
            let name = format!("command{}_mc", logic.command_count);
            logic.command_count += 1;
            sp.placed.clear();
            if let Some(e) = sp.attach(
                manager,
                "mc_contextualCommandLine",
                &name,
                ATTACH + logic.command_count as i32,
                Props::default(),
                false,
                &ov,
            ) {
                if let (Some((td, _)), Some(&txt)) = (field, sp.placed.get(COMMAND_FIELD)) {
                    for (image, centre, scale) in text_icons(lib, td, &text) {
                        sp.bitmap(txt, image, scale, centre, ATTACH, false);
                    }
                }
                logic.command = Some((e, text));
            }
        }
        let glow = logic.command.is_some();
        if glow != logic.command_glow {
            logic.command_glow = glow;
            goto(
                &mut nodes,
                lib,
                at(paths::NOTIFY_GLOW),
                Err(if glow { "in" } else { "out" }),
                true,
            );
        }
    }

    // -- messages --
    // A game announcement (topCenter_mc's updateGameAnnouncement): typed in behind an
    // underscore over 0.05 s a letter (tweenFrom {text: '_'}, linear), erased from
    // the left over a second (tweenTo {text: ''}, Regular.easeOut) 5 s after it came
    // up. The demo's own messages (down, click to play) take its place while shown.
    // A reward announcement in the line under it (updateRewardAnnouncement) types in
    // twice as fast (0.025 s a letter) and goes the same way.
    let game_line = typed_line(&mut logic.game_announcement, dt, 0.05);
    let reward_line = typed_line(&mut logic.reward_announcement, dt, 0.025);
    let a = if state.announcement.is_empty() {
        game_line
    } else {
        state.announcement.clone()
    };
    let h = if state.hint.is_empty() {
        reward_line
    } else {
        state.hint.clone()
    };
    with(&mut nodes, at(paths::ANNOUNCEMENT), |p| p.text = Some(a));
    with(&mut nodes, at(paths::HINT), |p| p.text = Some(h));

    // -- down, dead or watching there is no crosshair, health or ammo
    // (updateHudVisibility: hidden while Spectating, with no MaxHealth or isDowned) --
    let off = state.dead || state.downed.is_some() || state.spectating.is_some();
    for path in [paths::CENTRE, paths::BOTTOM_LEFT, paths::BOTTOM_RIGHT] {
        with(&mut nodes, at(path), |p| p.hidden = off);
    }

    // -- down (bottomCenter_mc's sprite 943: updateDownedHealth): 'on' as he goes
    // down, its parts coming in, and 'off' as he is up again or dies, its parts
    // going; its meter's wedge as far round as his downed health is left
    // (meter_mc.value), and its line with the PC's key (sprite 940: `b is " G ") --
    let downed_on = state.downed.is_some();
    if downed_on != logic.downed_on {
        logic.downed_on = downed_on;
        let label = if downed_on { "on" } else { "off" };
        goto(&mut nodes, lib, at(paths::DOWNED), Err(label), true);
    }
    if let Some(left) = state.downed {
        with(&mut nodes, at(paths::DOWNED_METER), |p| {
            p.pie = Some(left.clamp(0.0, 1.0))
        });
    }
    let line = if state.pad {
        SELF_DESTRUCT_TEXT_PAD
    } else {
        SELF_DESTRUCT_TEXT
    };
    with(&mut nodes, at(paths::SELF_DESTRUCT), |p| {
        p.text = Some(line.to_string())
    });

    // -- the team's lives (bottomLeft_mc's sprite 799 and lifeMarkers_mc's setLives
    // and setFaction): three emblems of the players' side (frame 1 the Autobots',
    // 2 the Decepticons'), those lost red from the left (RedColor; at none left it
    // stays as it was at one) --
    let lost = 3 - state.lives.clamp(1, 3) as usize;
    let side = usize::from(state.decepticons);
    for i in 0..3 {
        let marker = at(&format!("{}/marker_{}", paths::LIVES, i + 1));
        goto(&mut nodes, lib, marker, Ok(side), false);
        with(&mut nodes, marker, |p| {
            p.cx = (i < lost).then_some(LIFE_LOST)
        });
    }

    // -- the revive he is giving (returnFlagNotifyTime_mc's NotifyProgressBarChanged:
    // shown while it is under way, "Reviving <name>" over its bar, filled as far as
    // it has come) --
    let revive = state.revive_bar.clone().filter(|(_, k)| *k > 0.0);
    with(&mut nodes, at(paths::REVIVE_BAR), |p| {
        p.alpha = Some(if revive.is_some() { 1.0 } else { 0.0 })
    });
    if let Some((name, k)) = revive {
        let bar = |part: &str| at(&format!("{}/{part}", paths::REVIVE_BAR));
        with(&mut nodes, bar("description_txt"), |p| {
            p.text = Some(format!("Reviving {name}"))
        });
        with(&mut nodes, bar("bar_mc"), |p| {
            p.xscale = Some(k.clamp(0.0, 1.0))
        });
    }

    // -- another player down (topCenter_mc's sprite 900: NotifyDownedAllyChanged,
    // as downed teammates' markers come and go): 'on' while one is, 'off' once none
    // is; its "Find and Revive your Ally" types in over half a second as it is
    // placed (sprite 899's tweenFrom) --
    if state.ally_downed != logic.ally_notice_on {
        logic.ally_notice_on = state.ally_downed;
        let label = if state.ally_downed { "on" } else { "off" };
        goto(&mut nodes, lib, at(paths::ALLY_DOWNED), Err(label), true);
    }
    match at(paths::FIND_ALLY) {
        Some(e) => {
            let age = match logic.find_ally.as_mut() {
                Some((shown, age)) if *shown == e => {
                    *age += dt;
                    *age
                }
                _ => {
                    logic.find_ally = Some((e, 0.0));
                    0.0
                }
            };
            let text = typed_in(FIND_ALLY_TEXT, (age / 0.5).min(1.0));
            with(&mut nodes, Some(e), |p| p.text = Some(text));
        }
        None => logic.find_ally = None,
    }

    // -- out till the round ends: the respawn screen over the HUD (TnHUD's
    // MultiplayerRespawn_GFX). As it opens its soft edges darken the view's sides
    // and its line ("Waiting for Next Wave to Respawn", with no time to count down)
    // comes in from three times its size (their interps: half a second, eased out);
    // under it the spectating group's "[ Cycle Players ]" hint (the PC's: the keys
    // in the mouse buttons' orange), and over that the player watched
    // (SetSpectateTarget: "Currently Spectating" and his name, easing in over half
    // a second, out again with nobody to watch) --
    match (state.spectating.is_some(), logic.respawn.is_some()) {
        (true, false) => {
            if let Some(hud) = at("hud_mc") {
                // (The edges and the line fade in; the spectating group shows at
                // once, its currentlySpectating_mc at none.)
                let part =
                    |sp: &mut Spawner, symbol: &str, name: &str, depth: i32, y: f32, alpha| {
                        let set = Props {
                            y: Some(y),
                            alpha,
                            ..Default::default()
                        };
                        sp.attach(hud, symbol, name, RESPAWN_DEPTH + depth, set, false, &none)
                    };
                let edges = part(
                    &mut sp,
                    "respawn_edges",
                    "respawnEdges_mc",
                    1,
                    0.0,
                    Some(0.0),
                );
                let spectate = part(
                    &mut sp,
                    "respawn_spectate",
                    "respawnSpectate_mc",
                    3,
                    RESPAWN_SPECTATE_Y,
                    None,
                );
                let timer = part(
                    &mut sp,
                    "respawn_timer",
                    "respawnTimer_mc",
                    6,
                    RESPAWN_TIMER_Y,
                    Some(0.0),
                );
                if let (Some(edges), Some(timer), Some(spectate)) = (edges, timer, spectate) {
                    logic.respawn = Some(RespawnScreen {
                        edges,
                        timer,
                        spectate,
                        age: 0.0,
                        shows_target: false,
                        fade: (0.0, 0.0, 1.0),
                        name: String::new(),
                        pad_icons: None,
                    });
                }
            }
        }
        (false, true) => {
            if let Some(r) = logic.respawn.take() {
                for e in [r.edges, r.timer, r.spectate] {
                    sp.commands.entity(e).try_despawn();
                }
            }
        }
        _ => {}
    }
    if let (Some(watched), Some(r)) = (state.spectating.as_ref(), logic.respawn.as_mut()) {
        r.age += dt;
        let k = ease_out(r.age / 0.5);
        let (edges, timer) = (r.edges, r.timer);
        with(&mut nodes, Some(edges), |p| {
            p.alpha = Some(k);
            p.xscale = Some(stage.0 / RESPAWN_EDGES_SIZE);
            p.yscale = Some(stage.2 / RESPAWN_EDGES_SIZE);
        });
        with(&mut nodes, Some(timer), |p| {
            p.alpha = Some(k);
            p.xscale = Some(3.0 - 2.0 * k);
            p.yscale = Some(3.0 - 2.0 * k);
        });
        let field = |part: &str| at(&format!("hud_mc/{part}"));
        with(&mut nodes, field("respawnTimer_mc/time_txt"), |p| {
            p.text = Some(String::new())
        });
        with(&mut nodes, field("respawnTimer_mc/respawn_txt"), |p| {
            p.text = Some(RESPAWN_NEXT_WAVE.to_string())
        });
        // (With a controller, the bumpers' pictures at its ends: they make the
        // line's ascent theirs, so its words sit lower by the difference.)
        let hint = cycle_players_hint(lib, state.pad);
        with(
            &mut nodes,
            field("respawnSpectate_mc/buttonHints_txt"),
            |p| {
                p.text = Some(hint.text.to_string());
                p.y = Some(hint.y);
            },
        );
        if state.pad != r.pad_icons.is_some() {
            for e in r.pad_icons.take().into_iter().flatten() {
                sp.commands.entity(e).try_despawn();
            }
            if state.pad {
                let scale = Vec2::splat(hint.icon_scale);
                r.pad_icons = Some(
                    CYCLE_PLAYERS_ICONS
                        .iter()
                        .zip(hint.icons)
                        .filter_map(|(image, centre)| {
                            sp.bitmap(r.spectate, image, scale, centre, ATTACH, false)
                        })
                        .collect(),
                );
            }
        }
        // SetSpectateTarget: a name brings the group in and shows it; none takes
        // it away (the last name stays in it as it goes).
        let shows = watched.as_ref().is_some_and(|n| !n.is_empty());
        let (from, to, t) = r.fade;
        let alpha_now = from + (to - from) * ease_out(t / 0.5);
        if shows != r.shows_target {
            r.shows_target = shows;
            r.fade = (alpha_now, if shows { 1.0 } else { 0.0 }, 0.0);
        } else {
            r.fade.2 = t + dt;
        }
        if let Some(name) = watched.as_ref().filter(|_| shows) {
            r.name.clone_from(name);
        }
        let (from, to, t) = r.fade;
        let alpha = from + (to - from) * ease_out(t / 0.5);
        with(
            &mut nodes,
            field("respawnSpectate_mc/currentlySpectating_mc"),
            |p| p.alpha = Some(alpha),
        );
        let name = r.name.clone();
        with(
            &mut nodes,
            field("respawnSpectate_mc/currentlySpectating_mc/spectatedPlayer_txt"),
            |p| p.text = Some(name),
        );
    }

    // -- a marker over each other player (mc_markerPawn: updateSpecialArguments):
    // his name in the friendly colour over his health and shield bars, which turn
    // red, with the red glow behind them, at 75% health and under with the shield
    // down (at half health with some shield left). Off the screen it hides (the
    // game's arrow to him at the screen's edge is left out). --
    let ids: HashSet<usize> = state.allies.iter().map(|a| a.id).collect();
    logic.markers.retain(|id, (e, _)| {
        let keep = ids.contains(id);
        if !keep {
            sp.commands.entity(*e).despawn();
        }
        keep
    });
    let half = Vec2::new(stage.0, stage.2) / 2.0;
    let mut strobes = std::mem::take(&mut logic.strobes);
    strobes.retain(|id, _| ids.contains(id));
    for ally in &state.allies {
        let name = format!("marker{}_mc", ally.id);
        if !logic.markers.contains_key(&ally.id) {
            let Some(anchor) = at(paths::MARKERS) else {
                continue;
            };
            let mut ov: HashMap<String, Props> = MARKER_HIDDEN
                .iter()
                .map(|part| {
                    let hide = Props {
                        hidden: true,
                        ..Default::default()
                    };
                    (part.to_string(), hide)
                })
                .collect();
            ov.insert(
                "container_mc".to_string(),
                Props {
                    cx: Some(FRIENDLY),
                    ..Default::default()
                },
            );
            // Hidden until it has been placed.
            let set = Props {
                hidden: true,
                ..Default::default()
            };
            let depth = ATTACH + ally.id as i32;
            if let Some(e) = sp.attach(anchor, "mc_markerPawn", &name, depth, set, false, &ov) {
                logic.markers.insert(ally.id, (e, false));
            }
            continue;
        }
        let Some((entity, low_shown)) = logic.markers.get_mut(&ally.id) else {
            continue;
        };
        let place = ally
            .at
            .map(|p| Vec2::new(p.x, -p.y) * half)
            .filter(|p| p.x.abs() <= half.x && p.y.abs() <= half.y);
        with(&mut nodes, Some(*entity), |p| {
            p.hidden = place.is_none();
            if let Some(place) = place {
                p.x = Some(place.x);
                p.y = Some(place.y);
            }
        });
        let path = format!("{}/{name}", paths::MARKERS);
        let text = ally.name.clone();
        with(
            &mut nodes,
            at(&format!("{path}/container_mc/description_txt")),
            |p| p.text = Some(text),
        );
        // health_mc and shields_mc.gotoAndStop(ceil(value * _totalframes)); down, the
        // health bar shows his downed health (SA_NormalizedDownedHealth).
        let bar_frame = |v: f32| ((v.clamp(0.0, 1.0) * 100.0).ceil() as usize).clamp(1, 100) - 1;
        let health = ally.health.clamp(0.0, 1.0);
        let bar = at(&format!("{path}/health_mc"));
        goto(
            &mut nodes,
            lib,
            bar,
            Ok(bar_frame(ally.downed.unwrap_or(health))),
            false,
        );
        // Down, downed_mc shows, strobing (IsDowned: its doStrobe towards the
        // marker's colour, CurrentColor), till he is up again.
        with(&mut nodes, at(&format!("{path}/downed_mc")), |p| {
            p.hidden = ally.downed.is_none()
        });
        if ally.downed.is_some() {
            let t = strobes.entry(ally.id).or_insert(0.0);
            *t += dt;
            let (scale, cx) = strobe(*t, FRIENDLY);
            let icon = at(&format!("{path}/downed_mc/downedIcon_mc"));
            with(&mut nodes, icon, |p| {
                p.xscale = Some(scale);
                p.yscale = Some(scale);
                p.cx = Some(cx);
            });
        } else {
            strobes.remove(&ally.id);
        }
        let shields = at(&format!("{path}/shields_mc"));
        with(&mut nodes, shields, |p| p.hidden = ally.shield.is_none());
        let shield = ally.shield.unwrap_or(0.0);
        goto(&mut nodes, lib, shields, Ok(bar_frame(shield)), false);
        let low = if shield > 0.0 {
            health <= 0.5
        } else {
            health <= 0.75
        };
        if low != *low_shown {
            *low_shown = low;
            let glow = at(&format!("{path}/background_mc"));
            with(&mut nodes, glow, |p| p.hidden = !low);
            let label = if low { "low" } else { "normal" };
            goto(&mut nodes, lib, glow, Err(label), low);
            // Red, at the bars' own alpha (75% and 50%); back to their own colours.
            for (bar, alpha) in [(bar, 0.75), (shields, 0.5)] {
                with(&mut nodes, bar, |p| {
                    p.cx = low.then_some(RED);
                    p.alpha = low.then_some(alpha);
                });
            }
        }
    }
    logic.strobes = strobes;

    // -- a marker over each sentry the Orbital Pistol has revealed (mc_markerPawn
    // for an enemy: TnObjectiveMarkerTypeTransformerVersus.ShouldDisplayEnemyMarker,
    // its TnBuffRevealPositionToEnemy), through walls: in the enemy colour, with no
    // name or health, so its arrow (icon_mc) shows, unfolding as it appears, and over
    // it the icon of the mark the sentry has (buffs_mc: updateBuffs'
    // mc_markerPawnBuffIcon), lowered for the missing name. Off the screen it hides,
    // as the players' do. --
    let ids: HashSet<u64> = state.revealed.iter().map(|r| r.id).collect();
    logic.revealed.retain(|id, (e, _, _)| {
        let keep = ids.contains(id);
        if !keep {
            sp.commands.entity(*e).despawn();
        }
        keep
    });
    for r in &state.revealed {
        let name = format!("enemy{}_mc", r.id);
        let path = format!("{}/{name}", paths::MARKERS);
        if !logic.revealed.contains_key(&r.id) {
            let Some(anchor) = at(paths::MARKERS) else {
                continue;
            };
            let hide = Props {
                hidden: true,
                ..Default::default()
            };
            let mut ov: HashMap<String, Props> = ENEMY_MARKER_HIDDEN
                .iter()
                .map(|part| (part.to_string(), hide.clone()))
                .collect();
            ov.insert(
                "container_mc".to_string(),
                Props {
                    cx: Some(ENEMY),
                    ..Default::default()
                },
            );
            ov.insert(
                "buffs_mc".to_string(),
                Props {
                    y: Some(MARKER_BUFFS_Y + MARKER_NO_NAME_DROP),
                    ..Default::default()
                },
            );
            // Hidden until it has been placed (over the players' markers).
            logic.revealed_depth = (logic.revealed_depth + 1) % 10_000;
            let depth = ATTACH + 100 + logic.revealed_depth;
            if let Some(e) = sp.attach(anchor, "mc_markerPawn", &name, depth, hide, false, &ov) {
                logic.revealed.insert(r.id, (e, 0, None));
            }
            continue;
        }
        let Some((entity, shown, icon)) = logic.revealed.get_mut(&r.id) else {
            continue;
        };
        let place =
            r.at.map(|p| Vec2::new(p.x, -p.y) * half)
                .filter(|p| p.x.abs() <= half.x && p.y.abs() <= half.y);
        with(&mut nodes, Some(*entity), |p| {
            p.hidden = place.is_none();
            if let Some(place) = place {
                p.x = Some(place.x);
                p.y = Some(place.y);
            }
        });
        // Its mark's icon: the one buff it shows, centred over the arrow.
        if r.mark != *shown {
            let Some(buffs) = at(&format!("{path}/buffs_mc")) else {
                continue;
            };
            if let Some(old) = icon.take() {
                sp.commands.entity(old).despawn();
            }
            *shown = r.mark;
            let image = MARK_ICONS.get((r.mark as usize).wrapping_sub(1));
            if let Some(&image) = image {
                let set = Props {
                    xscale: Some(BUFF_ICON_SCALE),
                    yscale: Some(BUFF_ICON_SCALE),
                    cx: Some(ENEMY),
                    ..Default::default()
                };
                if let Some(e) = sp.attach(
                    buffs,
                    "mc_markerPawnBuffIcon",
                    "buffIcon0_mc",
                    ATTACH,
                    set,
                    false,
                    &none,
                ) {
                    sp.bitmap(e, image, Vec2::ONE, Vec2::ZERO, 1, false);
                    *icon = Some(e);
                }
            }
        }
    }

    // -- a homing weapon's lock (mc_markerLockOn, markers_mc's 'LockOn' type: on the
    // sentry while the lock is for it, hidden behind the camera, as it isn't kept on
    // the screen). Its script (updateSpecialArguments, each update): while the lock
    // is coming it stays a quarter of the way nearer the screen's middle each time,
    // its prongs closing in (20 out at none, in at a lock) and fading in; locked, it
    // goes over onto the sentry, faster each time (LockStickiness), its prongs dimmed
    // to a quarter and its locked_mc playing 'on' ('off' as a lock is lost). --
    let lock_path = format!("{}/markerLockOn_mc", paths::MARKERS);
    if let Some(lock) = state.lock_on
        && let Some(m) = logic.lock_on.as_mut().filter(|m| m.id == lock.id)
    {
        let target = lock.at.map(|p| Vec2::new(p.x, -p.y) * half);
        let v = lock.progress;
        let k = dt * MARKER_UPDATES;
        if v < 1.0 {
            m.stickiness = 0.25;
            m.pos *= 0.75f32.powf(k);
        } else {
            m.stickiness = (m.stickiness + 0.05 * k).min(1.0);
            if let Some(t) = target {
                m.pos = t + (m.pos - t) * (1.0 - m.stickiness).powf(k);
            }
        }
        let pos = m.pos;
        with(&mut nodes, Some(m.entity), |p| {
            p.hidden = target.is_none();
            p.x = Some(pos.x);
            p.y = Some(pos.y);
        });
        if v != m.last {
            m.last = v;
            let out = (1.0 - v) * 20.0;
            let wide = 1.0 + (1.0 - v) * 0.5;
            for prong in 1..=3 {
                let panner = at(&format!(
                    "{lock_path}/acquiring_mc/prong{prong}_mc/panner_mc"
                ));
                with(&mut nodes, panner, |p| {
                    p.y = Some(-out);
                    p.xscale = Some(wide);
                });
            }
        }
        let acquiring = at(&format!("{lock_path}/acquiring_mc"));
        let locked = at(&format!("{lock_path}/locked_mc"));
        if v < 1.0 {
            with(&mut nodes, acquiring, |p| p.alpha = Some(v));
            if m.locked {
                goto(&mut nodes, lib, locked, Err("off"), true);
                m.locked = false;
            }
        } else if !m.locked {
            with(&mut nodes, acquiring, |p| p.alpha = Some(0.25));
            goto(&mut nodes, lib, locked, Err("on"), true);
            m.locked = true;
        }
    } else if let Some(lock) = state.lock_on {
        // A new sentry's: the marker starts over where it is (its first frame's
        // currentPos is the _x and _y UpdateMarker gave it).
        if let Some(old) = logic.lock_on.take() {
            sp.commands.entity(old.entity).despawn();
        }
        if let Some(anchor) = at(paths::MARKERS) {
            let place = lock.at.map(|p| Vec2::new(p.x, -p.y) * half);
            let set = Props {
                hidden: true,
                x: place.map(|p| p.x),
                y: place.map(|p| p.y),
                ..Default::default()
            };
            let depth = ATTACH + 90;
            if let Some(e) = sp.attach(
                anchor,
                "mc_markerLockOn",
                "markerLockOn_mc",
                depth,
                set,
                false,
                &none,
            ) {
                logic.lock_on = Some(LockMarker {
                    entity: e,
                    id: lock.id,
                    pos: place.unwrap_or(Vec2::ZERO),
                    stickiness: 0.25,
                    locked: false,
                    last: f32::NAN,
                });
            }
        }
    } else if let Some(old) = logic.lock_on.take() {
        sp.commands.entity(old.entity).despawn();
    }

    // -- an enemy grenade near him (mc_markerGrenade, markers_mc's 'Grenade' type:
    // UpdateMarker, each frame it is in reach): its icon (icon_mc's GrenadeIconFrag,
    // blinking) over it; off the screen, or behind the camera (turned back through
    // the middle), kept at the screen's edge inside the safe frame, with its arrow
    // (arrow_mc, red, its chevrons running out) turned towards it. A grenade gone,
    // or out of reach, takes its marker with it (FinishMarkerUpdate). --
    let ids: HashSet<u64> = state.grenades.iter().map(|g| g.id).collect();
    logic.grenade_markers.retain(|id, (e, _)| {
        let keep = ids.contains(id);
        if !keep {
            sp.commands.entity(*e).despawn();
        }
        keep
    });
    let edge = marker_clamp_in(stage.0, stage.2, state.safe_frame);
    for g in &state.grenades {
        let name = format!("grenade{}_mc", g.id);
        let path = format!("{}/{name}", paths::MARKERS);
        let (place, arrow) = clamp_marker(g.at, g.in_front, half, edge);
        let Some((entity, ready)) = logic.grenade_markers.get_mut(&g.id) else {
            // Attached on top of the markers there are (getNextHighestDepth), its
            // first frame's place set at once.
            let Some(anchor) = at(paths::MARKERS) else {
                continue;
            };
            logic.grenade_depth = (logic.grenade_depth + 1) % 10_000;
            let depth = ATTACH + 20_000 + logic.grenade_depth;
            let set = Props {
                x: Some(place.x),
                y: Some(place.y),
                ..Default::default()
            };
            if let Some(e) = sp.attach(anchor, "mc_markerGrenade", &name, depth, set, false, &none)
            {
                logic.grenade_markers.insert(g.id, (e, false));
            }
            continue;
        };
        with(&mut nodes, Some(*entity), |p| {
            p.x = Some(place.x);
            p.y = Some(place.y);
        });
        let arrow_mc = at(&format!("{path}/arrow_mc"));
        with(&mut nodes, arrow_mc, |p| {
            p.hidden = arrow.is_none();
            p.rotation = arrow;
        });
        // Its grenade's type (updateGrenadeType, from its special argument, the
        // grenade's class): a frag's arrow red and running ('loop'), its icon at 65%,
        // centred.
        if !*ready {
            let Some(icon) = at(&format!("{path}/icon_mc/icon_mc")) else {
                continue;
            };
            *ready = true;
            with(&mut nodes, arrow_mc, |p| p.cx = Some(GRENADE_RED));
            goto(&mut nodes, lib, arrow_mc, Err("loop"), true);
            let scale = Vec2::splat(GRENADE_ICON_SCALE);
            sp.bitmap(icon, GRENADE_ICON, scale, Vec2::ZERO, 1, false);
        }
    }

    // -- the prompts under the crosshair (sprites 872 and 873): DETONATE while the
    // grenade launcher in hand has rounds out (ActiveGrenadeCount over 0), at its
    // left with the button that sets them off (updateIcon: the right mouse button,
    // the left in the Southpaw layout; with a controller, PreferGamepad, LT: its XBOX
    // frame); CLUSTERBOMB while the Combustor's bomb is in the air (CanClusterBomb),
    // at its right with the fire button (RT: XBOX_LH). --
    let prompts = [
        (
            state.grenades_out,
            GRENADE_PROMPTS,
            "mc_activeGrenadePromptText",
            "grenadePrompt_mc",
            -10.0,
            "PC_RMB",
            "PC_LMB",
            "XBOX",
        ),
        (
            state.can_cluster_bomb,
            CLUSTER_PROMPTS,
            "mc_clusterBombPromptText",
            "clusterBombPrompt_mc",
            10.0,
            "PC_LMB",
            "PC_RMB",
            "XBOX_LH",
        ),
    ];
    for (k, (show, anchor, symbol, name, x, icon, icon_lefty, icon_pad)) in
        prompts.into_iter().enumerate()
    {
        let shown = if k == 0 {
            &mut logic.grenade_prompt
        } else {
            &mut logic.cluster_prompt
        };
        match (show && !off, *shown) {
            (true, None) => {
                if let Some(parent) = at(anchor) {
                    let set = Props {
                        x: Some(x),
                        ..Default::default()
                    };
                    *shown = sp.attach(parent, symbol, name, ATTACH, set, false, &none);
                }
            }
            (true, Some(_)) => {
                let button = at(&format!("{anchor}/{name}/buttonIcon_mc"));
                let frame = if state.pad {
                    icon_pad
                } else if state.southpaw {
                    icon_lefty
                } else {
                    icon
                };
                goto(&mut nodes, lib, button, Err(frame), false);
            }
            (false, Some(e)) => {
                sp.commands.entity(e).despawn();
                *shown = None;
            }
            (false, None) => {}
        }
    }

    logic.started = true;
}

#[cfg(test)]
mod tests {
    /// Each HUD piece gets a slot of its own among the HUD's colours (its mesh
    /// tag) as it is spawned, given back as it goes and given out again with its
    /// colours none; setting a piece's colours says whether they changed.
    #[test]
    fn hud_pieces_get_slots_for_their_colours() {
        use super::*;
        let mut world = World::new();
        world.init_resource::<HudColors>();
        let a = world.spawn((HudSlot, MeshTag(u32::MAX))).id();
        let b = world.spawn((HudSlot, MeshTag(u32::MAX))).id();
        let tag = |w: &World, e: Entity| w.get::<MeshTag>(e).unwrap().0;
        assert_eq!((tag(&world, a), tag(&world, b)), (0, 1));
        {
            let mut colors = world.resource_mut::<HudColors>();
            assert!(colors.set(1, Vec4::splat(0.5), Vec4::ZERO));
            assert!(!colors.set(1, Vec4::splat(0.5), Vec4::ZERO));
            assert!(!colors.set(9, Vec4::ONE, Vec4::ZERO));
            assert_eq!(colors.colors[1], [0.5, 0.5, 0.5, 0.5, 0.0, 0.0, 0.0, 0.0]);
        }
        world.entity_mut(b).despawn();
        assert_eq!(world.resource::<HudColors>().free, [1]);
        world.clear_trackers();
        let c = world.spawn((HudSlot, MeshTag(u32::MAX))).id();
        assert_eq!(tag(&world, c), 1);
        assert_eq!(world.resource::<HudColors>().colors[1], NO_COLORS);
        // (Changed, so the render world writes the slot's colours, none, over b's.)
        assert!(world.is_resource_changed::<HudColors>());
    }

    /// The low-health effect (the game's is a post process) fills the screen however
    /// hud_mc is drawn: as it is on foot, squashed in the car (it showed only 0.98
    /// by 0.92 of the screen there), zoomed in aiming, jolted and turned by a shot's
    /// camera shake.
    #[test]
    fn the_low_health_effect_fills_the_screen_in_the_car() {
        use bevy::prelude::*;
        let size = Vec2::new(super::STAGE_H * 16.0 / 9.0, super::STAGE_H) * 1.02;
        for zoom in [Vec2::ONE, Vec2::new(0.96, 0.90), Vec2::splat(1.02)] {
            for shake in [Vec3::ZERO, Vec3::new(1.5, -2.0, 3.0)] {
                let (at, scale, turn) = super::on_screen(size, zoom, shake);
                // hud_mc as layout_hud places it (jolted and turned by the shake), in
                // the root that zooms it.
                let jolt = Vec2::new(shake.y, shake.x) * 8.0;
                let hud_turn = Vec2::from_angle((shake.z * 0.1).to_radians());
                let own_turn = Vec2::from_angle(turn.to_radians());
                for corner in [
                    Vec2::new(-0.5, -0.5),
                    Vec2::new(0.5, 0.5),
                    Vec2::new(0.5, -0.5),
                ] {
                    let in_hud = at + own_turn.rotate(corner * scale);
                    let seen = (jolt + hud_turn.rotate(in_hud)) * zoom;
                    assert!(
                        (seen - corner * size).abs().max_element() < 1e-3,
                        "{zoom} {shake}: {seen} for {}",
                        corner * size
                    );
                }
            }
        }
    }

    /// A console's line with a controller: "Press", the X button's picture (29
    /// pixels high) in the line in place of the keyboard's key, then the words, all
    /// centred together; the line's ascent the picture's, so its words lower and
    /// its field higher by half of what that adds. With the keyboard, the key in
    /// its colour, no picture.
    #[test]
    fn a_controllers_prompt_shows_its_button() {
        use bevy::prelude::*;
        let line = "Press <<UseInteract>> to deposit <font face='$NumberFontWide' \
                    color=\"#FFCC00\">*`i</font> of <font face='$NumberFontWide' \
                    color='#00FFFF'>*`c</font> to open door";
        let prompt = super::Prompt::Deposit(line, 250, 1000);
        let keys = [
            ("D-PAD LEFT".to_string(), false),
            ("D-PAD RIGHT".to_string(), false),
        ];
        let pad = super::command_text("X", false, true, &keys, prompt);
        assert!(pad.starts_with("Press {ButtonX} to deposit "), "{pad}");
        let board = super::command_text("E", false, false, &keys, prompt);
        assert!(
            board.starts_with("Press <font color='#00FFD4'>E</font> to"),
            "{board}"
        );
        // (A button without a picture, by its name as a key.)
        let stick = super::command_text("LS", false, true, &keys, super::Prompt::Revive);
        assert_eq!(stick, "Hold <font color='#00FFD4'>LS</font> to Revive");

        let dir = crate::assets_dir().join("hud");
        if !dir.join("hud.txt").exists() {
            return;
        }
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<Image>();
        let world = app.world_mut();
        let server = world.resource::<AssetServer>().clone();
        let lib = world.resource_scope(|_, mut meshes: Mut<Assets<Mesh>>| {
            super::HudLib::load(&dir, &server, &mut meshes)
        });
        let lib = lib.expect("the HUD");
        // Every button's picture is the HUD's.
        for (_, _, image) in super::BUTTON_MARKUPS {
            assert!(lib.images.contains_key(*image), "{image}");
        }
        let (td, y) = super::command_field(&lib).expect("the command line's field");
        assert_eq!(y, -12.0);
        let icons = super::text_icons(&lib, td, &pad);
        assert_eq!(icons.len(), 1);
        let (image, centre, scale) = icons[0];
        assert_eq!(image, "Gamepad_X");
        // (Its picture is 32 pixels square.)
        assert!((scale - Vec2::splat(29.0 / 32.0)).abs().max_element() < 1e-5);
        // After "Press ", the line centred in the field with the picture in it.
        let words = super::text_width(&lib, td, &pad.replace("{ButtonX}", ""));
        let all = super::text_width(&lib, td, &pad);
        assert!((all - words - 29.0).abs() < 1e-3, "{all} {words}");
        let middle = (td.rect[0] + td.rect[2]) / 2.0;
        let press = super::text_width(&lib, td, "Press ");
        assert!((centre.x - (middle - all / 2.0 + press + 14.5)).abs() < 1e-3);
        // Its baseline 19.5 + 29 / 32 down it, the line's: the font's ascent 17.8.
        let lift = super::button_lift(&lib, td, &pad);
        let ascent = 0.99023 * 18.0;
        assert!(
            (lift - (19.5 + 29.0 / 32.0 - ascent)).abs() < 1e-3,
            "{lift}"
        );
        let base = td.rect[1] + 2.0 + ascent + lift;
        assert!((centre.y - (base - 19.5 - 29.0 / 32.0 + 14.5)).abs() < 1e-3);
        assert_eq!(super::button_lift(&lib, td, &board), 0.0);
        assert!(super::text_icons(&lib, td, &board).is_empty());
    }

    /// The spectating hint as the respawn screen shows it: on a PC with the keyboard
    /// the keys, [ and ], in orange round "Cycle Players"; with a controller its
    /// bumpers' pictures (34 pixels square) at the line's ends, the words between
    /// them lower by what the pictures add to the line's ascent.
    #[test]
    fn the_spectating_hint_shows_its_keys_and_bumpers() {
        use bevy::prelude::*;
        let dir = crate::assets_dir().join("hud");
        if !dir.join("hud.txt").exists() {
            return;
        }
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<Image>();
        let world = app.world_mut();
        let server = world.resource::<AssetServer>().clone();
        let lib = world.resource_scope(|_, mut meshes: Mut<Assets<Mesh>>| {
            super::HudLib::load(&dir, &server, &mut meshes)
        });
        let lib = lib.expect("the HUD");
        let keys = super::cycle_players_hint(&lib, false);
        assert_eq!(keys.text, super::CYCLE_PLAYERS_HINT);
        assert_eq!(keys.y, 27.0);
        assert!(
            keys.text
                .contains("[</font>  Cycle Players  <font color='#FF9333'>]")
        );
        for image in super::CYCLE_PLAYERS_ICONS {
            assert_eq!(
                lib.images.get(image).map(|i| i.1),
                Some(Vec2::splat(64.0)),
                "{image}"
            );
        }
        let pad = super::cycle_players_hint(&lib, true);
        assert_eq!(pad.text.trim(), "Cycle Players");
        assert!((pad.icon_scale - 34.0 / 64.0).abs() < 1e-5);
        // (Either side of the middle, just past the words.)
        let words = super::text_width(
            &lib,
            match lib.defs.get(&300008) {
                Some(super::Def::Text(td)) => td,
                _ => panic!("buttonHints_txt"),
            },
            super::CYCLE_PLAYERS_HINT_PAD,
        );
        assert!(words > 100.0, "{words}");
        assert!((pad.icons[0].x + pad.icons[1].x).abs() < 1e-3);
        assert!((pad.icons[1].x - (words / 2.0 + 17.0)).abs() < 1e-3);
        assert_eq!(pad.icons[0].y, pad.icons[1].y);
        // (Its words lower than the keyboard's by the pictures' extra ascent: their
        // baseline 44 of 64 pixels down them, 23.4 at their size.)
        assert!(pad.y > keys.y && pad.y < keys.y + 23.4, "{}", pad.y);
        let baseline = pad.icons[0].y - 17.0 + 44.0 * 34.0 / 64.0;
        assert!(baseline > pad.y, "{baseline}");
    }

    /// Every part the movie's clips place has its record in hud.txt: the
    /// controller's buttons too (rb_mc's xbox frame, the reload prompt's X, the
    /// grenade prompts' triggers), which build_hud.py takes from the button icons
    /// movie the HUD imports them from.
    #[test]
    fn every_part_the_clips_place_is_exported() {
        let path = crate::assets_dir().join("hud/hud.txt");
        let Ok(text) = std::fs::read_to_string(&path) else {
            assert!(std::env::var_os("BEVY_ASSET_ROOT").is_none(), "no {path:?}");
            return;
        };
        let mut defined = std::collections::HashSet::new();
        let mut placed = Vec::new();
        for line in text.lines() {
            let p: Vec<&str> = line.split_whitespace().collect();
            match p.as_slice() {
                ["sprite" | "quad" | "text" | "line" | "bar", id, ..] => {
                    defined.insert(*id);
                }
                ["key", sprite, frame, _, child, ..] if *child != "-" => {
                    placed.push((*sprite, *frame, *child));
                }
                _ => {}
            }
        }
        let missing: Vec<_> = placed
            .iter()
            .filter(|(_, _, child)| !defined.contains(child))
            .collect();
        assert!(missing.is_empty(), "placed but not exported: {missing:?}");
        // rb_mc's xbox frame (sprite 860's first) shows RB; the reload prompt's
        // first frame, X.
        assert!(text.lines().any(|l| l.starts_with("quad 207 Gamepad_R1 ")));
        assert!(text.lines().any(|l| l.starts_with("quad 218 Gamepad_X ")));
        // The grenade marker, its arrow's chevron drawn as its image (with its
        // halo) and its two running copies.
        assert!(text.lines().any(|l| l == "symbol mc_markerGrenade 540"));
        assert!(text.lines().any(|l| l.starts_with("quad 534 HudShape534 ")));
    }

    /// A grenade's marker on the screen stays where it is, arrowless; off the
    /// screen it goes to the clamp box's edge along its line from the middle, its
    /// arrow pointing out that way; behind the camera it is turned back through
    /// the middle first, then kept at the edge.
    #[test]
    fn markers_kept_on_the_screen_go_to_its_edge() {
        use super::*;
        // A 16:9 stage, 1280 by 720, with no safe frame: the box 90 in from the
        // top and bottom, 90 * 1280 / 1120 in from the sides.
        let half = Vec2::new(640.0, 360.0);
        let edge = marker_clamp_in(1280.0, STAGE_H, Vec2::ZERO);
        assert!((edge.x - (640.0 - 90.0 * 1280.0 / 1120.0)).abs() < 1e-3);
        assert!((edge.y - 270.0).abs() < 1e-3);
        // Half the safe frame's share on each side: 10% of the height takes 36 more.
        assert!((marker_clamp_in(1280.0, STAGE_H, Vec2::new(0.0, 0.1)).y - 234.0).abs() < 1e-3);
        // On the screen (y up in the device coordinates, down on the stage).
        let (p, arrow) = clamp_marker(Vec2::new(0.5, 0.25), true, half, edge);
        assert_eq!((p, arrow), (Vec2::new(320.0, -90.0), None));
        // Off to the right, level with the middle: on the right edge, the arrow
        // turned a quarter anticlockwise (from down to right).
        let (p, arrow) = clamp_marker(Vec2::new(1.5, 0.0), true, half, edge);
        assert!((p - Vec2::new(edge.x, 0.0)).length() < 1e-3);
        assert!((arrow.unwrap() + 90.0).abs() < 1e-3);
        // Far below: on the bottom edge, the arrow down as drawn.
        let (p, arrow) = clamp_marker(Vec2::new(0.0, -3.0), true, half, edge);
        assert!((p - Vec2::new(0.0, edge.y)).length() < 1e-3);
        assert!(arrow.unwrap().abs() < 1e-3);
        // Off past a corner: where its line meets the box.
        let (p, _) = clamp_marker(Vec2::new(2.0, 2.0), true, half, edge);
        let m = -360.0 / 640.0;
        assert!((p.y - m * p.x).abs() < 1e-2, "{p}");
        assert!(
            (p.x - edge.x).abs() < 1e-3 || (p.y + edge.y).abs() < 1e-3,
            "{p}"
        );
        // Behind the camera, projected up and left (as a point behind comes out,
        // turned through the middle): it is below and to the right of him, inside
        // the box once turned back, and still goes to the edge.
        let (p, arrow) = clamp_marker(Vec2::new(-0.2, 0.1), false, half, edge);
        assert!(p.x > 0.0 && p.y > 0.0, "{p}");
        assert!(
            (p.x - edge.x).abs() < 1e-3 || (p.y - edge.y).abs() < 1e-3,
            "{p}"
        );
        assert!(arrow.is_some());
    }

    /// A scaled sprite with a scaling grid is drawn 9-sliced: the ammo bar's one
    /// segment of a one-round clip (mc_ammoBarSegment's parts, the grid -22.5 to
    /// -9.5 across their -32 to 0, made 185 wide) is the whole bar with its soft
    /// ends 9.5 wide, not its image stretched; unscaled, it is as it was; scaled
    /// smaller than its ends, they shrink together.
    #[test]
    fn scaled_sprites_with_a_scaling_grid_are_nine_sliced() {
        use super::*;
        use bevy::mesh::VertexAttributeValues;
        let scale = 185.0 / 32.0;
        let s = Slice {
            grid: [-22.5, -3.5, -9.5, 3.5],
            bounds: [-32.0, -16.0, 0.0, 16.0],
            scale: Vec2::new(scale, 1.0),
            offset: Vec2::ZERO,
        };
        assert!(s.scaled());
        let at = |v: f32| s.place(0, v) * scale;
        for (v, want) in [(-32.0, -185.0), (-22.5, -175.5), (-9.5, -9.5), (0.0, 0.0)] {
            assert!((at(v) - want).abs() < 1e-3, "{v}: {} not {want}", at(v));
        }
        // (Its height unscaled: as it was.)
        assert!((s.place(1, -16.0) + 16.0).abs() < 1e-4 && (s.place(1, 3.5) - 3.5).abs() < 1e-4);
        // The mesh: cut at the grid's lines, each piece's texture as the image has it.
        let mesh = sliced_quad_mesh([-32.0, -16.0, 0.0, 16.0], [0.0, 0.0, 1.0, 1.0], &s);
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("no uvs");
        };
        let row: Vec<(f32, f32)> = (0..4).map(|i| (pos[i][0] * scale, uv[i][0])).collect();
        let want = [
            (-185.0, 0.0),
            (-175.5, 9.5 / 32.0),
            (-9.5, 22.5 / 32.0),
            (0.0, 1.0),
        ];
        for ((x, u), (wx, wu)) in row.iter().zip(want) {
            assert!((x - wx).abs() < 1e-3 && (u - wu).abs() < 1e-5, "{row:?}");
        }
        assert_eq!(pos.len(), 16);
        // Unscaled: nothing to slice.
        let one = Slice {
            scale: Vec2::ONE,
            ..s
        };
        assert!(!one.scaled());
        assert!((one.place(0, -22.5) + 22.5).abs() < 1e-5);
        // Mirrored and scaled the same: the same 9-slice, mirrored by the sprite.
        let mirrored = Slice {
            scale: Vec2::new(-scale, 1.0),
            ..s
        };
        assert!((mirrored.place(0, -22.5) - s.place(0, -22.5)).abs() < 1e-5);
        // Smaller than its ends (4 and 4 of 10, at half size): they shrink together.
        let small = Slice {
            grid: [4.0, 0.0, 6.0, 1.0],
            bounds: [0.0, 0.0, 10.0, 1.0],
            scale: Vec2::new(0.5, 1.0),
            offset: Vec2::ZERO,
        };
        let at = |v: f32| small.place(0, v) * 0.5;
        for (v, want) in [(0.0, 0.0), (4.0, 2.5), (6.0, 2.5), (10.0, 5.0)] {
            assert!((at(v) - want).abs() < 1e-4, "{v}: {}", at(v));
        }
    }

    /// The ability's JAMMED (jammed_mc) goes as the jam ends: the HUD's
    /// gotoAndPlay("off") lands on the clip's first frame, empty, whose stop()
    /// keeps it there, rather than playing on into "on"'s pulsing JAMMED.
    #[test]
    fn jammed_goes_as_the_jam_ends() {
        use super::*;
        let path = crate::assets_dir().join("hud/hud.txt");
        let Ok(text) = std::fs::read_to_string(&path) else {
            assert!(std::env::var_os("BEVY_ASSET_ROOT").is_none(), "no {path:?}");
            return;
        };
        let lines: Vec<Vec<&str>> = text
            .lines()
            .map(|l| l.split_whitespace().collect())
            .collect();
        let id = lines
            .iter()
            .find(|p| p.len() > 5 && p[0] == "key" && p[5] == "jammed_mc")
            .map(|p| p[4])
            .expect("jammed_mc");
        let mut sprite = SpriteDef {
            frames: 1,
            labels: HashMap::new(),
            stops: Vec::new(),
            gotos: HashMap::new(),
            tracks: BTreeMap::new(),
            masks: Vec::new(),
            grid: None,
        };
        // Its keys, by frame: whether something is placed from there.
        let mut keys: Vec<(usize, bool)> = Vec::new();
        for p in &lines {
            if p.len() < 3 || p[1] != id {
                continue;
            }
            let f: usize = p[2].parse().unwrap_or(0);
            match p[0] {
                "sprite" => {
                    sprite.frames = f.max(1);
                    sprite.stops = vec![false; sprite.frames];
                }
                "label" if p.len() > 3 => {
                    sprite.labels.insert(p[3].to_string(), f);
                }
                "stop" => sprite.stops[f] = true,
                "goto" if p.len() > 3 => {
                    sprite.gotos.insert(f, p[3].to_string());
                }
                "key" if p.len() > 4 => keys.push((f, p[4] != "-")),
                _ => {}
            }
        }
        keys.sort();
        let shows = |frame: usize| {
            keys.iter()
                .take_while(|(f, _)| *f <= frame)
                .last()
                .is_some_and(|(_, placed)| *placed)
        };
        // Its timeline as the clips play it (30 frames a second), `secs` on.
        let run = |(mut frame, mut playing): (usize, bool), secs: f32| {
            for _ in 0..(secs * FPS) as usize {
                if !playing {
                    break;
                }
                let mut next = (frame + 1) % sprite.frames;
                if let Some(&f) = sprite.gotos.get(&next).and_then(|l| sprite.labels.get(l)) {
                    next = f;
                }
                frame = next;
                playing = !sprite.stops[next];
            }
            (frame, playing)
        };
        // Jammed: JAMMED shows, pulsing on for as long as it lasts.
        let on = landing(&sprite, Err("on"), true).expect("on");
        assert!(on.1 && shows(on.0), "{on:?}");
        let jammed = run(on, 10.0);
        assert!(jammed.1 && shows(jammed.0), "{jammed:?}");
        // The jam over: gone, and it stays gone.
        let off = landing(&sprite, Err("off"), true).expect("off");
        assert!(!off.1 && !shows(off.0), "{off:?}");
        let after = run(off, 10.0);
        assert!(!shows(after.0), "{after:?}");
        // gotoAndStop and a frame number land the same way; a label it hasn't
        // got changes nothing.
        assert_eq!(landing(&sprite, Ok(0), true), Some((0, false)));
        assert_eq!(landing(&sprite, Ok(4), false), Some((4, false)));
        assert_eq!(landing(&sprite, Err("nowhere"), true), None);
    }

    /// The pieces' draw orders rise along the list by ORDER_STEP or more; the
    /// ones they have are kept when clips are attached or removed mid-list, the
    /// new pieces fitted between; and orders given come back unchanged.
    #[test]
    fn draw_orders_rise_and_are_kept_round_attached_clips() {
        use super::*;
        let rises = |o: &[f32]| {
            o.iter()
                .all(|&v| v.fract() == 0.0 && v >= ORDER_STEP as f32 && v < ORDER_LIMIT as f32)
                && o.windows(2).all(|p| p[1] - p[0] >= ORDER_STEP as f32)
        };
        // Afresh: spaced by ORDER_SPACING; and kept as they are.
        let fresh = draw_orders(&[0.0; 10]);
        assert_eq!(fresh[0], ORDER_SPACING as f32);
        assert!(rises(&fresh));
        assert_eq!(draw_orders(&fresh), fresh);
        // Three pieces attached mid-list: the rest keep theirs.
        let mut list = fresh.clone();
        list.splice(4..4, [0.0; 3]);
        let got = draw_orders(&list);
        assert!(rises(&got), "{got:?}");
        assert_eq!(&got[..4], &fresh[..4]);
        assert_eq!(&got[7..], &fresh[4..]);
        assert_eq!(draw_orders(&got), got);
        // And removed again: nothing changes.
        let mut removed = got.clone();
        removed.drain(4..7);
        assert_eq!(draw_orders(&removed), fresh);
        // More attached than the gap holds: one or two round them move, the list
        // rises.
        let mut list = fresh.clone();
        list.splice(4..4, [0.0; 30]);
        let got = draw_orders(&list);
        assert!(rises(&got), "{got:?}");
        let moved = |list: &[f32], got: &[f32]| {
            list.iter()
                .zip(got)
                .filter(|(a, b)| **a != 0.0 && a != b)
                .count()
        };
        assert!(moved(&list, &got) <= 2, "{got:?}");
        assert_eq!(draw_orders(&got), got);
        // A piece moved from the end to the front: only it changes.
        let mut list = fresh.clone();
        let last = list.pop().unwrap();
        list.insert(0, last);
        let got = draw_orders(&list);
        assert!(rises(&got), "{got:?}");
        assert_eq!(&got[1..], &fresh[..9]);
        // Added at the end past the limit: all afresh, still rising.
        let mut list = vec![(ORDER_LIMIT - ORDER_STEP) as f32];
        list.push(0.0);
        let got = draw_orders(&list);
        assert!(rises(&got), "{got:?}");
        // Clips coming and going at random: always rising, unchanged when nothing
        // is, and only the pieces attached (or a few round them) given new orders.
        let mut seed = 12345u64;
        let mut random = |n: usize| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % n.max(1)
        };
        let mut list = draw_orders(&[0.0; 200]);
        let (mut most, mut total, mut inserts) = (0, 0, 0);
        for _ in 0..500 {
            let at = random(list.len());
            if random(2) == 0 || list.len() < 50 {
                let k = 1 + random(8);
                list.splice(at..at, std::iter::repeat_n(0.0, k));
                let got = draw_orders(&list);
                assert!(rises(&got));
                let m = moved(&list, &got);
                most = most.max(m);
                total += m;
                inserts += 1;
                list = got;
            } else {
                let k = (1 + random(8)).min(list.len() - at);
                list.drain(at..at + k);
                assert_eq!(draw_orders(&list), list);
            }
            assert_eq!(draw_orders(&list), list);
        }
        // (Pieces packed close by earlier attachings move more: 3.5 a time here,
        // at most 30, where clips come and go anywhere in the list.)
        assert!(
            total <= inserts * 6 && most <= 40,
            "{total} in {inserts}, most {most}"
        );
    }
}
