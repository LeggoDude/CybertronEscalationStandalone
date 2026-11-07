//! What a type's blueprint resolves its tasks' references to
//! (OnResolveAssetReference, OnResolveAnimationParameters): the weapon, grenade,
//! dialogue, reaction animation sets and hit reaction pickers its tasks name by
//! number, and the animations they play (by the stance the pawn is in).

use std::sync::Arc;

use bevy::math::Vec3;

use super::anim::AnimParams;
use super::data::Behaviour;
use super::hits::HitReactionPicker;
use super::pawn::stance;
use super::weapon::GunData;

/// A TnDirectionalAnimSet: an animation (and the one after it) for each direction
/// round the pawn.
#[derive(Clone, Debug, Default)]
pub struct DirectionalAnimSet {
    /// Each: its direction's centre and half angle (degrees, from the pawn's
    /// facing, clockwise), its animation and its out animation.
    pub animations: Vec<(f32, f32, AnimParams, AnimParams)>,
}

impl DirectionalAnimSet {
    fn from_props(b: &Behaviour, key: &str) -> Option<DirectionalAnimSet> {
        let n = b.n(&format!("{key}.Animations"));
        if n == 0 {
            return None;
        }
        let base = AnimParams::default();
        Some(DirectionalAnimSet {
            animations: (0..n)
                .map(|i| {
                    let k = format!("{key}.Animations[{i}]");
                    (
                        b.f(&format!("{k}.AngleCenter"), 0.0),
                        b.f(&format!("{k}.HalfAngle"), 0.0),
                        AnimParams::from_props(b, &format!("{k}.Anim"), &base),
                        AnimParams::from_props(b, &format!("{k}.OutAnim"), &base),
                    )
                })
                .collect(),
        })
    }

    /// FindAnimationByDirection: the animation whose arc holds the direction (a
    /// yaw relative to the pawn, rotation units).
    pub fn find(&self, local_yaw: f32) -> Option<(AnimParams, AnimParams)> {
        for (centre, half, anim, out) in &self.animations {
            if anim.is_none() {
                continue;
            }
            let rel = super::ue::wrap_rot(local_yaw - centre * super::ue::ROT_PER_DEG);
            let angle = rel * 0.005493;
            if angle < -half || angle > *half {
                continue;
            }
            return Some((anim.clone(), out.clone()));
        }
        None
    }
}

/// A TnDistanceBasedAnimSet: animations for the distances they cover.
#[derive(Clone, Debug, Default)]
pub struct DistanceAnimSet {
    /// Each: its distance range (min, max) and its animation.
    pub animations: Vec<(f32, f32, AnimParams)>,
}

impl DistanceAnimSet {
    fn from_props(b: &Behaviour, key: &str) -> Option<DistanceAnimSet> {
        let n = b.n(&format!("{key}.Animations"));
        if n == 0 {
            return None;
        }
        let base = AnimParams::default();
        Some(DistanceAnimSet {
            animations: (0..n)
                .map(|i| {
                    let k = format!("{key}.Animations[{i}]");
                    (
                        b.f(&format!("{k}.DistanceRange.Min"), 0.0),
                        b.f(&format!("{k}.DistanceRange.Max"), 0.0),
                        AnimParams::from_props(b, &format!("{k}.Anim"), &base),
                    )
                })
                .collect(),
        })
    }

    /// FindAnimationsThatWorkWithDistance: each animation (added to `out`) whose
    /// range starts within the distance.
    pub fn working_with(&self, distance: f32, out: &mut Vec<AnimParams>) {
        for (min, _, anim) in &self.animations {
            if anim.is_none() || distance < *min {
                continue;
            }
            out.push(anim.clone());
        }
    }
}

/// What an asset reference resolves to.
#[derive(Clone, Debug)]
pub enum Asset {
    None,
    /// A weapon's class.
    Weapon(String),
    /// A projectile's class (a grenade).
    Projectile(String),
    /// A sound event (dialogue).
    Dialog(String),
    AnimSet(Arc<DirectionalAnimSet>),
    DistanceAnimSet(Arc<DistanceAnimSet>),
    Picker(Arc<HitReactionPicker>),
    /// A tactical evaluator's class.
    Evaluator(&'static str),
    /// TnAreaOfEffectAttackBlueprint (the Guard's area attack).
    AreaOfEffect(Arc<AreaOfEffect>),
    /// The Leaper's operators' blueprints.
    LeapSmash(Arc<LeapSmash>),
    MeleeLunge(Arc<MeleeLunge>),
    Skid(Arc<Skid>),
    /// The jets' TnChooseJetRobotAttackPointOperatorBlueprint.
    RobotAttackPoint(super::jet::RobotAttackPoint),
}

/// TnLeapSmashOperatorBlueprint: the dive's AccelRate and AirSpeed, when the
/// transform plays (StartTransformAnimTime: seconds from the top of the leap), the
/// smash's damage, radius, height limit and momentum, the damage multiplier the
/// Leaper takes the while (open to blows as it leaps), and how much more
/// vulnerable a miss leaves it (LeapSmashMissedVulnerableAmount: WS_Vulnerability).
#[derive(Clone, Debug, Default)]
pub struct LeapSmash {
    pub start_transform_anim_time: f32,
    pub accel_rate: f32,
    pub air_speed: f32,
    pub smash_damage: f32,
    pub damage_radius: f32,
    pub damage_height_limit: f32,
    pub damage_momentum: f32,
    pub damage_multiplier: f32,
    pub missed_vulnerable_amount: i32,
}

/// TnMeleeLungeOperatorBlueprint: its flight's AirSpeed, AccelRate and time, and
/// its damage sweep's damage, socket, extent and momentum.
#[derive(Clone, Debug, Default)]
pub struct MeleeLunge {
    pub air_speed: f32,
    pub accel_rate: f32,
    pub duration: f32,
    pub damage: f32,
    pub socket: Arc<str>,
    pub extent: Vec3,
    pub damage_momentum: f32,
}

/// TnAnimatedSkidOperatorBlueprint: how long it slides to a stop, and its damage
/// sweep's time, damage, socket, extent and momentum.
#[derive(Clone, Debug, Default)]
pub struct Skid {
    pub skid_duration: f32,
    pub sweep_duration: f32,
    pub damage: f32,
    pub socket: Arc<str>,
    pub extent: Vec3,
    pub damage_momentum: f32,
}

impl LeapSmash {
    /// Its values, over its class's defaults.
    fn from_props(b: &Behaviour, key: &str) -> Option<LeapSmash> {
        b.get(&format!("{key}.class"))?;
        let f = |k: &str, d: f32| b.f(&format!("{key}.{k}"), d);
        Some(LeapSmash {
            start_transform_anim_time: f("StartTransformAnimTime", 0.5),
            accel_rate: f("AccelRate", 30000.0),
            air_speed: f("AirSpeed", 15000.0),
            smash_damage: f("SmashDamage", 200.0),
            damage_radius: f("DamageRadius", 2000.0),
            damage_height_limit: f("DamageHeightLimit", 100.0),
            damage_momentum: f("DamageMomentum", 400000.0),
            damage_multiplier: f("DamageMultiplier", 50.0),
            missed_vulnerable_amount: b.i(&format!("{key}.LeapSmashMissedVulnerableAmount"), 0),
        })
    }
}

impl MeleeLunge {
    fn from_props(b: &Behaviour, key: &str) -> Option<MeleeLunge> {
        b.get(&format!("{key}.class"))?;
        let f = |k: &str, d: f32| b.f(&format!("{key}.{k}"), d);
        Some(MeleeLunge {
            air_speed: f("AirSpeed", 7500.0),
            accel_rate: f("AccelRate", 50000.0),
            duration: f("Duration", 0.3),
            damage: f("Damage", 150.0),
            socket: Arc::from(
                b.get(&format!("{key}.DamageSweeperSocketName"))
                    .unwrap_or("PositionSocket"),
            ),
            extent: b.v3(&format!("{key}.DamageSweeperExtent"), Vec3::splat(350.0)),
            damage_momentum: f("DamageMomentum", 300000.0),
        })
    }
}

impl Skid {
    fn from_props(b: &Behaviour, key: &str) -> Option<Skid> {
        b.get(&format!("{key}.class"))?;
        let f = |k: &str, d: f32| b.f(&format!("{key}.{k}"), d);
        Some(Skid {
            skid_duration: f("SkidDuration", 0.2),
            sweep_duration: f("SweepDuration", 1.0),
            damage: f("Damage", 150.0),
            socket: Arc::from(
                b.get(&format!("{key}.DamageSweeperSocketName"))
                    .unwrap_or("PositionSocket"),
            ),
            extent: b.v3(&format!("{key}.DamageSweeperExtent"), Vec3::splat(350.0)),
            damage_momentum: f("DamageMomentum", 300000.0),
        })
    }
}

/// TnAreaOfEffectAttackBlueprint: the attack's animation (high priority), and
/// what its blow does to each pawn of the other side within its radius of the
/// attacker as the animation's notify comes: its damage, of its type, and a push
/// away from it along the ground and up (MomentumAway, MomentumUp).
#[derive(Clone, Debug, Default)]
pub struct AreaOfEffect {
    pub attack_animation: AnimParams,
    pub radius: f32,
    pub damage: f32,
    pub momentum_away: f32,
    pub momentum_up: f32,
    pub damage_type: String,
}

impl AreaOfEffect {
    fn from_props(b: &Behaviour, key: &str) -> Option<AreaOfEffect> {
        b.get(&format!("{key}.class"))?;
        let f = |k: &str| b.f(&format!("{key}.{k}"), 0.0);
        Some(AreaOfEffect {
            attack_animation: expand_high_priority(b, &format!("{key}.AttackAnimation")),
            radius: f("Radius"),
            damage: f("Damage"),
            momentum_away: f("MomentumAway"),
            momentum_up: f("MomentumUp"),
            damage_type: b
                .obj(&format!("{key}.DamageType"))
                .unwrap_or("None")
                .to_string(),
        })
    }
}

/// A type's resolved references, built once from its behaviour.
#[derive(Clone, Debug, Default)]
pub struct Blueprint {
    pub class: String,
    pub assets: Vec<(i32, Asset)>,
    /// Animation references: plain params, or a choice by stance (its stances'
    /// animations and its default).
    pub anims: Vec<(i32, AnimRef)>,
    /// Its guns (the inventory TnAiWeaponController.EquipWeapon finds or creates
    /// a weapon class in: GunData::all_from_behaviour's, the first the one it
    /// starts with; the soldiers' rifle for a type with none).
    pub guns: Vec<Arc<GunData>>,
}

#[derive(Clone, Debug)]
pub enum AnimRef {
    Params(AnimParams),
    ByStance(Vec<(u8, AnimParams)>, AnimParams),
}

impl Blueprint {
    pub fn new(b: &Behaviour) -> Blueprint {
        let mut guns: Vec<Arc<GunData>> = GunData::all_from_behaviour(b)
            .into_iter()
            .map(Arc::new)
            .collect();
        if guns.is_empty() {
            guns.push(Arc::new(GunData::default()));
        }
        let mut bp = Blueprint {
            class: b.class.clone(),
            guns,
            ..Default::default()
        };
        match b.class.as_str() {
            "TnSoldierCharacterBlueprint" => bp.soldier(b),
            "TnShotgunnerCharacterBlueprint" => bp.shotgunner(b),
            "TnRocketTrooperCharacterBlueprint" => bp.rocket_trooper(b),
            "TnSharpShooterCharacterBlueprint" => bp.sharp_shooter(b),
            "TnScrapperCharacterBlueprint" => bp.scrapper(b),
            "TnSwarmerCharacterBlueprint" => bp.swarmer(b),
            "TnHeavyGunnerCharacterBlueprint" => bp.heavy_gunner(b),
            "TnTitanCharacterBlueprint" => bp.titan(b),
            "TnGuardCharacterBlueprint" => bp.guard(b),
            "TnLeaperCharacterBlueprint" => bp.leaper(b),
            "TnExplosiveDroneCharacter" => bp.explosive_drone(b),
            "TnSniperCharacterBlueprint" => bp.sniper(b),
            "TnJetCharacterBlueprint" => bp.jet(b),
            _ => {}
        }
        bp
    }

    /// TnJetCharacterBlueprint's references (OnResolveAssetReference: its rockets
    /// and its dogfight's gun, its dialog, its robot attack point's blueprint
    /// (SetUpRobotAttackBlueprint: its RobotTransform values); its
    /// OnResolveAnimationParameters: its robot's attack, RobotAttack).
    fn jet(&mut self, b: &Behaviour) {
        let obj = |k: &str| b.obj(k).map(|s| s.to_string());
        self.assets.push((
            0,
            obj("AirToGroundWeapon").map_or(Asset::None, Asset::Weapon),
        ));
        self.assets
            .push((1, obj("DogfightWeapon").map_or(Asset::None, Asset::Weapon)));
        for (r, k) in [
            (2, "SawEnemy"),
            (3, "TransformToRobot"),
            (4, "MoveToTransformToRobot"),
            (5, "TransformToVehicle"),
            (6, "TauntGroundEnemies"),
            (7, "TauntAirEnemies"),
        ] {
            self.dialog(b, r, k);
        }
        self.assets.push((
            8,
            Asset::RobotAttackPoint(super::jet::RobotAttackPoint::from_behaviour(b)),
        ));
        // (Animation.ExpandAnimationParameters: its compact params' name, rate and
        // blends over the defaults.)
        self.anims.push((
            0,
            AnimRef::Params(AnimParams {
                name: b
                    .get("RobotAttack.AnimName")
                    .map_or(Arc::from("None"), Arc::from),
                play_rate: b.f("RobotAttack.PlayRate", 1.0),
                blend_in: b.f("RobotAttack.BlendIn", 0.1),
                blend_out: b.f("RobotAttack.BlendOut", 0.1),
                ..Default::default()
            }),
        ));
    }

    /// TnSniperCharacterBlueprint's references (OnResolveAssetReference: its gun,
    /// its boosters (its flights read them: sniper.rs) and its dialog).
    fn sniper(&mut self, b: &Behaviour) {
        let obj = |k: &str| b.obj(k).map(|s| s.to_string());
        self.assets
            .push((0, obj("WeaponClass").map_or(Asset::None, Asset::Weapon)));
        for (r, k) in [
            (2, "SearchingForEnemyDialog"),
            (3, "AwareOfEnemyDialog"),
            (4, "LostEnemyDialog"),
            (5, "BattleChatterDialog"),
        ] {
            self.assets
                .push((r, obj(k).map_or(Asset::None, Asset::Dialog)));
        }
    }

    /// A reference to a sound event (a dialog), by the blueprint's property.
    fn dialog(&mut self, b: &Behaviour, r: i32, key: &str) {
        let a = b
            .obj(key)
            .map_or(Asset::None, |s| Asset::Dialog(s.to_string()));
        self.assets.push((r, a));
    }

    /// A reference to its weapon.
    fn weapon(&mut self, b: &Behaviour, r: i32) {
        let a = b
            .obj("PrimaryWeapon")
            .map_or(Asset::None, |s| Asset::Weapon(s.to_string()));
        self.assets.push((r, a));
    }

    /// A reference to a directional animation set.
    fn anim_set(&mut self, b: &Behaviour, r: i32, key: &str) {
        let a = DirectionalAnimSet::from_props(b, key)
            .map_or(Asset::None, |s| Asset::AnimSet(Arc::new(s)));
        self.assets.push((r, a));
    }

    /// A reference to a hit reaction picker.
    fn picker(&mut self, b: &Behaviour, r: i32, key: &str) {
        let a = HitReactionPicker::from_props(b, key)
            .map_or(Asset::None, |p| Asset::Picker(Arc::new(p)));
        self.assets.push((r, a));
    }

    /// The knockback's animations (references 0 to 2: its take off, its time in
    /// the air and its landing), as most of the types number them.
    fn knockback_anims(&mut self, b: &Behaviour, first: i32) {
        for (i, k) in [
            "KnockedbackTakeOffParams",
            "KnockedbackInAirParams",
            "KnockedbackLandParams",
        ]
        .iter()
        .enumerate()
        {
            self.anims.push((
                first + i as i32,
                AnimRef::Params(expand_high_priority(b, k)),
            ));
        }
    }

    /// The blind fire from cover by the stance (none in any other stance).
    fn blind_fire_anims(&mut self, b: &Behaviour, r: i32) {
        let base = AnimParams::default();
        let blind = vec![
            (
                stance::LOW_COVER_HIDE,
                AnimParams::from_props(b, "LowCoverBlindFire", &base),
            ),
            (
                stance::LOW_CORNER_COVER_HIDE,
                AnimParams::from_props(b, "LowCornerCoverBlindFire", &base),
            ),
            (
                stance::CORNER_COVER_HIDE,
                AnimParams::from_props(b, "CornerCoverBlindFire", &base),
            ),
        ];
        self.anims
            .push((r, AnimRef::ByStance(blind, AnimParams::default())));
    }

    /// The cover and navigation hit reaction pickers, from reference `first` on.
    fn cover_pickers(&mut self, b: &Behaviour, first: i32) {
        for (i, k) in [
            "PeekCornerCoverHitReaction",
            "PeekMidCoverHitReaction",
            "PeekMidCornerCoverHitReaction",
            "PeekOpenCoverHitReaction",
            "PeekLedgeCoverHitReaction",
            "InCornerCoverHitReaction",
            "InMidCoverHitReaction",
            "InOpenCoverHitReaction",
            "NavigatingHitReaction",
        ]
        .iter()
        .enumerate()
        {
            self.picker(b, first + i as i32, k);
        }
    }

    /// TnShotgunnerCharacterBlueprint's references.
    fn shotgunner(&mut self, b: &Behaviour) {
        self.weapon(b, 0);
        for (r, k) in [(1, "RightStepAnimSet"), (2, "LeftStepAnimSet")] {
            let a = DistanceAnimSet::from_props(b, k)
                .map_or(Asset::None, |s| Asset::DistanceAnimSet(Arc::new(s)));
            self.assets.push((r, a));
        }
        self.picker(b, 3, "NavigatingHitReaction");
        self.dialog(b, 4, "AwareOfEnemyDialog");
        self.knockback_anims(b, 0);
        let base = AnimParams::default();
        self.anims.push((
            3,
            AnimRef::Params(AnimParams::from_props(b, "DashBackAnim", &base)),
        ));
        self.anims.push((
            4,
            AnimRef::Params(AnimParams::from_props(b, "LookForEnemies", &base)),
        ));
    }

    /// TnRocketTrooperCharacterBlueprint's references.
    fn rocket_trooper(&mut self, b: &Behaviour) {
        self.weapon(b, 0);
        self.weapon(b, 8);
        for (r, k) in [
            (1, "AwareOfEnemyDialog"),
            (2, "MovingToCoverDialog"),
            (3, "EnemyCloakedDialog"),
            (4, "EnemyUncloakedDialog"),
            (5, "FiringFromCoverDialog"),
        ] {
            self.dialog(b, r, k);
        }
        self.anim_set(b, 6, "GrenadeReactions");
        self.anim_set(b, 7, "BumpReactions");
        self.cover_pickers(b, 9);
        self.knockback_anims(b, 0);
        self.blind_fire_anims(b, 3);
    }

    /// TnSharpShooterCharacterBlueprint's references.
    fn sharp_shooter(&mut self, b: &Behaviour) {
        self.weapon(b, 0);
        self.anim_set(b, 1, "GrenadeReactions");
        self.anim_set(b, 2, "BumpReactions");
        for (r, k) in [
            (3, "EnemyCloakedDialog"),
            (4, "EnemyUncloakedDialog"),
            (14, "AwareOfEnemyDialog"),
            (15, "SearchingForEnemyDialog"),
            (16, "LostEnemyDialog"),
            (17, "EnemyReaquiredDialog"),
            (18, "OnFireDialog"),
            (19, "EnemyHealthLowDialog"),
        ] {
            self.dialog(b, r, k);
        }
        self.cover_pickers(b, 5);
        self.knockback_anims(b, 0);
        self.blind_fire_anims(b, 3);
    }

    /// TnScrapperCharacterBlueprint's references: its navigating hit reactions,
    /// and its reactions' and knockback's animations (ExpandAnimationParameters:
    /// their own priority).
    fn scrapper(&mut self, b: &Behaviour) {
        self.picker(b, 0, "NavigatingHitReaction");
        self.melee_reactions(b);
    }

    /// TnSwarmerCharacterBlueprint's references: its reactions' and knockback's
    /// animations, as the Scrapper's (no assets).
    fn swarmer(&mut self, b: &Behaviour) {
        self.melee_reactions(b);
    }

    /// The melee kinds' reactions and knockback (their animation references 0 to
    /// 5: ExpandAnimationParameters, their own priority).
    fn melee_reactions(&mut self, b: &Behaviour) {
        for (r, k) in [
            (0, "HitReactionParams"),
            (1, "StunnedReactionParams"),
            (2, "MeleeBlockedParams"),
            (3, "KnockedbackTakeOffParams"),
            (4, "KnockedbackInAirParams"),
            (5, "KnockedbackLandParams"),
        ] {
            let mut p = expand_high_priority(b, k);
            p.priority = 0;
            p.override_mode = AnimParams::default().override_mode;
            self.anims.push((r, AnimRef::Params(p)));
        }
    }

    /// TnHeavyGunnerCharacterBlueprint's references (no animations of its own).
    fn heavy_gunner(&mut self, b: &Behaviour) {
        for (r, k) in [
            (0, "AwareOfEnemyDialog"),
            (1, "LostEnemyDialog"),
            (2, "EnemyHasLowHealthDialog"),
        ] {
            self.dialog(b, r, k);
        }
        self.anim_set(b, 3, "GravityBombReactions");
    }

    /// TnTitanCharacterBlueprint's references: its drones' spawning animation
    /// (ExpandAnimationParameters).
    fn titan(&mut self, b: &Behaviour) {
        for (r, k) in [
            (0, "AwareOfEnemyDialog"),
            (1, "LostEnemyDialog"),
            (2, "SpawningDronesDialog"),
            (3, "EnemyHasLowHealthDialog"),
        ] {
            self.dialog(b, r, k);
        }
        self.anim_set(b, 4, "GravityBombReactions");
        let mut p = expand_high_priority(b, "SpawnDrones");
        p.priority = 0;
        p.override_mode = AnimParams::default().override_mode;
        self.anims.push((0, AnimRef::Params(p)));
    }

    /// TnExplosiveDroneCharacter.OnResolveAnimationParameters: its explosion's
    /// (ExpandHighPriorityAnimationParameters).
    fn explosive_drone(&mut self, b: &Behaviour) {
        let p = expand_high_priority(b, "ExplosionAnimation");
        self.anims.push((0, AnimRef::Params(p)));
    }

    /// TnGuardCharacterBlueprint's references (OnResolveAssetReference: its
    /// dialog events, its area attack; OnResolveAnimationParameters,
    /// ExpandAnimationParameters: its changes of form and its special attack).
    fn guard(&mut self, b: &Behaviour) {
        for (r, d) in [
            (0, "Guard_EnemySpottedAlert"),
            (1, "Guard_AlertAcknowledged"),
            (2, "Guard_HeardNoiseInvestigate"),
            (3, "Guard_SearchFailed"),
            (4, "Guard_BattleModeAlert"),
            (5, "Guard_BattleModeChatter"),
            (6, "Guard_BattleModeEnemyLost"),
            (7, "Guard_BattleModeNoiseHeard"),
        ] {
            self.assets.push((r, Asset::Dialog(d.to_string())));
        }
        let aoe = AreaOfEffect::from_props(b, "AreaOfEffectAttack")
            .map_or(Asset::None, |a| Asset::AreaOfEffect(Arc::new(a)));
        self.assets.push((8, aoe));
        for (r, k) in [
            (0, "InvestigateTransformToBattleAnimation"),
            (1, "BattleTransformToInvestigateAnimation"),
            (2, "SpecialAttack"),
        ] {
            let mut p = expand_high_priority(b, k);
            p.priority = 0;
            p.override_mode = AnimParams::default().override_mode;
            self.anims.push((r, AnimRef::Params(p)));
        }
    }

    /// TnLeaperCharacterBlueprint's references (OnResolveAssetReference: its leap
    /// smash's, its lunge's and its skids' (a hit's and a miss's) blueprints, its
    /// gravity bomb reactions; OnResolveAnimationParameters: its lunge's, leap
    /// smash's and knockback's animations (ExpandAnimationParameters), its booster
    /// attack's as it is).
    fn leaper(&mut self, b: &Behaviour) {
        let a = LeapSmash::from_props(b, "LeapSmashBlueprint")
            .map_or(Asset::None, |x| Asset::LeapSmash(Arc::new(x)));
        self.assets.push((0, a));
        let a = MeleeLunge::from_props(b, "MeleeLungeBlueprint")
            .map_or(Asset::None, |x| Asset::MeleeLunge(Arc::new(x)));
        self.assets.push((1, a));
        for (r, k) in [
            (2, "MeleeLungeSucceedSkidBlueprint"),
            (3, "MeleeLungeFailSkidBlueprint"),
        ] {
            let a = Skid::from_props(b, k).map_or(Asset::None, |x| Asset::Skid(Arc::new(x)));
            self.assets.push((r, a));
        }
        self.anim_set(b, 4, "GravityBombReactions");
        for (r, k) in [
            (0, "MeleeLungeTellParams"),
            (1, "MeleeLungeParams"),
            (2, "MeleeLungeFailRecoveryParams"),
            (3, "MeleeLungeSucceedRecoveryParams"),
            (4, "LeapSmashTellParams"),
            (5, "LeapSmashTransformParams"),
            (6, "LeapSmashRecoveryParams"),
            (7, "KnockedbackTakeOffParams"),
            (8, "KnockedbackInAirParams"),
            (9, "KnockedbackLandParams"),
        ] {
            let mut p = expand_high_priority(b, k);
            p.priority = 0;
            p.override_mode = AnimParams::default().override_mode;
            self.anims.push((r, AnimRef::Params(p)));
        }
        let base = AnimParams::default();
        self.anims.push((
            10,
            AnimRef::Params(AnimParams::from_props(b, "BoosterAttackParams", &base)),
        ));
    }

    /// TnSoldierCharacterBlueprint's references.
    fn soldier(&mut self, b: &Behaviour) {
        let obj = |k: &str| b.obj(k).map(|s| s.to_string());
        let mut add = |r: i32, a: Asset| self.assets.push((r, a));
        add(0, obj("PrimaryWeapon").map_or(Asset::None, Asset::Weapon));
        add(1, obj("Grenades").map_or(Asset::None, Asset::Projectile));
        add(2, Asset::Dialog("FiredGrenadeDialog".into()));
        let dialogs = [
            (5, "EnemyCloakedDialog"),
            (6, "EnemyUncloakedDialog"),
            (16, "AwareOfEnemyDialog"),
            (17, "SearchingForEnemyDialog"),
            (18, "LostEnemyDialog"),
            (19, "EnemyReaquiredDialog"),
            (20, "OnFireDialog"),
            (21, "EnemyHealthLowDialog"),
        ];
        for (r, k) in dialogs {
            add(r, obj(k).map_or(Asset::None, Asset::Dialog));
        }
        for (r, k) in [
            (3, "GrenadeReactions"),
            (4, "BumpReactions"),
            (22, "BumpReactionsAware"),
        ] {
            add(
                r,
                DirectionalAnimSet::from_props(b, k)
                    .map_or(Asset::None, |s| Asset::AnimSet(Arc::new(s))),
            );
        }
        let pickers = [
            (7, "PeekCornerCoverHitReaction"),
            (8, "PeekMidCoverHitReaction"),
            (9, "PeekMidCornerCoverHitReaction"),
            (10, "PeekOpenCoverHitReaction"),
            (11, "PeekLedgeCoverHitReaction"),
            (12, "InCornerCoverHitReaction"),
            (13, "InMidCoverHitReaction"),
            (14, "InOpenCoverHitReaction"),
            (15, "NavigatingHitReaction"),
        ];
        for (r, k) in pickers {
            add(
                r,
                HitReactionPicker::from_props(b, k)
                    .map_or(Asset::None, |p| Asset::Picker(Arc::new(p))),
            );
        }
        add(23, Asset::Evaluator("TnCloseOnEnemyTacticalEvaluator"));
        // The animations.
        let base = AnimParams::default();
        let grenade_default =
            AnimParams::from_props(b, "GrenadeThrowParams.DefaultAnimParams", &base);
        let grenade_stances = (0..b.n("GrenadeThrowParams.StanceAnimations"))
            .map(|i| {
                let k = format!("GrenadeThrowParams.StanceAnimations[{i}]");
                (
                    stance::number(b.get(&format!("{k}.Stance")).unwrap_or("STANCE_None")),
                    AnimParams::from_props(b, &format!("{k}.AnimParams"), &base),
                )
            })
            .collect();
        self.anims
            .push((0, AnimRef::ByStance(grenade_stances, grenade_default)));
        for (r, k) in [
            (1, "KnockedbackTakeOffParams"),
            (2, "KnockedbackInAirParams"),
            (3, "KnockedbackLandParams"),
        ] {
            self.anims
                .push((r, AnimRef::Params(expand_high_priority(b, k))));
        }
        let blind = vec![
            (
                stance::LOW_COVER_HIDE,
                AnimParams::from_props(b, "LowCoverBlindFire", &base),
            ),
            (
                stance::LOW_CORNER_COVER_HIDE,
                AnimParams::from_props(b, "LowCornerCoverBlindFire", &base),
            ),
            (
                stance::CORNER_COVER_HIDE,
                AnimParams::from_props(b, "CornerCoverBlindFire", &base),
            ),
        ];
        // (Any other stance: falls through to the reload's case, as the script's
        // switch does.)
        self.anims.push((
            4,
            AnimRef::ByStance(blind, AnimParams::from_props(b, "ReloadParams", &base)),
        ));
        self.anims.push((
            5,
            AnimRef::Params(AnimParams::from_props(b, "ReloadParams", &base)),
        ));
        self.anims.push((
            6,
            AnimRef::Params(AnimParams::from_props(b, "NavigatingReloadParams", &base)),
        ));
    }

    /// ResolveAssetReference.
    pub fn asset(&self, r: i32) -> Asset {
        self.assets
            .iter()
            .find(|(k, _)| *k == r)
            .map_or(Asset::None, |(_, a)| a.clone())
    }

    /// The gun of a weapon class (an Asset::Weapon's: by its whole name or its
    /// own), if it has one.
    pub fn gun(&self, weapon: &str) -> Option<Arc<GunData>> {
        let class = weapon.rsplit('.').next().unwrap_or(weapon);
        self.guns.iter().find(|g| g.class == class).cloned()
    }

    /// ResolveAnimationParameters (empty params for none).
    pub fn anim(&self, r: i32, stance: u8) -> AnimParams {
        match self.anims.iter().find(|(k, _)| *k == r) {
            None => AnimParams::default(),
            Some((_, AnimRef::Params(p))) => p.clone(),
            Some((_, AnimRef::ByStance(list, default))) => list
                .iter()
                .find(|(s, _)| *s == stance)
                .map_or_else(|| default.clone(), |(_, p)| p.clone()),
        }
    }
}

/// Animation.ExpandHighPriorityAnimationParameters: a TnHighPriorityAnimParams
/// (name, rate, blends) as full params, high priority, overriding its equal.
fn expand_high_priority(b: &Behaviour, key: &str) -> AnimParams {
    AnimParams {
        name: b
            .get(&format!("{key}.AnimName"))
            .map_or(Arc::from("None"), Arc::from),
        play_rate: b.f(&format!("{key}.PlayRate"), 1.0),
        blend_in: b.f(&format!("{key}.BlendIn"), 0.1),
        blend_out: b.f(&format!("{key}.BlendOut"), 0.1),
        priority: 1,
        override_mode: super::anim::OVERRIDE_IF_HIGHER_OR_EQUAL,
        ..Default::default()
    }
}
