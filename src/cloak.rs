//! The Scout's Cloak (TransGame.TnAbilityCloaking: a TnAbilityAddBuff putting
//! TnBuffCloak on him), as the game runs it in Escalation:
//!
//! * **The meter** (abilities.rs; "CP,ESC": 10 needed of 14, 2 taken as it starts):
//!   it drains 1 a second while he walks cloaked, 3 while he sprints or dodges or his
//!   vehicle boosts (CloakResourceDatas[1]: 12 s and 4 s from full), and refills at 1
//!   a second. Specter halves the drain (TnBuffCloakResourceDrainRate), Cloak
//!   Intercooler doubles the refill (TnBuffCloakResourceRefillRate).
//! * **Starting** plays Nav_ToStealth over his upper body (main.rs's trigger clip);
//!   TnBuffCloak's ApplyFx, Cloaking_FX (lightning and sparks round him), goes off
//!   on his PositionSocket and follows it, at his robot's size (character.rs's
//!   Robot::effect_scale), and TnCloakMaterialOverrider puts the
//!   cloak on him (TnCharacterMaterialBlueprint's values, SharedCharacterMaterials;
//!   drawn by paint.wgsl from tools/build_cloak.py's textures):
//!   - for CloakActivateDuration (1.5 s) his HmMeshHighlightComponent draws the
//!     overlay, TR_Cloak_Overlay_MATINST, over him, its DefragValue going from 0 to
//!     1: a ragged front out from his right elbow's joint
//!     (CloakOverlayOriginBoneName) to his bounds' radius times 1.2 (ObjectRadius),
//!     glowing blue where it passes, his body black with an orange rim behind it,
//!     burning out to nothing (paint.wgsl's cloak_overlay). Here each of his meshes
//!     gets a second of itself for it (an [`Overlay`]), drawn over it;
//!   - halfway through, his meshes go into the cloak, TR_Cloak_MATINST, and stop
//!     casting shadows (UpdateCloak): nothing of him but the picture behind him
//!     shifted (paint.wgsl's cloak_glass). His decals go as the overlay reaches them.
//!
//!   It goes on into his vehicle.
//! * **Running low** is the campaign's only (UpdateFadeOut runs only if IsCampaign;
//!   Escalation's TnOnlineGameSettingsSV is a GTS_SingleTeamGame), so not in the
//!   demo's matches ([`CAMPAIGN`]): there, with a quarter of his meter left
//!   (CloakFadeOutNormalizedResourceStart), the cloak's Fade_In rises from 0 to 1 as
//!   the meter runs out, its waves coming out over him from the joint and running ever
//!   faster, with the warning (CloakFadeOutSound, ABILITY_CLOAK_POWER_OUT:
//!   BL_TRANS_POWER.CLOAK_AUTOBOT_POWER_OUT).
//! * **Ending**, however it ends (OnStop): he is seen again at once, his own
//!   materials and shadows back, and Cloaking_FX goes off again (UnapplyFx).
//! * **Exposed** (TnPawn.ExposeSelf): a shot, a blow or a grenade of his gives him
//!   away at once, or 1.25 s later with Shadow Stalker (TnBuffCloakExposeDelay);
//!   damage always at once. The cloak ends and he can't cloak again for 1.5 s
//!   (TnBuffDecloak's time; what it does isn't in its script: taken as that).
//! * **Its sounds** (TnBuffCloak's ApplySound and UnapplySound, by his faction):
//!   CLOAK_<AUTOBOT|DECEPTICON>_START_LP looping while he is cloaked, _OFF as it ends.
//!
//! Every PC draws every Jazz's cloak (the buff goes to all: kBuffReplicateToAll), the
//! others' from their snapshots (their meter with them, for running low).

use std::collections::HashMap;
use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{NoFrustumCulling, VisibilityRange};
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::skinning::SkinnedMesh;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::abilities::{ClassAbility, End, Kind, SHADOW_STALKER, Sockets};
use crate::character::{PaintCache, PaintMaterial};
use crate::levelfx::{FxId, FxSystems};
use crate::sound::Sfx;
use crate::{JazzOf, Player, RemotePlayer, RobotForm, VehicleForm};

/// CloakResourceDatas[1]'s SprintDrainRate and BoostDrainRate.
pub const SPRINT_DRAIN: f32 = 3.0;
/// TnBuffCloakExposeDelay's ValuePlusSum (Shadow Stalker) and TnBuffDecloak's BuffTime.
const STALKER_DELAY: f32 = 1.25;
const DECLOAK_SECS: f32 = 1.5;
/// CloakActivateDuration: the overlay's time (its DefragValue 0 to 1 over it); his
/// meshes go into the cloak halfway through (UpdateCloak).
pub const ACTIVATE_SECS: f32 = 1.5;
/// CloakFadeOutNormalizedResourceStart: the share of his meter left as it starts to
/// run low.
const FADE_START: f32 = 0.25;
/// Whether the match is the campaign's (TnCloakMaterialOverrider.IsCampaign), which
/// alone runs the cloak low: never, the demo's matches being Escalation's.
const CAMPAIGN: bool = false;
/// The bone the cloak comes out from (CloakOverlayOriginBoneName), and the overlay's
/// reach over his bounds' radius (UpdateOverlayPosition's ObjectRadius).
const OVERLAY_BONE: &str = "R_CloudA_ElbowRobo12_XT";
const OVERLAY_REACH: f32 = 1.2;
/// TnBuffCloak's ApplyFx and UnapplyFx, on its FxSocket.
const FX: &str = "Cloaking_FX";
const FX_SOCKET: &str = "PositionSocket";
/// The overlay's copies are drawn a little in front of his meshes (their depth's
/// constant bias), so they cover them, and the cloak doesn't show through them.
const OVERLAY_DEPTH_BIAS: f32 = 4.0;
/// TR_Cloak_Overlay_Mat's clip value (OpacityMaskClipValue: paint.wgsl's CLOAK_CLIP).
const OVERLAY_CLIP: f32 = 0.3333;

const SND_LOOP: [&str; 2] = [
    "BL_TRANS_POWER.CLOAK_AUTOBOT_START_LP",
    "BL_TRANS_POWER.CLOAK_DECEPTICON_START_LP",
];
const SND_OFF: [&str; 2] = [
    "BL_TRANS_POWER.CLOAK_AUTOBOT_OFF",
    "BL_TRANS_POWER.CLOAK_DECEPTICON_OFF",
];
const SND_POWER_OUT: &str = "BL_TRANS_POWER.CLOAK_AUTOBOT_POWER_OUT";
/// (Its recording, to cut it short as the cloak ends: StopFadeOut.)
const WAVE_POWER_OUT: &str = "SYNTH_POWERUP_CLOAK_DISRUPT";

/// The cloak's textures (tools/build_cloak.py), and the dead's dissolve's
/// (enemies.rs's: its overlay is the cloak's, with IaconMask_NORM's lines, and
/// ElectricArcs for an electrocution).
const TEXTURES: [&str; 7] = [
    "textures/cloak/WEP_Defrag_Alphas.ktx2",
    "textures/cloak/pulseGradients.png",
    "textures/cloak/DiffClouds_CLR.ktx2",
    "textures/cloak/bubbles.ktx2",
    "textures/cloak/Pixel_4_NORM_DXT1.ktx2",
    "textures/cloak/IaconMask_NORM.ktx2",
    "textures/cloak/ElectricArcs.ktx2",
];

/// A Jazz's cloak: seconds since it went on, an attack's exposure to come (Shadow
/// Stalker: seconds to it), the seconds before he can cloak again, the lightning
/// on him (Cloaking_FX), and whether its warning has sounded.
#[derive(Default)]
pub(crate) struct CloakState {
    pub since: f32,
    pub expose_in: Option<f32>,
    pub lockout: f32,
    fx: Option<FxId>,
    fading: bool,
}

/// He gives himself away (TnPawn.ExposeSelf): damage at once, an attack of his at
/// once or after Shadow Stalker's delay. The cloak ends; he can't cloak again for a
/// moment (TnBuffDecloak).
pub fn expose(a: &mut ClassAbility, damage: bool) {
    if a.kind != Kind::Cloak || !a.active {
        return;
    }
    if !damage && a.has(SHADOW_STALKER) {
        a.cloak.expose_in.get_or_insert(STALKER_DELAY);
        return;
    }
    a.cloak.expose_in = None;
    a.end(End::Forced);
    a.cloak.lockout = DECLOAK_SECS;
}

/// Whether he may cloak now (not while TnBuffDecloak is on him).
pub fn can_start(a: &ClassAbility) -> bool {
    a.cloak.lockout <= 0.0
}

/// How the cloak draws him `since` seconds after it went on with `meter` (0 to 1)
/// of his meter left: the overlay's DefragValue while it is drawn, whether he is in
/// the cloak, and its Fade_In.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Look {
    pub overlay: Option<f32>,
    pub cloaked: bool,
    pub fade: f32,
}

impl Look {
    pub fn at(since: f32, meter: f32) -> Look {
        Look {
            overlay: (since <= ACTIVATE_SECS).then(|| since / ACTIVATE_SECS),
            cloaked: since >= ACTIVATE_SECS * 0.5,
            fade: if CAMPAIGN { fade_in(meter) } else { 0.0 },
        }
    }
}

/// The cloak's Fade_In with `meter` (0 to 1) of his meter left: none above a
/// quarter, then rising to 1 as it runs out (FPctByRange(meter, 0.25, 0)).
pub fn fade_in(meter: f32) -> f32 {
    if meter > FADE_START {
        0.0
    } else {
        (1.0 - meter / FADE_START).clamp(0.0, 1.0)
    }
}

pub struct CloakPlugin;

impl Plugin for CloakPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CloakCopies>()
            .add_systems(Startup, load_textures)
            .add_systems(
                Update,
                cloak_looks
                    .after(crate::abilities::AbilitySystems)
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

/// Every Jazz's cloak: an attack's exposure coming due and the lockout running out
/// (his PC's), its clock, his trigger clip as it starts, its lightning as it starts
/// and ends, and its sounds.
pub(crate) fn run_cloak(
    time: Res<Time>,
    mut sfx: ResMut<Sfx>,
    mut fx: ResMut<FxSystems>,
    mut sockets: Sockets,
    mut players: Query<(Entity, &Transform, &mut Player, Has<RemotePlayer>)>,
) {
    let dt = time.delta_secs();
    for (entity, transform, mut player, remote) in &mut players {
        let player = &mut *player;
        let feet = transform.translation;
        // (His lightning on his socket, wherever he is: his middle without it.)
        let socket = sockets
            .frame(entity, FX_SOCKET)
            .map(|(at, x, _)| (at, x))
            .unwrap_or((player.centre(feet), *transform.forward()));
        if let Some(id) = player.ability.cloak.fx {
            if fx.alive(id) {
                fx.place(id, socket.0, socket.1, Vec3::ZERO);
            } else {
                player.ability.cloak.fx = None;
            }
        }
        if player.ability.kind != Kind::Cloak {
            continue;
        }
        let side = usize::from(player.look.decepticon);
        let a = &mut player.ability;
        if !remote {
            a.cloak.lockout = (a.cloak.lockout - dt).max(0.0);
            if let Some(left) = a.cloak.expose_in.as_mut() {
                *left -= dt;
                if *left <= 0.0 {
                    a.cloak.expose_in = None;
                    if a.active {
                        a.end(End::Forced);
                        a.cloak.lockout = DECLOAK_SECS;
                    }
                }
            }
            if !a.active {
                a.cloak.expose_in = None;
            }
        }
        let owner = entity.to_bits();
        if a.started {
            a.cloak.since = 0.0;
            a.cloak.fading = false;
            player.support.trigger_anim = Some(0.0);
        } else if a.active {
            a.cloak.since += dt;
        }
        let a = &mut player.ability;
        // (Its lightning as it goes on and as it comes off.)
        if a.started || a.ended.is_some() {
            let size = player.kit.robot.effect_scale();
            a.cloak.fx = fx.spawn_scaled(FX, socket.0, socket.1, size, &[]);
        }
        let at = remote.then(|| feet + Vec3::Y);
        if a.active {
            sfx.hold(owner ^ 0x434c4b, SND_LOOP[side], at);
        }
        // (His PC's sounds reach the others' as his.)
        if !remote {
            let meter = a.resource / a.kind.meter().max;
            if CAMPAIGN && a.active && !a.cloak.fading && fade_in(meter) > 0.0 {
                a.cloak.fading = true;
                sfx.play_owned(owner, SND_POWER_OUT, 0.0, 1.0, None);
            }
            if a.ended.is_some() {
                if a.cloak.fading {
                    sfx.cut(owner, WAVE_POWER_OUT);
                }
                sfx.play(SND_OFF[side]);
            }
        }
        if !a.active {
            a.cloak.fading = false;
        }
    }
}

/// The cloak's textures (tools/build_cloak.py's), repeating as the game samples
/// them; the dissolve's too.
#[derive(Resource)]
pub(crate) struct CloakTextures([Handle<Image>; 7]);

impl CloakTextures {
    /// A dissolving robot's material given the dissolve's textures: the overlay's
    /// (WEP_Defrag_Alphas and the gradients) and its own.
    pub fn give_dissolve(&self, m: &mut crate::character::Paint) {
        let [alphas, pulse, _, _, _, lines, arcs] = self.0.clone();
        m.cloak_alphas = Some(alphas);
        m.cloak_pulse = Some(pulse);
        m.defrag_lines = Some(lines);
        m.defrag_arcs = Some(arcs);
    }
}

fn load_textures(mut commands: Commands, asset_server: Res<AssetServer>) {
    let root = crate::assets_dir();
    let textures = TEXTURES.map(|path| {
        if !root.join(Path::new(path)).exists() {
            warn!("No {path} (tools/build_cloak.py makes it): the cloak and the dissolve can't be drawn as the game draws them");
        }
        asset_server.load_with_settings(path, |s: &mut ImageLoaderSettings| {
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..ImageSamplerDescriptor::linear()
            });
        })
    });
    commands.insert_resource(CloakTextures(textures));
}

/// The cloak's look on the Jazzes in it (see the top): each of his meshes'
/// overlay, his meshes' cloak, his decals, and Bevy's copy of the picture behind
/// him for the cloak to shift.
#[derive(Resource, Default)]
struct CloakCopies {
    jazzes: HashMap<Entity, Copies>,
    /// The invisible transmissive triangle that keeps Bevy copying the picture
    /// (levelfx.rs's scene copy), while anyone is in the cloak.
    scene_copy: Option<Entity>,
}

/// A cloaking Jazz's: by the paint each copies, the overlay's copy and the cloak's;
/// and each of his meshes' overlay.
#[derive(Default)]
struct Copies {
    paints: HashMap<AssetId<PaintMaterial>, [Handle<PaintMaterial>; 2]>,
    overlays: HashMap<Entity, Entity>,
}

/// A second of one of his meshes, drawn over it with the overlay.
#[derive(Component)]
struct Overlay;

/// A mesh of his in the cloak: what it had, the paint the cloak copied, and whether
/// it cast shadows.
#[derive(Component)]
struct Cloaked {
    had: Had,
    own: Handle<PaintMaterial>,
    shadows: bool,
}

enum Had {
    Paint(Handle<PaintMaterial>),
    Plain(Handle<StandardMaterial>),
}

/// What paint.wgsl's cloak_fx says each copy draws.
const DRAW_OVERLAY: f32 = 1.0;
const DRAW_CLOAK: f32 = 2.0;

type MeshParts<'a> = (
    Option<&'a MeshMaterial3d<PaintMaterial>>,
    Option<&'a MeshMaterial3d<StandardMaterial>>,
    Option<&'a Cloaked>,
    Has<NotShadowCaster>,
    Has<Overlay>,
);

type Seconds<'a> = (
    &'a Mesh3d,
    &'a Transform,
    Option<&'a ChildOf>,
    Option<&'a SkinnedMesh>,
    Option<&'a Visibility>,
    Option<&'a VisibilityRange>,
    Has<NoFrustumCulling>,
);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn cloak_looks(
    mut commands: Commands,
    mut copies: ResMut<CloakCopies>,
    textures: Option<Res<CloakTextures>>,
    players: Query<(Entity, &Transform, &Player)>,
    models: Query<(Entity, &JazzOf, Has<RobotForm>), Or<(With<RobotForm>, With<VehicleForm>)>>,
    children: Query<&Children>,
    meshes: Query<MeshParts, With<Mesh3d>>,
    seconds: Query<Seconds, (Without<Overlay>, Without<crate::decals::Decal>)>,
    mut overlay_parts: Query<(&mut Visibility, &MeshMaterial3d<PaintMaterial>), With<Overlay>>,
    names: Query<(&Name, &GlobalTransform)>,
    bounds: Query<(&Aabb, &GlobalTransform), (With<Mesh3d>, Without<Overlay>)>,
    mut paints: ResMut<Assets<PaintMaterial>>,
    mut cache: ResMut<PaintCache>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    mut decals: Query<
        (&GlobalTransform, &mut Visibility),
        (With<crate::decals::Decal>, Without<Overlay>),
    >,
) {
    let mut any_cloaked = false;
    for (jazz, transform, player) in &players {
        let a = &player.ability;
        let look = (a.kind == Kind::Cloak && a.active)
            .then(|| Look::at(a.cloak.since, a.resource / a.kind.meter().max));
        if look.is_none() && !copies.jazzes.contains_key(&jazz) {
            continue;
        }
        let his: Vec<(Entity, bool)> = models
            .iter()
            .filter(|(_, of, _)| of.0 == jazz)
            .flat_map(|(m, _, robot)| children.iter_descendants(m).map(move |e| (e, robot)))
            .collect();
        let Some(look) = look else {
            // Seen again (OnStop): his own materials and shadows back, the overlay
            // gone, his decals shown.
            if let Some(c) = copies.jazzes.remove(&jazz) {
                for overlay in c.overlays.into_values() {
                    commands.entity(overlay).try_despawn();
                }
            }
            for &(e, _) in &his {
                if let Ok((_, _, Some(c), _, _)) = meshes.get(e) {
                    restore(&mut commands, e, c);
                }
            }
            crate::decals::hide_within(his.iter().map(|&(e, _)| e), &mut decals, None);
            continue;
        };
        any_cloaked |= look.cloaked;

        // Where it comes out from (his robot's elbow; his middle without it), and
        // how far the overlay reaches: his robot's bounds' radius times 1.2.
        let joint = his
            .iter()
            .filter(|(_, robot)| *robot)
            .find_map(|&(e, _)| {
                names
                    .get(e)
                    .ok()
                    .filter(|(n, _)| n.as_str() == OVERLAY_BONE)
                    .map(|(_, g)| g.translation())
            })
            .unwrap_or(transform.translation + Vec3::Y * player.kit.robot.height * 0.6);
        let radius = robot_radius(his.iter().filter(|(_, r)| *r).map(|&(e, _)| e), &bounds)
            .unwrap_or(player.kit.robot.height * 0.9)
            * OVERLAY_REACH;
        // (His decals go as the overlay's front reaches them, and all of them in
        // the cloak: the front is where the noise's k, at least the DefragValue to
        // the 0.75th, has come out to: 0.75 of the reach at 1.)
        let front = match look.overlay {
            _ if look.cloaked => f32::MAX,
            Some(d) => d.powf(0.75) / 0.75 * radius,
            None => f32::MAX,
        };
        crate::decals::hide_within(
            his.iter().map(|&(e, _)| e),
            &mut decals,
            Some((joint, front)),
        );

        let c = copies.jazzes.entry(jazz).or_default();
        let mut kept = Vec::new();
        for &(e, _) in &his {
            let Ok((paint, plain, cloaked, no_shadow, overlay)) = meshes.get(e) else {
                continue;
            };
            if overlay {
                continue;
            }
            // The paint his copies copy: the one it had (or has, if something has
            // given it another since it went into the cloak), a plain material's
            // painted.
            let current = match (cloaked, paint) {
                (Some(c), Some(p)) if c.own == p.0 || is_copy(&paints, &p.0) => Some(c.own.clone()),
                _ => None,
            };
            let (own, had) = match (current, paint, plain) {
                (Some(own), ..) => (own, None),
                (None, Some(p), _) => (p.0.clone(), Some(Had::Paint(p.0.clone()))),
                (None, None, Some(s)) => {
                    let Some(h) = cache.colored(
                        &s.0,
                        (Vec3::ONE, Vec3::ONE, player.look.energon()),
                        false,
                        &standard,
                        &mut paints,
                    ) else {
                        continue;
                    };
                    (h, Some(Had::Plain(s.0.clone())))
                }
                _ => continue,
            };
            let Some([over, glass]) = copies_of(c, &own, textures.as_deref(), &mut paints) else {
                continue;
            };

            // Its overlay, while it is drawn.
            if look.overlay.is_some() {
                match c.overlays.get(&e).copied() {
                    Some(o) if overlay_parts.contains(o) => {
                        if let (Ok(mut vis), Ok(s)) = (overlay_parts.get_mut(o), seconds.get(e)) {
                            let want = s.4.copied().unwrap_or_default();
                            vis.0.set_if_neq(want);
                            if vis.1.0 != over {
                                commands.entity(o).try_insert(MeshMaterial3d(over.clone()));
                            }
                        }
                    }
                    _ => {
                        if let Ok(s) = seconds.get(e)
                            && let Some(o) = spawn_overlay(&mut commands, s, over.clone())
                        {
                            c.overlays.insert(e, o);
                        }
                    }
                }
                kept.push(e);
            }

            // Into the cloak, halfway through.
            if look.cloaked {
                let on_glass = paint.is_some_and(|p| p.0 == glass);
                if !on_glass {
                    let mut ec = commands.entity(e);
                    if let Some(had) = had {
                        if plain.is_some() && paint.is_none() {
                            ec.try_remove::<MeshMaterial3d<StandardMaterial>>();
                        }
                        let shadows = match cloaked {
                            Some(old) => old.shadows,
                            None => !no_shadow,
                        };
                        ec.try_insert(Cloaked {
                            had,
                            own: own.clone(),
                            shadows,
                        });
                    }
                    ec.try_insert((MeshMaterial3d(glass), NotShadowCaster));
                }
            } else if let Some(cl) = cloaked {
                restore(&mut commands, e, cl);
            }
        }
        // (The overlays of meshes gone, or of an overlay that is over.)
        c.overlays.retain(|mesh, o| {
            let keep = kept.contains(mesh);
            if !keep {
                commands.entity(*o).try_despawn();
            }
            keep
        });

        // What every copy draws now.
        let cloak = joint.extend(radius);
        for [over, glass] in c.paints.values() {
            for (h, what) in [(over, DRAW_OVERLAY), (glass, DRAW_CLOAK)] {
                let fx = Vec4::new(what, look.overlay.unwrap_or(1.0), look.fade, 0.0);
                if paints.get(h).is_some_and(|m| {
                    m.extension.colors.cloak != cloak || m.extension.colors.cloak_fx != fx
                }) && let Some(m) = paints.get_mut(h)
                {
                    m.extension.colors.cloak = cloak;
                    m.extension.colors.cloak_fx = fx;
                }
            }
        }
    }
    // (Jazzes gone: their overlays go too.)
    copies.jazzes.retain(|e, c| {
        let keep = players.contains(*e);
        if !keep {
            for o in c.overlays.values() {
                commands.entity(*o).try_despawn();
            }
        }
        keep
    });

    // Bevy keeps a copy of the picture for the cloak to shift while something
    // transmissive is drawn: an invisible (zero-area) transmissive triangle.
    match (any_cloaked, copies.scene_copy) {
        (true, None) => {
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3]);
            mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
            copies.scene_copy = Some(
                commands
                    .spawn((
                        Mesh3d(mesh_assets.add(mesh)),
                        MeshMaterial3d(standard.add(StandardMaterial {
                            base_color: Color::NONE,
                            specular_transmission: 1.0,
                            unlit: true,
                            ..default()
                        })),
                        Transform::default(),
                        NoFrustumCulling,
                        NotShadowCaster,
                        NotShadowReceiver,
                    ))
                    .id(),
            );
        }
        (false, Some(e)) => {
            commands.entity(e).try_despawn();
            copies.scene_copy = None;
        }
        _ => {}
    }
}

/// A mesh out of the cloak: what it had back, and its shadows.
fn restore(commands: &mut Commands, e: Entity, c: &Cloaked) {
    let mut ec = commands.entity(e);
    match &c.had {
        Had::Paint(p) => {
            ec.try_insert(MeshMaterial3d(p.clone()));
        }
        Had::Plain(s) => {
            ec.try_remove::<MeshMaterial3d<PaintMaterial>>()
                .try_insert(MeshMaterial3d(s.clone()));
        }
    }
    if c.shadows {
        ec.try_remove::<NotShadowCaster>();
    }
    ec.try_remove::<Cloaked>();
}

/// Whether a paint is one of the cloak's copies.
fn is_copy(paints: &Assets<PaintMaterial>, h: &Handle<PaintMaterial>) -> bool {
    paints
        .get(h)
        .is_some_and(|m| m.extension.colors.cloak.w >= 0.0)
}

/// The overlay's copy of `own` and the cloak's (made the first time): its own
/// textures and paint, masked and drawn a little in front for the overlay,
/// see-through (premultiplied: what it adds over the picture) for the cloak.
fn copies_of(
    c: &mut Copies,
    own: &Handle<PaintMaterial>,
    textures: Option<&CloakTextures>,
    paints: &mut Assets<PaintMaterial>,
) -> Option<[Handle<PaintMaterial>; 2]> {
    if let Some(found) = c.paints.get(&own.id()) {
        return Some(found.clone());
    }
    let base = paints.get(own)?.clone();
    let make = |alpha_mode: AlphaMode, depth_bias: f32| {
        let mut m = base.clone();
        m.base.alpha_mode = alpha_mode;
        m.base.depth_bias = depth_bias;
        m.extension.colors.cloak = Vec4::new(0.0, 0.0, 0.0, 1.0);
        if let Some(t) = textures {
            let [alphas, pulse, clouds, bubbles, pixels, ..] = t.0.clone();
            m.extension.cloak_alphas = Some(alphas);
            m.extension.cloak_pulse = Some(pulse);
            m.extension.cloak_clouds = Some(clouds);
            m.extension.cloak_bubbles = Some(bubbles);
            m.extension.cloak_pixels = Some(pixels);
        }
        m
    };
    let made = [
        paints.add(make(AlphaMode::Mask(OVERLAY_CLIP), OVERLAY_DEPTH_BIAS)),
        paints.add(make(AlphaMode::Premultiplied, 0.0)),
    ];
    c.paints.insert(own.id(), made.clone());
    Some(made)
}

/// A second of mesh `s` for the overlay: the same mesh, skinned to the same bones,
/// where it is and shown as it is.
#[allow(clippy::type_complexity)]
fn spawn_overlay(
    commands: &mut Commands,
    (mesh, transform, parent, skin, visibility, range, no_culling): (
        &Mesh3d,
        &Transform,
        Option<&ChildOf>,
        Option<&SkinnedMesh>,
        Option<&Visibility>,
        Option<&VisibilityRange>,
        bool,
    ),
    material: Handle<PaintMaterial>,
) -> Option<Entity> {
    let parent = parent?.parent();
    let mut ec = commands.spawn((
        Overlay,
        Mesh3d(mesh.0.clone()),
        MeshMaterial3d(material),
        *transform,
        visibility.copied().unwrap_or_default(),
        NotShadowCaster,
        ChildOf(parent),
    ));
    if let Some(skin) = skin {
        ec.insert(skin.clone());
    }
    if let Some(range) = range {
        ec.insert(range.clone());
    }
    if no_culling {
        ec.insert(NoFrustumCulling);
    }
    Some(ec.id())
}

/// His robot's bounds' radius (its meshes' boxes together: the sphere round them).
#[allow(clippy::type_complexity)]
fn robot_radius(
    meshes: impl Iterator<Item = Entity>,
    bounds: &Query<(&Aabb, &GlobalTransform), (With<Mesh3d>, Without<Overlay>)>,
) -> Option<f32> {
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for e in meshes {
        let Ok((aabb, g)) = bounds.get(e) else {
            continue;
        };
        let (c, h) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
        for i in 0..8 {
            let corner = c + h * Vec3::new(
                if i & 1 == 0 { -1.0 } else { 1.0 },
                if i & 2 == 0 { -1.0 } else { 1.0 },
                if i & 4 == 0 { -1.0 } else { 1.0 },
            );
            let p = g.transform_point(corner);
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    (lo.x <= hi.x).then(|| (hi - lo).length() * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposure() {
        let mut a = ClassAbility::new(Kind::Cloak, 0);
        a.active = true;
        expose(&mut a, false);
        assert!(!a.active && a.cloak.lockout > 0.0 && !can_start(&a));
        let mut a = ClassAbility::new(Kind::Cloak, SHADOW_STALKER);
        a.active = true;
        expose(&mut a, false);
        assert!(a.active && a.cloak.expose_in == Some(STALKER_DELAY));
        expose(&mut a, true);
        assert!(!a.active);
    }

    #[test]
    fn it_goes_on_as_the_overrider_puts_it_on() {
        // The overlay from the start, its DefragValue rising over 1.5 s...
        let start = Look::at(0.0, 1.0);
        assert_eq!(start.overlay, Some(0.0));
        assert!(!start.cloaked);
        // ...his meshes into the cloak halfway through...
        let half = Look::at(0.75, 0.9);
        assert!(half.cloaked && (half.overlay.unwrap() - 0.5).abs() < 1e-6);
        assert!(!Look::at(0.7, 0.9).cloaked);
        // ...and the overlay over at 1.5 s.
        assert_eq!(Look::at(1.5, 0.8).overlay, Some(1.0));
        assert_eq!(Look::at(1.6, 0.8).overlay, None);
        assert!(Look::at(1.6, 0.8).cloaked);
    }

    #[test]
    fn it_fades_in_its_waves_over_the_last_quarter_of_the_meter_in_the_campaign_only() {
        assert_eq!(fade_in(1.0), 0.0);
        assert_eq!(fade_in(0.26), 0.0);
        assert!(fade_in(0.25).abs() < 1e-6);
        assert!((fade_in(0.125) - 0.5).abs() < 1e-6);
        assert_eq!(fade_in(0.0), 1.0);
        // (12 s of a full meter's walk: 3.5 of its 14 left with 3.5 s to go.) Only
        // in the campaign: Escalation's cloak never runs low.
        assert_eq!(Look::at(8.0, 3.5 / 14.0).fade, 0.0);
        assert_eq!(Look::at(9.0, 2.5 / 14.0).fade > 0.0, CAMPAIGN);
        assert_eq!(Look::at(11.9, 0.01).fade > 0.0, CAMPAIGN);
    }
}
