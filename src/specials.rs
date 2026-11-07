//! The weapon upgrades' special shots (upgrades.rs's LastShotSpecial and
//! RandomShotSpecial), as the game fires them: TnWeapon.FireAmmunition makes a
//! shot the special one when the clip holds `_SpecialShotInClip` rounds as it fires
//! (1, the clip's last: Lights Out and Planet Buster, ClientInstallModification) or
//! by the weapon's RandomShotSpecialPct (Fireblast), firing it in the weapon's
//! special fire mode (GetFiringModeForSpecialShot: 1; the Path Blaster's 2 in a team
//! match, TnWeaponHeavyPistol's, as multiplayer's upgrades are). The special mode
//! has its own effects (the weapon mesh's FireModeEffects for it, or mode 0's when
//! it has none: GetFireModeEffectIndex), camera shake (its CameraShakes entry, the
//! last one past the end) and kick (the hip's: there is no FineAimCameraRecoils
//! entry for it), and does what it does:
//!
//! * **Lights Out** (the Path Blaster, fire mode 2): no damage of its own (mode 2
//!   has no InstantHitDamage), but where it strikes - or at its range's end - a
//!   shockwave (TnWeaponHeavyPistol.ProcessInstantHit: HurtRadius of its
//!   TnWeaponDataShockwaveBulletExtension) of 450 to everything within 15 m
//!   (HeavyPistol_SV_WEPDATA's, the demo's Path Blaster data), at full strength
//!   (the weapon's GetRadiusDamageScale is Actor's 1), through walls
//!   (IgnoreHurtRadiusTrace) and not to him (the shooter is its IgnoredActor). It
//!   flashes as the Nucleon Charge Rifle does (MuzzleFlash_Sniper_FX), its tracer is
//!   the Gambler's (Tracer_Gambler_FX), its impact the bright blast of
//!   Exp_LastShot_FX with EXPL_UPGRADE (ForceDefaultFXToAlwaysPlay), its shot
//!   SHOOT_UPGRADE, its shell AssaultRifleArm_Shell_FX, and it shakes the view
//!   harder (CameraShakes[1]).
//! * **Planet Buster** (the Riot Cannon, fire mode 1): its round is the special
//!   one (riot.rs's PLANET_BUSTER); its flash and sound are the cannon's own.
//! * **Fireblast** (the Scatter Blaster, fire mode 1, a quarter of its shots:
//!   Shotgun_SV_WEPDATA's RandomShotSpecialPct): the same pellets and damage, with
//!   the fire effects (AssaultRifleArm_MuzzleFlash_FX, Trail_ScatterBlaster_SP_FX,
//!   Squib_ScatterBlaster_SP_FX), and every pellet that wounds sets its enemy on
//!   fire (TnDamageTypeFireShotgun's VictimBuffs): it burns for 2 s at 15 a second
//!   (TnBuffDotFromFireShotgun, multiplayer's; a further pellet starts it over),
//!   with Grimlock's flamethrower fire on it while it burns (TnBuffOnFire's ApplyFx,
//!   Flamethrower_Impact_FX), and it carries an explosion for the rest of its life
//!   (TnBuffExplodeOnDeathFire, BuffTime -1): when it dies, 50 to everything within
//!   25 m that can be seen from its middle (HurtRadius from its TargetingSocket, at
//!   full strength), not the shooter (its IgnoredActor) - the burning one's own
//!   blast, instigated by its own controller (the buff's OwnerController), so it
//!   hurts the burning one's enemies: the players.
//!
//! The HUD shows the special shot in the clip (TnHudDataObserverSpecialShotInClip:
//! the ammo bar's segment for it flashes, hud.rs).

use bevy::prelude::*;

use crate::enemies::{Death, Hit};
use crate::levelfx::{FxId, FxSystems};
use crate::sound::Sfx;
use crate::{CameraShake, Enemy, LocalPlayer, Player};

/// A special shot's own effects (its fire mode's FireModeEffects), the game's
/// particle systems (fx/weapons_fx.txt, levelfx.rs): the muzzle flash, the tracer,
/// the impact (along the surface's normal), the shell out of the ShellEject
/// socket ("" for the gun's own); its shot and impact sounds ("" for the gun's
/// own).
pub(crate) struct SpecialFx {
    pub muzzle: &'static str,
    pub tracer: &'static str,
    pub impact: &'static str,
    pub shell: &'static str,
    pub sound_shot: &'static str,
    pub sound_impact: &'static str,
}

/// The Path Blaster's Lights Out (HeavyPistol_SV_WEPDATA's fire mode 2).
pub(crate) const LIGHTS_OUT_FX: SpecialFx = SpecialFx {
    muzzle: "MuzzleFlash_Sniper_FX",
    tracer: "Tracer_Gambler_FX",
    impact: "Exp_LastShot_FX",
    shell: "AssaultRifleArm_Shell_FX",
    sound_shot: "BL_WPN_GUN_BATTLE_PISTOL.SHOOT_UPGRADE",
    sound_impact: "BL_WPN_GUN_BATTLE_PISTOL.EXPL_UPGRADE",
};

/// The Scatter Blaster's Fireblast (Shotgun_SV_WEPDATA's fire mode 1).
pub(crate) const FIREBLAST_FX: SpecialFx = SpecialFx {
    muzzle: "AssaultRifleArm_MuzzleFlash_FX",
    tracer: "Trail_ScatterBlaster_SP_FX",
    impact: "Squib_ScatterBlaster_SP_FX",
    shell: "",
    sound_shot: "",
    sound_impact: "",
};

/// A weapon's special shot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Special {
    /// The Path Blaster's (HeavyPistolLastShotInClip).
    LightsOut,
    /// The Riot Cannon's (BazookaLastShotInClip).
    PlanetBuster,
    /// The Scatter Blaster's (ShotgunRandomSpecial).
    Fireblast,
}

impl Special {
    /// The special shot of weapon `id` (main.rs's WeaponStats::id), if it has one.
    pub fn of(id: &str) -> Option<Special> {
        match id {
            "HeavyPistol" => Some(Special::LightsOut),
            "Bazooka" => Some(Special::PlanetBuster),
            "Shotgun" => Some(Special::Fireblast),
            _ => None,
        }
    }

    /// Its own effects, for an instant-hit gun's.
    pub fn fx(self) -> Option<&'static SpecialFx> {
        match self {
            Special::LightsOut => Some(&LIGHTS_OUT_FX),
            Special::Fireblast => Some(&FIREBLAST_FX),
            Special::PlanetBuster => None,
        }
    }

    /// Its camera shake, if not the gun's own.
    pub fn shake(self) -> Option<CameraShake> {
        match self {
            Special::LightsOut => Some(LIGHTS_OUT_SHAKE),
            _ => None,
        }
    }

    /// Its round, for a gun whose rounds fly.
    pub fn round(self) -> Option<&'static crate::riot::RoundKind> {
        match self {
            Special::PlanetBuster => Some(&crate::riot::PLANET_BUSTER),
            _ => None,
        }
    }
}

/// Lights Out's shockwave: its damage and reach (ShockwaveDamage, ShockwaveRadius).
pub(crate) const SHOCKWAVE_DAMAGE: f32 = 450.0;
pub(crate) const SHOCKWAVE_RADIUS: f32 = 15.0;

/// Lights Out's camera shake (HeavyPistol_SV_WEPDATA's CameraShakes[1]: intensity
/// 1.25, rolling 0.75, falling off over 0.85 s, the roll over 0.75 s).
pub(crate) const LIGHTS_OUT_SHAKE: CameraShake = CameraShake {
    intensity: 1.25,
    roll: 0.75,
    roll_secs: 0.75,
    secs: 0.85,
};

/// Fireblast's chance a shot (Shotgun_SV_WEPDATA's RandomShotSpecialPct).
pub(crate) const FIREBLAST_CHANCE: f32 = 0.25;

/// The fire's burn (TnBuffDotFromFireShotgun, multiplayer's: DamagePerSecond[0],
/// BuffTime[0]) and the effect on what burns (TnBuffOnFire's ApplyFx).
pub(crate) const BURN_DPS: f32 = 15.0;
pub(crate) const BURN_SECS: f32 = 2.0;
const BURN_FX: &str = "Flamethrower_Impact_FX";

/// The explosion a burnt enemy carries (TnBuffExplodeOnDeathFire: Damage,
/// DamageRadius; UseDamageFalloff false).
pub(crate) const DEATH_BLAST_DAMAGE: f32 = 50.0;
pub(crate) const DEATH_BLAST_RADIUS: f32 = 25.0;

/// An enemy's fire from Fireblast, if it has been set alight, or from a trap's flame
/// jets.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Burn {
    /// Seconds left of its burn (0: not burning).
    pub left: f32,
    /// The Jazz whose pellet last set it alight (the burn's Instigator), whom its
    /// explosion leaves out.
    pub by: Option<Entity>,
    /// It carries the explosion (for the rest of its life, once set alight).
    pub bomb: bool,
    /// Seconds left of a flame jet's fire on it (TnBuffInFlameZone, renewed as the
    /// flames hurt it: traps.rs): Grimlock's fire on it and the explosion in it
    /// while it lasts, though no burn of its own.
    pub zone: f32,
    /// The fire on it while it burns.
    fx: Option<FxId>,
}

impl Burn {
    /// A Fireblast pellet fired by `by` (his Jazz) has wounded it: it burns afresh
    /// for the burn's full time, and carries the explosion.
    pub fn ignite(&mut self, by: Entity) {
        self.left = BURN_SECS;
        self.by = Some(by);
        self.bomb = true;
    }

    /// A trap's flame jets have hurt it (TnDamageTypeFlameZone's TnBuffInFlameZone):
    /// their fire on it, with its explosion in it, for FLAME_ZONE_SECS from now.
    pub fn in_flames(&mut self) {
        self.zone = FLAME_ZONE_SECS;
    }

    /// It is on fire (a burn, or a flame jet's).
    fn alight(&self) -> bool {
        self.left > 0.0 || self.zone > 0.0
    }
}

/// How long a flame jet's fire stays on what it hurt (TnBuffInFlameZone's BuffTime:
/// the jets' pain comes every second, so it stays while it is in them).
pub(crate) const FLAME_ZONE_SECS: f32 = 1.0;

/// Explosions of burnt enemies that have died this frame: where (their middle),
/// whom they leave out (the one who set them alight) and whose they are (the
/// enemy's type: the kill feed's killer, its blast TnDamageTypeFire).
#[derive(Resource, Default)]
struct DeathBlasts(Vec<(Vec3, Option<Entity>, Option<usize>)>);

pub struct SpecialsPlugin;

impl Plugin for SpecialsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DeathBlasts>()
            .add_observer(enemy_gone)
            .add_systems(
                Update,
                (burning_enemies, death_blasts)
                    .chain()
                    .after(crate::fire)
                    .before(crate::hud::HudSystems)
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

/// Lights Out's shockwave at `at`: its damage to every enemy within its reach (into
/// what it overlaps, CollidingActors: an enemy's body), more to one the Orbital
/// Pistol has marked (its TnDamageTypeShockwaveGun's deaths). How many it wounded.
pub(crate) fn shockwave(
    enemies: &mut Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
    (at, by): (Vec3, Option<Entity>),
) -> u32 {
    let mut wounded = 0;
    for (_, _, mut enemy) in enemies.iter_mut() {
        let reach = enemy.reach(at);
        if !enemy.alive() || reach > SHOCKWAVE_RADIUS {
            continue;
        }
        let centre = enemy.centre();
        enemy.take(
            SHOCKWAVE_DAMAGE,
            by,
            crate::xp::Blow::Weapon,
            // (TnDamageTypeShockwaveGun: TnDamageType's words.)
            Hit::new(Death::Shockwave, centre, centre - at).of_type("TnDamageType"),
        );
        wounded += 1;
    }
    wounded
}

/// The burning enemies: each burns for its time, taking its damage a frame at a
/// time (TnBuffDot.TickBuff: DamagePerSecond times the frame, more if it is marked),
/// with the fire on its middle, which goes out with the burn (TnBuffDotFromFire.
/// OnUnapplyOrOwnerDied: TnBuffOnFire comes off). One that burns to death is gone
/// (and goes off: enemy_gone).
fn burning_enemies(
    time: Res<Time>,
    mut fx: ResMut<FxSystems>,
    mut enemies: Query<&mut Enemy>,
    mut players: Query<&mut Player>,
) {
    let dt = time.delta_secs();
    for mut enemy in &mut enemies {
        let at = enemy.centre();
        // (A flame jet's fire runs out while it lives; dead, it keeps it, and its
        // explosion goes off as it goes: enemy_gone.)
        if enemy.burn.zone > 0.0 && enemy.alive() {
            enemy.burn.zone = (enemy.burn.zone - dt).max(0.0);
        }
        if enemy.burn.left > 0.0 && enemy.alive() {
            let secs = enemy.burn.left.min(dt);
            enemy.burn.left -= secs;
            let by = enemy.burn.by;
            // (TnDamageTypeFireDot: it dies of fire.)
            let dealt = enemy.take(
                BURN_DPS * secs,
                by,
                crate::xp::Blow::Burn,
                Hit::new(Death::Fire, at, Vec3::ZERO).of_type("TnDamageTypeFireDot"),
            );
            // (His fire this life: OnDamage's AccumulateFireShotgunDamageForXpEvent.)
            if let Some(mut p) = by.and_then(|b| players.get_mut(b).ok()) {
                p.fire_damage += dealt;
            }
        }
        let burning = enemy.burn.alight() && enemy.health > 0.0;
        match (burning, enemy.burn.fx) {
            (true, None) => enemy.burn.fx = fx.spawn(BURN_FX, at, Vec3::Y, &[]),
            (true, Some(id)) => fx.place(id, at, Vec3::Y, Vec3::ZERO),
            (false, Some(id)) => {
                fx.stop(id);
                enemy.burn.fx = None;
            }
            (false, None) => {}
        }
    }
}

/// An enemy going away: one that has died with the explosion in it goes off
/// (death_blasts), and the fire on it goes out with it. (An enemy taken away
/// alive, as a match ends, doesn't.)
fn enemy_gone(
    gone: On<Remove, Enemy>,
    mut fx: ResMut<FxSystems>,
    mut blasts: ResMut<DeathBlasts>,
    enemies: Query<(&Enemy, Option<&crate::enemies::Foe>)>,
) {
    let Ok((enemy, foe)) = enemies.get(gone.entity) else {
        return;
    };
    if let Some(id) = enemy.burn.fx {
        fx.stop(id);
    }
    // (A flame jet's explosion goes with its fire: TnBuffInFlameZone's.)
    if enemy.health <= 0.0 && (enemy.burn.bomb || enemy.burn.zone > 0.0) {
        blasts
            .0
            .push((enemy.centre(), enemy.burn.by, foe.map(|f| f.kind)));
    }
}

/// The burnt enemies' explosions (TnBuffExplodeOnDeathBase.Explode): each hurts the
/// player at this PC (every PC does its own) if he is within its reach and in sight
/// of it (HurtRadius traces from where it went off), unless he set it alight; the
/// HUD's damage arrow points back at it.
fn death_blasts(
    mut blasts: ResMut<DeathBlasts>,
    level: Res<crate::Level>,
    colliders: Res<crate::Colliders>,
    mut sfx: ResMut<Sfx>,
    mut players: Query<(Entity, &Transform, &mut Player), With<LocalPlayer>>,
) {
    for (at, by, kind) in blasts.0.drain(..) {
        for (jazz, transform, mut player) in &mut players {
            if Some(jazz) == by {
                continue;
            }
            // (Into what it overlaps: his body's cylinder.)
            let body = crate::riot::body_of(&player);
            if crate::riot::distance_to_body(at, transform.translation, body) > DEATH_BLAST_RADIUS {
                continue;
            }
            let to = player.centre(transform.translation) - at;
            let distance = to.length();
            let dir = to.normalize_or(Vec3::Y);
            if level
                .shot_surface(&colliders, at, dir, distance)
                .is_some_and(|(t, _)| t < distance - 0.25)
            {
                continue;
            }
            // (A blast: TnBuffExplodeOnDeathBase's HurtRadius, explosive.)
            let hit = crate::enemies::Hit::new(crate::enemies::Death::Explosive, at + to, dir)
                .of_type("TnDamageType");
            if crate::damage_player(&mut player, DEATH_BLAST_DAMAGE, hit) {
                // (The burnt enemy's own: its controller's, the kill feed's killer;
                // TnDamageTypeFire, TnDamageType's words.)
                if let Some(kind) = kind {
                    player.last_attack.0 = crate::killfeed::Who::Enemy(kind as u16);
                }
                sfx.play_from(crate::SND_PLAYER_HIT, None);
                player.hud_events.push(crate::hud::HudEvent::Damaged {
                    yaw: crate::heading(at - transform.translation),
                    amount: DEATH_BLAST_DAMAGE / crate::HIT_INDICATOR_HEALTH,
                });
            }
        }
    }
}

/// Plays a special shot's effects for one round (pellet) of it: on the first, the
/// muzzle flash at `origin` along `barrel` (kept on the muzzle of `gun`, the Jazz and
/// the weapon in his hand) and the shell from `port` thrown out along `side`; the
/// tracer along `dir` out to `distance`, and where it struck (`impact`: the place
/// and the surface's normal there), the impact and its sound.
#[allow(clippy::too_many_arguments)]
pub(crate) fn play_fx(
    commands: &mut Commands,
    (fx, sfx): (&mut FxSystems, &mut Sfx),
    kind: &SpecialFx,
    (origin, barrel): (Vec3, Vec3),
    (dir, distance): (Vec3, f32),
    impact: Option<(Vec3, Vec3)>,
    gun: Option<(Entity, usize)>,
    (first, port): (bool, Option<(Vec3, Vec3)>),
) {
    if first {
        crate::riot::flash_on_muzzle(commands, fx, kind.muzzle, origin, barrel, gun);
        if let (false, Some((at, side))) = (kind.shell.is_empty(), port) {
            fx.spawn(kind.shell, at, side, &[]);
        }
    }
    fx.spawn_tracer(kind.tracer, origin, dir, distance);
    if let Some((at, normal)) = impact {
        fx.spawn(kind.impact, at, normal, &[]);
        if !kind.sound_impact.is_empty() {
            sfx.play_at(kind.sound_impact, at);
        }
    }
}
