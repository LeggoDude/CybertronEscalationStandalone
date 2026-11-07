//! The operators that run a plan's primitive tasks (HtnOperator and its
//! subclasses the soldiers' domain uses): each starts (OnStart, then its
//! behaviour flags: set outright by a TnBehaviorOperator, added by any other),
//! updates until it succeeds or fails (OnUpdate), hears events (OnHandleEvent)
//! and finishes (OnFinish, and OnPlanFinish as its plan ends).

use bevy::prelude::*;

use super::anim::{AnimParams, Callback};
use super::blueprint::Asset;
use super::brain::{
    BEHAVIOR_AIMER, BEHAVIOR_ALLOW_MOTION, BEHAVIOR_HEAD_TRACKING, BEHAVIOR_LEDGE_FALL,
    BEHAVIOR_ROTATE_TO_FOCUS, BEHAVIOR_SHOOTER, BEHAVIOR_WEAPON_TRACKING, Brain, Ctx, Event,
    EventId,
};
use super::htn::{self, Status, Task};
use super::motion;
use super::pawn::{Physics, stance};
use super::sensors::with_sensor;
use super::shared::{self, ClaimInfo, MemberInfo};
use super::tactical::TacticalNode;
use super::ue;
use crate::nav::Cover;

/// Callback kinds an operator's animations call back with.
const CB_COMPLETE: u8 = 1;
const CB_FAILED: u8 = 2;
const CB_OPENING_COMPLETE: u8 = 3;
const CB_TAKE_OFF_COMPLETE: u8 = 4;
const CB_LAND_COMPLETE: u8 = 5;
const CB_SCAN_FINISHED: u8 = 6;
const CB_SCAN_FAILED: u8 = 7;
const CB_THROW_COMPLETE: u8 = 8;

/// An operator and its state, by its class.
#[derive(Clone, Debug)]
pub enum Kind {
    NoOp,
    Idle,
    ClearArea,
    ClearOverrideGoal,
    ChooseNewSearchPoint,
    ChooseNewPatrolPoint,
    ExitCover,
    MakeIncapableOfSquadAction,
    ReleaseSquadAction,
    ClaimSquadAction {
        action: usize,
        release_on_plan_finish: bool,
    },
    PlayDialog,
    ClearStance,
    SetStance,
    IdleForDuration {
        time_remaining: f32,
    },
    NavigatingIdle,
    UpdateNavigationParams,
    RobotMoveTo,
    UseInteract,
    EnableTacticalEvaluator,
    LookAt {
        time_remaining: f32,
        location: usize,
    },
    TurnToFocus {
        use_look_direction: bool,
        min_cos: f32,
        focus: usize,
    },
    LookForEnemies {
        location_ref: usize,
        location: Vec3,
        should_shoot: bool,
        half_cone: f32,
        duration: f32,
        direction: Vec3,
        time_remaining: f32,
    },
    ScanPatrolPoint {
        time_left: f32,
        index: usize,
        animations: Vec<String>,
        active: String,
    },
    ShootAtEnemy {
        time_remaining: f32,
        lead_target: bool,
        old_check_for_aiming: bool,
    },
    ShootAtLocation {
        time_remaining: f32,
        location: usize,
        stop_shooting_when_finished: bool,
    },
    ShootNearLocation {
        location: usize,
        direction: (f32, f32),
        duration: f32,
        time_remaining: f32,
        pitch_half: f32,
        yaw_half: f32,
    },
    PlayAnimation {
        params: AnimParams,
        additive: bool,
        complete: bool,
        focus: usize,
        interruptible: bool,
    },
    PlayHitReaction {
        complete: bool,
    },
    HitReaction {
        complete: bool,
        started: bool,
    },
    Knockedback {
        direction: Vec3,
        landing: bool,
        complete: bool,
        playing: AnimParams,
        land: AnimParams,
        in_air: AnimParams,
        take_off: AnimParams,
    },
    PlayPositionRelativeAnimation {
        found: AnimParams,
        found_out: AnimParams,
        complete: bool,
        opening_complete: bool,
        playing_out: bool,
        interruptible: bool,
        end_event: i32,
    },
    MoveInRandomDirection {
        current_time: f32,
        destination: Vec3,
    },
    EnterCover {
        time_left: f32,
        tactical: Option<TacticalNode>,
    },
    PeekFromCover {
        current_time: f32,
        tactical: Option<TacticalNode>,
        focus: usize,
    },
    MeleeEnemy {
        interruptible: bool,
        succeed_when_complete: bool,
        melee_type: u8,
    },
    ThrowGrenade {
        /// EGrenadeStatus: 0 none, 1 thrown, 2 failed, 3 done.
        status: u8,
        grenade: String,
    },
    StrafeAndShoot {
        side_angle: f32,
        max_range: f32,
        min_range: f32,
        enemy_start: Vec3,
        pawn_start: Vec3,
        relative_ref: usize,
        current_time: f32,
        destination: Vec3,
    },
    /// The Shotgunner's dash back: an animation that fires its gun (its rounds'
    /// notifies), facing a location.
    AnimatedShootingAttack {
        params: AnimParams,
        complete: bool,
        focus: usize,
    },
    /// A step round its enemy, to its left or right, as far as it can go.
    SideStep {
        params: AnimParams,
        complete: bool,
    },
    MoveAwayFromEnemy {
        current_time: f32,
        destination: Vec3,
        enemy: Option<Entity>,
    },
    /// Its gun taken in hand (the Rocket Trooper's launcher).
    EquipWeapon {
        finish_immediately: bool,
        aim_location: usize,
    },
    UnequipWeapon {
        finish_immediately: bool,
    },
    /// An ability triggered (the Sharpshooter's cloak).
    TriggerAbility {
        complete_on_activate: bool,
    },
    /// The Sharpshooter's scan: its laser sight on its enemy, closing in as it
    /// holds still, till it has locked on and held it.
    ScanSniperEnemy {
        last_enemy: Option<Entity>,
        scan_duration: f32,
        scan_time: f32,
        hold_lock_on_duration: f32,
        lock_on_duration: f32,
        lock_on_time: f32,
    },
    /// HtnOperator itself: OnStart returns nothing (false), so it fails to start.
    Base,
    /// TnMoveToOperator: a move it runs itself (moveto.rs).
    MoveTo(Box<super::moveto::MoveTo>),
    /// A side step clear of whatever stands between it and its enemy.
    MoveToClearAttackChannel {
        current_time: f32,
        destination: Vec3,
    },
    /// Waiting its turn at its enemy (WS_SquadCommand: SC_Attack).
    IdleInAttackChannel,
    /// The melee kinds' attack at their turn (its squad told of it as it ends).
    SquadMeleeEnemy {
        melee_type: u8,
        satisfied_squad_command: u8,
    },
    /// A reaction's animation, played again while the world state it clears
    /// comes back.
    React {
        params: AnimParams,
        world_state: usize,
        interruptible: bool,
        complete: bool,
    },
    /// A reaction's animation looped for a while.
    ReactForDuration {
        current_time: f32,
        duration: f32,
    },
    /// TnBehaviorOperator itself: its behaviour (none of its own), done at once.
    Behavior,
    /// TnPlayReactionOperator: a second of nothing (its stimulus unused).
    PlayReaction {
        time_remaining: f32,
    },
    /// TnPlayInjuredReactionOperator: the injured sensor's stumble.
    PlayInjuredReaction {
        params: AnimParams,
        playing: bool,
    },
    /// TnReloadWeaponOperator: its gun reloaded.
    ReloadWeapon {
        interruptible: bool,
    },
    /// TnShootAtTrackedLocationOperator: its aim dragged towards a place (its
    /// enemy, while it sees him) at its tracking speed, shooting while on it.
    ShootAtTrackedLocation {
        location_ref: usize,
        duration: f32,
        time_remaining: f32,
        tracking_speed: f32,
        ignore_aiming: bool,
    },
    /// TnStopMoveOperator: its motion stopped (and none allowed), done at once.
    StopMove,
    /// TnChangeRobotFormOperator: the Guard's change of form (guard.rs), its
    /// animation played facing a place; the form taken as it ends.
    ChangeRobotForm {
        target_form: i32,
        look_at: usize,
        succeeded: bool,
    },
    /// TnChooseNearbyLocationOperator: a place near it to look at next (the
    /// suspected enemy's), done at once.
    ChooseNearbyLocation,
    /// TnAreaOfEffectAttackOperator: the Guard's blast, its animation's notify
    /// knocking back and hurting the other side round it.
    AreaOfEffectAttack {
        attack: Option<std::sync::Arc<super::blueprint::AreaOfEffect>>,
        complete: bool,
    },
    /// TnGuardScanPatrolPointOperator: its look swept across its patrol point's
    /// facing (or its custom animations played) while it observes it.
    GuardScanPatrolPoint {
        observe: f32,
        yaw: f32,
        half_cone: f32,
        scan_duration: f32,
        time_elapsed: f32,
        index: usize,
        animations: Vec<String>,
    },
    /// TnPlayTellFacingFocusOperator: a tell's animation (high priority) played
    /// turning to face its enemy, done as it ends (its motion stopped then).
    PlayTellFacingFocus {
        complete: bool,
    },
    /// TnStopRotatingOperator: its focus straight ahead (turning to it), done at
    /// once.
    StopRotating,
    /// TnClaimLeapSmashOperator: its squad's leap smash claimed, done at once.
    ClaimLeapSmash,
    /// The Leaper's lunge, skid and leap smash (leaper.rs).
    MeleeLunge(Box<super::leaper::MeleeLunge>),
    AnimatedSkid(Box<super::leaper::Skid>),
    LeapSmash(Box<super::leaper::LeapSmash>),
    /// The Titan's drones going out, and the drones' own (drones.rs):
    /// TnDroneMoveToOperator, TnHoverAtLocationOperator, TnExplodeOperator and
    /// TnChooseNewDefendActorLocationOperator.
    SpawnDrones(Box<super::drones::SpawnDrones>),
    /// The Snipers' (sniper.rs): their points chosen, their flights to them, their
    /// insects on the walls and their leaving them.
    ChooseNewSniperPoint,
    ChooseNewSniperPointTowardsGoal,
    ChooseNewSniperSearchPoint,
    ChooseNewSniperAttackPoint,
    FlyToSniperPoint(Box<super::sniper::FlyTo>),
    AttachToWall(Box<super::sniper::Attach>),
    DetachFromWall(Box<super::sniper::Detach>),
    CrawlOnSniperPoint(Box<super::sniper::Crawl>),
    DroneMoveTo,
    HoverAtLocation(super::drones::HoverAtLocation),
    Explode,
    ChooseNewDefendActorLocation,
    /// The jets' (jet.rs): their flights (TnPlaneHoverToOperator), their points
    /// chosen, their hovers to face, their dashes aside, their changes of form and
    /// jumps, their robots' attack (TnAnimatedAttackOperator), their dogfights.
    PlaneHoverTo,
    ChooseNewAirAttackPoint,
    ChooseNewAerialPursuitPoint,
    ChooseNewFlyingCirclingPoint,
    ChooseJetRobotAttackPoint,
    ChooseNewFlyingInvestigateLocation,
    HoverToFace(Box<super::jet::HoverToFace>),
    PlaneDash(Box<super::jet::PlaneDash>),
    Transform(super::jet::Transform),
    Jump(super::jet::Jump),
    AnimatedAttack {
        params: AnimParams,
        additive: bool,
        complete: bool,
    },
    ShootAtEnemyHeadOn(Box<super::jet::Dogfight>),
    ShootAtEnemyFireForEffect(Box<super::jet::Dogfight>),
    /// A class this port doesn't run: it fails to start.
    Unknown(String),
}

/// An operator running a task.
#[derive(Clone, Debug)]
pub struct Op {
    pub kind: Kind,
    pub task: Task,
    /// Its serial number (its animations' callbacks name it).
    pub serial: u32,
    /// get_Interruptible.
    pub interruptible: bool,
    pub fail_context: &'static str,
}

impl Op {
    /// The operator of its class for a task.
    pub fn new(class: &str, task: Task, serial: u32) -> Op {
        // HtnOperator._Interruptible: true, but for the classes whose defaults
        // make them run to the end.
        let mut interruptible = !matches!(
            class,
            "TnHitReactionOperator"
                | "TnPlayHitReactionOperator"
                | "TnPeekFromCoverOperator"
                | "TnKnockedbackOperator"
                | "TnExitCoverOperator"
                | "TnEnterCoverOperator"
                | "TnPlayTellFacingFocusOperator"
                | "TnLeapSmashOperator"
                | "TnSpawnDronesOperator"
                | "TnFlyToSniperPointOperator"
                | "TnAttachToWallOperator"
                | "TnDetachFromWallOperator"
                | "TnTransformOperator"
                | "TnJumpOperator"
        );
        let kind = match class {
            "HtnNoOpOperator" => Kind::NoOp,
            "TnIdleOperator" => Kind::Idle,
            "TnClearAreaOperator" => Kind::ClearArea,
            "TnClearOverrideGoalOperator" => Kind::ClearOverrideGoal,
            "TnChooseNewSearchPointOperator" => Kind::ChooseNewSearchPoint,
            "TnChooseNewPatrolPointOperator" => Kind::ChooseNewPatrolPoint,
            "TnExitCoverOperator" => Kind::ExitCover,
            "TnMakeIncapableOfSquadActionOperator" => Kind::MakeIncapableOfSquadAction,
            "TnReleaseSquadActionOperator" => Kind::ReleaseSquadAction,
            "TnClaimSquadActionOperator" => Kind::ClaimSquadAction {
                action: 0,
                release_on_plan_finish: false,
            },
            "TnPlayDialogOperator" => Kind::PlayDialog,
            "TnClearStanceOperator" => Kind::ClearStance,
            "TnSetStanceOperator" => Kind::SetStance,
            "TnIdleForDurationOperator" => Kind::IdleForDuration {
                time_remaining: 0.0,
            },
            "TnNavigatingIdleOperator" => Kind::NavigatingIdle,
            "TnUpdateNavigationParamsOperator" => Kind::UpdateNavigationParams,
            "TnRobotMoveToOperator" => Kind::RobotMoveTo,
            "TnUseInteractOperator" => Kind::UseInteract,
            "TnEnableTacticalEvaluatorOperator" => Kind::EnableTacticalEvaluator,
            "TnLookAtOperator" => Kind::LookAt {
                time_remaining: 0.0,
                location: 0,
            },
            "TnTurnToFocusOperator" => Kind::TurnToFocus {
                use_look_direction: false,
                min_cos: 0.984,
                focus: 0,
            },
            "TnLookForEnemiesOperator" => Kind::LookForEnemies {
                location_ref: 0,
                location: Vec3::ZERO,
                should_shoot: false,
                half_cone: 0.0,
                duration: 0.0,
                direction: Vec3::ZERO,
                time_remaining: 0.0,
            },
            "TnScanPatrolPointOperator" => Kind::ScanPatrolPoint {
                time_left: 0.0,
                index: 0,
                animations: Vec::new(),
                active: String::new(),
            },
            "TnShootAtEnemyOperator" => Kind::ShootAtEnemy {
                time_remaining: 0.0,
                lead_target: true,
                old_check_for_aiming: true,
            },
            "TnShootAtLocationOperator" => Kind::ShootAtLocation {
                time_remaining: 0.0,
                location: 0,
                stop_shooting_when_finished: false,
            },
            "TnShootNearLocationOperator" => Kind::ShootNearLocation {
                location: 0,
                direction: (0.0, 0.0),
                duration: 0.0,
                time_remaining: 0.0,
                pitch_half: 0.0,
                yaw_half: 0.0,
            },
            "TnPlayAnimationOperator" => {
                interruptible = false;
                Kind::PlayAnimation {
                    params: AnimParams::default(),
                    additive: false,
                    complete: false,
                    focus: 0,
                    interruptible: false,
                }
            }
            "TnPlayHitReactionOperator" => Kind::PlayHitReaction { complete: false },
            "TnHitReactionOperator" => Kind::HitReaction {
                complete: false,
                started: false,
            },
            "TnKnockedbackOperator" => Kind::Knockedback {
                direction: Vec3::ZERO,
                landing: false,
                complete: false,
                playing: AnimParams::default(),
                land: AnimParams::default(),
                in_air: AnimParams::default(),
                take_off: AnimParams::default(),
            },
            "TnPlayPositionRelativeAnimationOperator" => {
                interruptible = false;
                Kind::PlayPositionRelativeAnimation {
                    found: AnimParams::default(),
                    found_out: AnimParams::default(),
                    complete: false,
                    opening_complete: false,
                    playing_out: false,
                    interruptible: false,
                    end_event: -1,
                }
            }
            "TnMoveInRandomDirectionOperator" => Kind::MoveInRandomDirection {
                current_time: 0.0,
                destination: Vec3::ZERO,
            },
            "TnEnterCoverOperator" => Kind::EnterCover {
                time_left: 0.5,
                tactical: None,
            },
            "TnPeekFromCoverOperator" => Kind::PeekFromCover {
                current_time: 0.0,
                tactical: None,
                focus: 0,
            },
            "TnMeleeEnemyOperator" => Kind::MeleeEnemy {
                interruptible: true,
                succeed_when_complete: false,
                melee_type: 10,
            },
            "TnThrowGrenadeOperator" => Kind::ThrowGrenade {
                status: 0,
                grenade: String::new(),
            },
            "TnStrafeAndShootOperator" => Kind::StrafeAndShoot {
                side_angle: 60.0,
                max_range: 6000.0,
                min_range: 1550.0,
                enemy_start: Vec3::ZERO,
                pawn_start: Vec3::ZERO,
                relative_ref: 0,
                current_time: 0.0,
                destination: Vec3::ZERO,
            },
            "TnAnimatedShootingAttackOperator" => Kind::AnimatedShootingAttack {
                params: AnimParams::default(),
                complete: false,
                focus: 0,
            },
            "TnSideStepOperator" => Kind::SideStep {
                params: AnimParams::default(),
                complete: false,
            },
            "TnMoveAwayFromEnemyOperator" => Kind::MoveAwayFromEnemy {
                current_time: 0.0,
                destination: Vec3::ZERO,
                enemy: None,
            },
            "TnEquipWeaponOperator" => Kind::EquipWeapon {
                finish_immediately: false,
                aim_location: 0,
            },
            "TnUnequipWeaponOperator" => Kind::UnequipWeapon {
                finish_immediately: false,
            },
            "TnTriggerAbilityOperator" => {
                interruptible = false;
                Kind::TriggerAbility {
                    complete_on_activate: false,
                }
            }
            "TnScanSniperEnemyOperator" => Kind::ScanSniperEnemy {
                last_enemy: None,
                scan_duration: 0.0,
                scan_time: 0.0,
                hold_lock_on_duration: 0.0,
                lock_on_duration: 0.0,
                lock_on_time: 0.0,
            },
            "HtnOperator" => Kind::Base,
            "TnBehaviorOperator" => Kind::Behavior,
            "TnPlayReactionOperator" => Kind::PlayReaction {
                time_remaining: 0.0,
            },
            "TnPlayInjuredReactionOperator" => Kind::PlayInjuredReaction {
                params: AnimParams::default(),
                playing: true,
            },
            "TnReloadWeaponOperator" => Kind::ReloadWeapon {
                interruptible: true,
            },
            "TnShootAtTrackedLocationOperator" => Kind::ShootAtTrackedLocation {
                location_ref: 0,
                duration: 0.0,
                time_remaining: 0.0,
                tracking_speed: 0.0,
                ignore_aiming: false,
            },
            "TnMoveToOperator" => Kind::MoveTo(Box::default()),
            "TnMoveToClearAttackChannelOperator" => Kind::MoveToClearAttackChannel {
                current_time: 0.0,
                destination: Vec3::ZERO,
            },
            "TnIdleInAttackChannelOperator" => Kind::IdleInAttackChannel,
            "TnSquadMeleeEnemyOperator" => Kind::SquadMeleeEnemy {
                melee_type: super::melee::MELEE_ANY,
                satisfied_squad_command: 0,
            },
            "TnReactOperator" => Kind::React {
                params: AnimParams::default(),
                world_state: 0,
                interruptible: true,
                complete: false,
            },
            "TnReactForDurationOperator" => Kind::ReactForDuration {
                current_time: 0.0,
                duration: 0.0,
            },
            "TnStopMoveOperator" => Kind::StopMove,
            "TnChangeRobotFormOperator" => Kind::ChangeRobotForm {
                target_form: 0,
                look_at: 0,
                succeeded: false,
            },
            "TnChooseNearbyLocationOperator" => Kind::ChooseNearbyLocation,
            "TnAreaOfEffectAttackOperator" => Kind::AreaOfEffectAttack {
                attack: None,
                complete: false,
            },
            "TnGuardScanPatrolPointOperator" => Kind::GuardScanPatrolPoint {
                observe: 0.0,
                yaw: 0.0,
                half_cone: 0.0,
                scan_duration: 0.0,
                time_elapsed: 0.0,
                index: 0,
                animations: Vec::new(),
            },
            "TnPlayTellFacingFocusOperator" => Kind::PlayTellFacingFocus { complete: false },
            "TnStopRotatingOperator" => Kind::StopRotating,
            "TnClaimLeapSmashOperator" => Kind::ClaimLeapSmash,
            "TnMeleeLungeOperator" => Kind::MeleeLunge(Box::default()),
            "TnAnimatedSkidOperator" => Kind::AnimatedSkid(Box::default()),
            "TnLeapSmashOperator" => Kind::LeapSmash(Box::default()),
            "TnSpawnDronesOperator" => Kind::SpawnDrones(Box::default()),
            "TnChooseNewSniperPointOperator" => Kind::ChooseNewSniperPoint,
            "TnChooseNewSniperPointTowardsGoalOperator" => Kind::ChooseNewSniperPointTowardsGoal,
            "TnChooseNewSniperSearchPointOperator" => Kind::ChooseNewSniperSearchPoint,
            "TnChooseNewSniperAttackPointOperator" => Kind::ChooseNewSniperAttackPoint,
            "TnFlyToSniperPointOperator" => Kind::FlyToSniperPoint(Box::default()),
            "TnAttachToWallOperator" => Kind::AttachToWall(Box::default()),
            "TnDetachFromWallOperator" => Kind::DetachFromWall(Box::default()),
            "TnCrawlOnSniperPointOperator" => Kind::CrawlOnSniperPoint(Box::default()),
            "TnDroneMoveToOperator" => Kind::DroneMoveTo,
            "TnHoverAtLocationOperator" => Kind::HoverAtLocation(Default::default()),
            "TnExplodeOperator" => Kind::Explode,
            "TnChooseNewDefendActorLocationOperator" => Kind::ChooseNewDefendActorLocation,
            "TnPlaneHoverToOperator" => Kind::PlaneHoverTo,
            "TnChooseNewAirAttackPointOperator" => Kind::ChooseNewAirAttackPoint,
            "TnChooseNewAerialPursuitPointOperator" => Kind::ChooseNewAerialPursuitPoint,
            "TnChooseNewFlyingCirclingPointOperator" => Kind::ChooseNewFlyingCirclingPoint,
            "TnChooseJetRobotAttackPointOperator" => Kind::ChooseJetRobotAttackPoint,
            "TnChooseNewFlyingInvestigateLocationOperator" => {
                Kind::ChooseNewFlyingInvestigateLocation
            }
            "TnHoverToFaceOperator" => Kind::HoverToFace(Box::default()),
            "TnPlaneDashOperator" => Kind::PlaneDash(Box::default()),
            "TnTransformOperator" => Kind::Transform(Default::default()),
            "TnJumpOperator" => Kind::Jump(Default::default()),
            "TnAnimatedAttackOperator" => Kind::AnimatedAttack {
                params: AnimParams::default(),
                additive: false,
                complete: false,
            },
            "TnShootAtEnemyHeadOnOperator" => Kind::ShootAtEnemyHeadOn(Box::default()),
            "TnShootAtEnemyFireForEffectOperator" => {
                Kind::ShootAtEnemyFireForEffect(Box::default())
            }
            other => Kind::Unknown(other.to_string()),
        };
        Op {
            kind,
            task,
            serial,
            interruptible,
            fail_context: "",
        }
    }

    fn cb(&self, kind: u8) -> Option<Callback> {
        Some(Callback {
            owner: self.serial,
            kind,
        })
    }

    /// HtnOperator.get_Interruptible (the classes with their own).
    pub fn is_interruptible(&self) -> bool {
        match &self.kind {
            Kind::PlayAnimation { interruptible, .. }
            | Kind::PlayPositionRelativeAnimation { interruptible, .. }
            | Kind::MeleeEnemy { interruptible, .. }
            | Kind::React { interruptible, .. }
            | Kind::ReloadWeapon { interruptible } => *interruptible,
            Kind::MoveTo(m) => !m.jumping && self.interruptible,
            _ => self.interruptible,
        }
    }
}

/// get_IsShootingDesired: a clear line of fire, and no weapon being equipped.
fn shooting_desired(b: &Brain, ctx: &Ctx) -> bool {
    b.core.ws_bool(htn::WS_WEAPON_HAS_CLEAR_LINE_OF_FIRE) && !ctx.pawn.weapon.is_equipping()
}

/// TnShootAtTrackedLocationOperator.get_IsShootingDesired: told to ignore its aim,
/// or its aimer on its target (TnHtnAiController.get_IsAimingAtTarget).
fn tracked_shooting_desired(ignore_aiming: bool, ctx: &Ctx) -> bool {
    ignore_aiming || super::weapon::is_aiming_at_target(ctx.pawn, ctx.pawn.muzzle)
}

/// The behaviour flags a TnBehaviorOperator sets as it starts (its class's, the
/// task description's added), or None for an operator that only adds its
/// description's.
pub fn behaviour_class_flags(op: &Op, b: &Brain, ctx: &Ctx) -> Option<u32> {
    match &op.kind {
        Kind::Idle | Kind::Behavior | Kind::ReloadWeapon { .. } => Some(0),
        Kind::ShootAtTrackedLocation { ignore_aiming, .. } => Some(
            if tracked_shooting_desired(*ignore_aiming, ctx) {
                BEHAVIOR_SHOOTER
            } else {
                0
            } | BEHAVIOR_HEAD_TRACKING
                | BEHAVIOR_WEAPON_TRACKING
                | BEHAVIOR_AIMER,
        ),
        Kind::NavigatingIdle | Kind::UpdateNavigationParams => Some(BEHAVIOR_ALLOW_MOTION),
        Kind::PlayAnimation { focus, .. } => Some(if *focus != 0 {
            BEHAVIOR_ROTATE_TO_FOCUS
        } else {
            0
        }),
        Kind::ShootAtEnemy { .. } => Some(
            if shooting_desired(b, ctx) {
                BEHAVIOR_SHOOTER
            } else {
                0
            } | BEHAVIOR_HEAD_TRACKING
                | BEHAVIOR_WEAPON_TRACKING
                | BEHAVIOR_AIMER,
        ),
        Kind::ShootAtLocation { .. } => Some(
            BEHAVIOR_SHOOTER | BEHAVIOR_HEAD_TRACKING | BEHAVIOR_WEAPON_TRACKING | BEHAVIOR_AIMER,
        ),
        Kind::StrafeAndShoot { .. } => Some(
            BEHAVIOR_HEAD_TRACKING
                | BEHAVIOR_ROTATE_TO_FOCUS
                | BEHAVIOR_AIMER
                | BEHAVIOR_ALLOW_MOTION,
        ),
        Kind::AnimatedShootingAttack { focus, .. } => Some(
            BEHAVIOR_SHOOTER
                | BEHAVIOR_HEAD_TRACKING
                | BEHAVIOR_WEAPON_TRACKING
                | BEHAVIOR_AIMER
                | if *focus != 0 {
                    BEHAVIOR_ROTATE_TO_FOCUS
                } else {
                    0
                },
        ),
        Kind::MoveAwayFromEnemy { .. } | Kind::MoveToClearAttackChannel { .. } => Some(
            BEHAVIOR_HEAD_TRACKING
                | BEHAVIOR_ROTATE_TO_FOCUS
                | BEHAVIOR_AIMER
                | BEHAVIOR_ALLOW_MOTION,
        ),
        _ => None,
    }
}

/// FindAimLocationOnTarget: its target's eyes (TnAiTargeter), drawn towards the
/// aimer's bad aim point as its competency falls.
pub fn find_aim_location_on_target(b: &Brain, ctx: &Ctx, target: Entity) -> Vec3 {
    let Some(a) = ctx.view.actor(target) else {
        return ctx.pawn.loc + ctx.pawn.forward() * 200.0;
    };
    let good = ctx
        .view
        .player(target)
        .map_or(a.loc + Vec3::Z * a.half_height * 0.8, |p| p.eyes);
    let c = b.core.competency;
    let bad = if c < 1.0 {
        super::weapon::bad_aim_point(ctx.pawn, good, a.loc, a.radius, a.half_height)
    } else {
        good
    };
    bad.lerp(good, c)
}

/// The convert of a tactical's cover to the stance hiding behind it
/// (TnEnterCoverOperator.ConvertCoverTypeToStance).
fn hide_stance(cover: Cover) -> u8 {
    match cover {
        Cover::Low => stance::LOW_COVER_HIDE,
        Cover::Ledge => stance::LEDGE_COVER_HIDE,
        Cover::LowLeftCorner | Cover::LowRightCorner => stance::LOW_CORNER_COVER_HIDE,
        Cover::LeftCorner | Cover::RightCorner => stance::CORNER_COVER_HIDE,
        Cover::None => stance::OPEN_COVER_HIDE,
    }
}

/// TnPeekFromCoverOperator.ConvertCoverTypeToStance.
fn peek_stance(cover: Cover) -> u8 {
    match cover {
        Cover::Low => stance::LOW_COVER_PEEK,
        Cover::LowLeftCorner | Cover::LowRightCorner => stance::LOW_CORNER_COVER_PEEK,
        Cover::LeftCorner | Cover::RightCorner => stance::CORNER_COVER_PEEK,
        Cover::Ledge => stance::LEDGE_COVER_PEEK,
        Cover::None => stance::OPEN_COVER_PEEK,
    }
}

/// GetLowCoverHandedness / GetCoverHandedness: the hand on the focus's side of
/// the node (0 right, 1 left).
fn cover_handedness(t: &TacticalNode, focus: Vec3) -> u8 {
    let right = super::weapon::from_local(Vec3::Y, 0.0, t.yaw);
    if (focus - t.location).dot(right) > 0.0 {
        0
    } else {
        1
    }
}

/// The tactical node's facing as a vector.
fn tactical_forward(t: &TacticalNode) -> Vec3 {
    ue::yaw_vec(t.yaw)
}

// ---------------------------------------------------------------------------
// Start
// ---------------------------------------------------------------------------

/// HtnOperator.Start's OnStart.
pub fn start(op: &mut Op, b: &mut Brain, ctx: &mut Ctx) -> bool {
    let task = op.task;
    let serial = op.serial;
    let cb = |k: u8| {
        Some(Callback {
            owner: serial,
            kind: k,
        })
    };
    let bp = b.blueprint.clone();
    match &mut op.kind {
        Kind::NoOp | Kind::Idle => true,
        Kind::ClearArea => {
            ctx.shared
                .broadcast(b.core.entity, EventId::AreaClear, None);
            true
        }
        Kind::ClearOverrideGoal => {
            b.clear_override_goal();
            true
        }
        Kind::ChooseNewSearchPoint => {
            let loc = ctx.pawn.loc;
            let now = ctx.view.now;
            with_sensor!(b, search, s => s.choose_new_search_point(b, loc, now));
            true
        }
        Kind::ChooseNewPatrolPoint => {
            let store = &mut b.sensors.patrol;
            store.index += 1;
            if store.index >= store.route.len() as i32 {
                if store.patrol_type == 0 {
                    b.core.set_ws_bool(htn::WS_PATROL_AVAILABLE, false);
                    return false;
                }
                store.index = 0;
            }
            let Some(&(dest, _)) = b.sensors.patrol.route.get(b.sensors.patrol.index as usize)
            else {
                return false;
            };
            b.core
                .store(super::brain::LR_PATROL_POINT, dest, Vec3::ZERO);
            true
        }
        Kind::ExitCover => {
            b.set_behavior(ctx, 0, false);
            ctx.pawn.stance.clear();
            true
        }
        Kind::MakeIncapableOfSquadAction => {
            let action = task.int(0) as usize;
            if action < shared::SAT_NUM {
                let now = ctx.view.now;
                ctx.shared
                    .squad
                    .make_incapable(b.core.entity, action, now, &mut ctx.shared.writes);
            }
            true
        }
        Kind::ReleaseSquadAction => {
            let action = task.int(0) as usize;
            let status = task.int(1) as u8;
            if action < shared::SAT_NUM {
                let now = ctx.view.now;
                ctx.shared.squad.release(
                    b.core.entity,
                    action,
                    status,
                    now,
                    &mut ctx.shared.writes,
                );
            }
            true
        }
        Kind::ClaimSquadAction {
            action,
            release_on_plan_finish,
        } => {
            *action = task.int(0).max(0) as usize;
            *release_on_plan_finish = task.int(1) == 1;
            if *action >= shared::SAT_NUM {
                return false;
            }
            claim_squad_action(b, ctx, *action)
        }
        Kind::PlayDialog => {
            if task.n != 1 {
                return false;
            }
            if let Asset::Dialog(d) = bp.asset(task.int(0)) {
                ctx.shared
                    .dialog
                    .push(super::audio::DialogRequest::say(b.core.entity, &d));
            }
            true
        }
        Kind::ClearStance => {
            let specific = if task.n > 0 {
                task.int(0) as u8
            } else {
                stance::NONE
            };
            if specific == stance::NONE || ctx.pawn.current_stance() == specific {
                ctx.pawn.stance.clear();
            }
            true
        }
        Kind::SetStance => {
            let s = task.int(0) as u8;
            match task.int(1) {
                0 => ctx.pawn.stance.push(s),
                1 => ctx.pawn.stance.pop(s),
                2 => {
                    ctx.pawn.stance.clear();
                    ctx.pawn.stance.push(s);
                }
                _ => {}
            }
            true
        }
        Kind::IdleForDuration { time_remaining } => {
            b.set_behavior(ctx, 0, false);
            let mut t = task.float(0);
            let randomness = if task.n > 1 { task.float(1) } else { 0.0 };
            t = ue::randomize(t, randomness, &mut b.core.rng);
            *time_remaining = t;
            true
        }
        Kind::NavigatingIdle | Kind::UpdateNavigationParams => {
            if b.sensors.path.is_none() {
                return false;
            }
            motion::set_speed(ctx.pawn, task.int(0) as u8);
            if let Some(path) = b.sensors.path.as_mut() {
                path.set_look_at_reference(task.int(1).max(0) as usize);
            }
            true
        }
        Kind::RobotMoveTo => {
            if b.sensors.path.is_none() {
                return false;
            }
            b.add_behavior(ctx, BEHAVIOR_ALLOW_MOTION);
            let (dest, range, speed, look) = (
                task.int(0).max(0) as usize,
                task.int(1),
                task.int(2) as u8,
                task.int(3).max(0) as usize,
            );
            with_sensor!(b, path, s => s.move_to_location(b, ctx, dest, range, speed, look));
            true
        }
        // (No interacts in the demo's Escalation.)
        Kind::UseInteract => {
            op.fail_context = "No Interact DataStore";
            false
        }
        Kind::EnableTacticalEvaluator => {
            let Asset::Evaluator(class) = bp.asset(task.int(0)) else {
                return true;
            };
            let Some(t) = b.sensors.tactical.as_mut() else {
                return true;
            };
            match t.picker.find_evaluator(class) {
                Some(e) => {
                    e.set_enabled(true);
                    true
                }
                None => {
                    op.fail_context = "No Evaluator";
                    false
                }
            }
        }
        Kind::LookAt {
            time_remaining,
            location,
        } => {
            *location = task.int(0).max(0) as usize;
            *time_remaining = ue::randomize(task.float(1), b.core.randomness, &mut b.core.rng);
            b.set_behavior(ctx, BEHAVIOR_HEAD_TRACKING | BEHAVIOR_AIMER, false);
            true
        }
        Kind::TurnToFocus {
            use_look_direction,
            min_cos,
            focus,
        } => {
            *focus = task.int(0).max(0) as usize;
            *min_cos = if task.n > 1 {
                (task.int(1) as f32 * ue::DEG_TO_RAD).cos()
            } else {
                0.984
            };
            if task.n > 2 {
                *use_look_direction = task.int(2) == 1;
            }
            b.set_behavior(
                ctx,
                BEHAVIOR_HEAD_TRACKING | BEHAVIOR_AIMER | BEHAVIOR_ROTATE_TO_FOCUS,
                false,
            );
            true
        }
        Kind::LookForEnemies {
            location_ref,
            location,
            should_shoot,
            half_cone,
            duration,
            direction,
            time_remaining,
        } => {
            if task.n != 4 {
                return false;
            }
            *location_ref = task.int(0).max(0) as usize;
            *location = look_for_location(b, ctx, *location_ref);
            ctx.pawn.set_focal_point(*location);
            *direction = (*location - ctx.pawn.loc) * Vec3::new(1.0, 1.0, 0.0);
            *duration = ue::randomize(task.float(1), b.core.randomness, &mut b.core.rng);
            *half_cone = task.int(2) as f32 * 0.5;
            *time_remaining = *duration;
            *should_shoot = task.int(3) != 0;
            true
        }
        Kind::ScanPatrolPoint {
            time_left,
            index,
            animations,
            active,
        } => {
            let store = &b.sensors.patrol;
            if store.index < 0 || store.index as usize >= store.route.len() {
                return false;
            }
            let (dest, observe) = store.route[store.index as usize];
            *time_left = observe;
            b.set_behavior(ctx, BEHAVIOR_ROTATE_TO_FOCUS, false);
            let _ = dest;
            if observe > 0.0 {
                animations.push("patrol_idle".into());
            }
            if !animations.is_empty() {
                *active = animations[*index].clone();
                let mut p = AnimParams::make(active, 0.5, 0.5, super::anim::RMM_ACCEL);
                p.mask = super::anim::MASK_UPPER_BODY;
                ctx.pawn
                    .anim
                    .enqueue(p, cb(CB_SCAN_FINISHED), cb(CB_SCAN_FAILED));
            }
            true
        }
        Kind::ShootAtEnemy {
            time_remaining,
            lead_target,
            old_check_for_aiming,
        } => {
            if b.core.enemy.is_none() {
                return false;
            }
            *time_remaining = task.float(0);
            if let Some(w) = b.sensors.weapon.as_mut() {
                let check = if task.n > 4 { task.int(4) == 1 } else { true };
                *old_check_for_aiming = w.check_for_aiming;
                w.check_for_aiming = check;
            }
            if task.n > 1 {
                b.shooter.set_unrestrained_fire(task.int(1) == 1);
            }
            let randomness = if task.n > 2 {
                task.float(2)
            } else {
                b.core.randomness
            };
            if task.n > 3 {
                *lead_target = task.int(3) == 1;
            }
            *time_remaining = ue::randomize(*time_remaining, randomness, &mut b.core.rng);
            true
        }
        Kind::ShootAtLocation {
            time_remaining,
            location,
            stop_shooting_when_finished,
        } => {
            *location = task.int(0).max(0) as usize;
            *time_remaining = ue::randomize(task.float(1), b.core.randomness, &mut b.core.rng);
            *stop_shooting_when_finished = b.core.behavior & BEHAVIOR_SHOOTER == 0;
            update_shoot_location(b, ctx, *location);
            true
        }
        Kind::ShootNearLocation {
            location,
            direction,
            duration,
            time_remaining,
            pitch_half,
            yaw_half,
        } => {
            *location = task.int(0).max(0) as usize;
            let at = b.core.fetch(*location);
            let d = (at - ctx.pawn.loc) * Vec3::new(1.0, 1.0, 0.0);
            *direction = (ue::pitch_of(d), ue::yaw_of(d));
            *duration = ue::randomize(task.float(1), b.core.randomness, &mut b.core.rng);
            *time_remaining = *duration;
            *pitch_half = task.int(2) as f32 * 0.5;
            *yaw_half = task.int(3) as f32 * 0.5;
            b.set_behavior(
                ctx,
                BEHAVIOR_HEAD_TRACKING
                    | BEHAVIOR_WEAPON_TRACKING
                    | BEHAVIOR_ROTATE_TO_FOCUS
                    | BEHAVIOR_AIMER
                    | BEHAVIOR_SHOOTER,
                false,
            );
            ctx.pawn.set_focal_point(at);
            b.shooter.set_unrestrained_fire(true);
            update_look_near(
                ctx,
                *direction,
                *time_remaining / duration.max(1e-3),
                *pitch_half,
                *yaw_half,
            );
            true
        }
        Kind::PlayAnimation {
            params,
            additive,
            complete,
            focus,
            interruptible,
        } => {
            *additive = task.n > 3 && task.int(3) == 1;
            let mut p = bp.anim(task.int(0), ctx.pawn.current_stance());
            p.priority = 1;
            p.override_mode = super::anim::OVERRIDE_IF_HIGHER_OR_EQUAL;
            *complete = false;
            if *additive {
                ctx.pawn
                    .anim
                    .enqueue_additive(p.clone(), cb(CB_COMPLETE), cb(CB_COMPLETE));
            } else {
                ctx.pawn
                    .anim
                    .enqueue(p.clone(), cb(CB_COMPLETE), cb(CB_COMPLETE));
            }
            *params = p;
            if task.n > 1 {
                *focus = task.int(1).max(0) as usize;
            }
            if task.n > 2 {
                *interruptible = task.int(2) == 1;
            }
            true
        }
        Kind::PlayHitReaction { .. } => {
            let params = b
                .sensors
                .hit
                .as_ref()
                .map(|h| h.params.clone())
                .unwrap_or_default();
            ctx.pawn
                .anim
                .enqueue(params, cb(CB_COMPLETE), cb(CB_COMPLETE));
            b.set_behavior(ctx, 0, false);
            if task.n > 0 {
                op.interruptible = task.bool(0);
            }
            true
        }
        Kind::HitReaction { complete, started } => {
            let Asset::Picker(picker) = bp.asset(task.int(0)) else {
                *complete = true;
                return true;
            };
            let tdi = b
                .sensors
                .hit
                .as_ref()
                .and_then(|h| h.tdi.clone())
                .unwrap_or_default();
            let params = picker.choose(ctx.pawn, &tdi);
            ctx.pawn
                .anim
                .enqueue(params, cb(CB_COMPLETE), cb(CB_COMPLETE));
            *started = true;
            b.set_behavior(ctx, BEHAVIOR_LEDGE_FALL, false);
            true
        }
        Kind::Knockedback {
            direction,
            complete,
            playing,
            land,
            in_air,
            take_off,
            ..
        } => {
            let get = |i: usize| {
                let mut p = bp.anim(task.int(i), stance::NONE);
                p.priority = 1;
                p.override_mode = super::anim::OVERRIDE_IF_HIGHER_OR_EQUAL;
                p.root_motion = super::anim::RMM_IGNORE;
                p
            };
            *take_off = get(0);
            *in_air = get(1);
            in_air.looping = true;
            *land = get(2);
            land.root_motion = super::anim::RMM_ACCEL;
            let air_time = ctx.pawn.do_knockback();
            if air_time > 0.5 {
                b.set_behavior(ctx, BEHAVIOR_ROTATE_TO_FOCUS, false);
                ctx.pawn
                    .anim
                    .enqueue(take_off.clone(), cb(CB_TAKE_OFF_COMPLETE), None);
                *playing = take_off.clone();
            } else {
                motion::stop(ctx.pawn);
                *complete = true;
            }
            *direction = -ue::normal(ctx.pawn.vel * Vec3::new(1.0, 1.0, 0.0));
            true
        }
        Kind::PlayPositionRelativeAnimation {
            found,
            found_out,
            complete,
            interruptible,
            end_event,
            ..
        } => {
            let position = b.core.fetch(task.int(0).max(0) as usize);
            let Asset::AnimSet(set) = bp.asset(task.int(1)) else {
                *complete = true;
                return true;
            };
            if task.n > 3 {
                *end_event = task.int(3);
            }
            if task.n > 2 {
                *interruptible = task.int(2) == 1;
            }
            let local_yaw = ue::yaw_of(position - ctx.pawn.loc) - ctx.pawn.yaw;
            let Some((anim, out)) = set.find(local_yaw) else {
                *complete = true;
                return true;
            };
            *found = anim;
            *found_out = out;
            found_out.looping = false;
            ctx.pawn.stance.set_handedness(0);
            if found_out.is_none() {
                ctx.pawn
                    .anim
                    .enqueue(found.clone(), cb(CB_COMPLETE), cb(CB_COMPLETE));
            } else {
                ctx.pawn.anim.enqueue(
                    found.clone(),
                    cb(CB_OPENING_COMPLETE),
                    cb(CB_OPENING_COMPLETE),
                );
            }
            b.set_behavior(ctx, BEHAVIOR_LEDGE_FALL, false);
            true
        }
        Kind::MoveInRandomDirection {
            current_time,
            destination,
        } => {
            let Some(d) = random_move_destination(b, ctx, task.int(1) as f32) else {
                return false;
            };
            *destination = d;
            b.set_behavior(
                ctx,
                BEHAVIOR_HEAD_TRACKING
                    | BEHAVIOR_ROTATE_TO_FOCUS
                    | BEHAVIOR_AIMER
                    | BEHAVIOR_ALLOW_MOTION,
                false,
            );
            let r = ctx.pawn.radius;
            motion::move_to_point(ctx.pawn, d, 0, r);
            motion::set_speed(ctx.pawn, task.int(0) as u8);
            *current_time = 0.0;
            true
        }
        Kind::EnterCover { tactical, .. } => {
            let Some(t) = b.sensors.tactical.as_ref().and_then(|s| s.current(ctx)) else {
                return false;
            };
            ctx.pawn.stance.clear();
            ctx.pawn.stance.push(hide_stance(t.cover));
            let focus_ref = task.int(0).max(0) as usize;
            let focus = if focus_ref != 0 {
                b.core.fetch(focus_ref)
            } else {
                t.location + tactical_forward(&t) * 1000.0
            };
            match t.cover {
                Cover::Ledge | Cover::Low | Cover::None => {
                    ctx.pawn.stance.set_handedness(cover_handedness(&t, focus))
                }
                Cover::LowLeftCorner | Cover::LeftCorner => ctx.pawn.stance.set_handedness(0),
                Cover::LowRightCorner | Cover::RightCorner => ctx.pawn.stance.set_handedness(1),
            }
            b.set_behavior(ctx, BEHAVIOR_ROTATE_TO_FOCUS, false);
            ctx.pawn.set_look_at_point(focus);
            if matches!(t.cover, Cover::Low | Cover::None | Cover::Ledge) {
                ctx.pawn.set_focal_point(focus);
            } else {
                ctx.pawn.set_focal_rotation(t.yaw);
            }
            motion::move_to_point(ctx.pawn, t.location, 0, 10.0);
            *tactical = Some(t);
            true
        }
        Kind::PeekFromCover {
            tactical, focus, ..
        } => {
            let Some(t) = b.sensors.tactical.as_ref().and_then(|s| s.current(ctx)) else {
                return false;
            };
            *focus = task.int(0).max(0) as usize;
            ctx.pawn.stance.clear();
            let f = peek_focus(b, &t, *focus);
            if matches!(t.cover, Cover::Low | Cover::None | Cover::Ledge) {
                ctx.pawn.stance.set_handedness(cover_handedness(&t, f));
            }
            ctx.pawn.stance.push(peek_stance(t.cover));
            motion::set_speed(ctx.pawn, 2);
            let dest = peek_destination(ctx.pawn.radius, &t);
            motion::move_to_point(ctx.pawn, dest, 0, 10.0);
            b.set_behavior(ctx, BEHAVIOR_ROTATE_TO_FOCUS, false);
            let confirmed = b.core.fetch(super::brain::LR_CONFIRMED_ENEMY);
            ctx.pawn.set_look_at_point(confirmed);
            *tactical = Some(t);
            true
        }
        Kind::MeleeEnemy {
            interruptible,
            succeed_when_complete,
            melee_type,
        } => {
            *melee_type = 10;
            let chance = task.float(0);
            if chance > b.core.rng.frand() {
                *melee_type = 0;
            }
            if task.n > 2 {
                *succeed_when_complete = task.bool(2);
            }
            if task.int(1) == 1 {
                b.set_behavior(
                    ctx,
                    BEHAVIOR_ROTATE_TO_FOCUS
                        | BEHAVIOR_HEAD_TRACKING
                        | BEHAVIOR_AIMER
                        | BEHAVIOR_ALLOW_MOTION,
                    false,
                );
            } else {
                b.set_behavior(ctx, BEHAVIOR_HEAD_TRACKING, false);
            }
            super::melee::start_attack(b, ctx, *melee_type);
            if task.n > 3 {
                *interruptible = task.bool(3);
            }
            true
        }
        Kind::ThrowGrenade { status, grenade } => {
            let anim = bp.anim(task.int(1), ctx.pawn.current_stance());
            if anim.is_none() {
                *status = 3;
                return true;
            }
            let Asset::Projectile(class) = bp.asset(task.int(0)) else {
                return false;
            };
            *grenade = class;
            let target = b.core.fetch(super::brain::LR_GRENADE_AIM_POINT);
            b.set_behavior(
                ctx,
                BEHAVIOR_ROTATE_TO_FOCUS | BEHAVIOR_AIMER | BEHAVIOR_HEAD_TRACKING,
                false,
            );
            ctx.pawn.set_focal_point(target);
            ctx.pawn.set_look_at_point(target);
            ctx.pawn
                .anim
                .enqueue(anim, cb(CB_THROW_COMPLETE), cb(CB_THROW_COMPLETE));
            true
        }
        Kind::StrafeAndShoot {
            side_angle,
            max_range,
            min_range,
            enemy_start,
            pawn_start,
            relative_ref,
            current_time,
            destination,
        } => {
            *pawn_start = ctx.pawn.loc;
            *relative_ref = task.int(1).max(0) as usize;
            *enemy_start = b.core.fetch_ground(*relative_ref);
            let (centre, right, left) =
                centre_and_corners(*pawn_start, *enemy_start, *side_angle, *max_range);
            let mut best = 0.0;
            for _ in 0..5 {
                let d = strafe_destination(
                    b,
                    ctx,
                    centre,
                    right,
                    left,
                    *enemy_start,
                    *pawn_start,
                    *min_range,
                    *max_range,
                );
                let dist = ue::vsize_sq2d(d - *pawn_start);
                if dist > best {
                    best = dist;
                    *destination = d;
                    if dist > 800.0 * 800.0 {
                        break;
                    }
                }
            }
            let r = ctx.pawn.radius;
            motion::move_to_point(ctx.pawn, *destination, 0, r);
            motion::set_speed(ctx.pawn, task.int(0) as u8);
            *current_time = 0.0;
            true
        }
        Kind::AnimatedShootingAttack {
            params,
            complete,
            focus,
        } => {
            // (Htn.GetHighPriorityAnimParameterValue.)
            let mut p = bp.anim(task.int(0), ctx.pawn.current_stance());
            p.priority = 1;
            p.override_mode = super::anim::OVERRIDE_IF_HIGHER_OR_EQUAL;
            *complete = false;
            ctx.pawn
                .anim
                .enqueue(p.clone(), cb(CB_COMPLETE), cb(CB_COMPLETE));
            *params = p;
            b.set_behavior(
                ctx,
                BEHAVIOR_HEAD_TRACKING | BEHAVIOR_WEAPON_TRACKING | BEHAVIOR_AIMER,
                false,
            );
            *focus = task.int(1).max(0) as usize;
            true
        }
        Kind::SideStep { params, complete } => {
            let Some(enemy) = b.core.enemy.and_then(|e| ctx.view.actor(e)) else {
                return false;
            };
            let right = bp.asset(task.int(0));
            let left = bp.asset(task.int(1));
            let from = ctx.pawn.loc;
            let to_right = traversable_distance(ctx, enemy.loc, from, -2000.0);
            let to_left = traversable_distance(ctx, enemy.loc, from, 2000.0);
            let mut found = Vec::new();
            if let Asset::DistanceAnimSet(set) = right {
                set.working_with(to_right, &mut found);
            }
            if let Asset::DistanceAnimSet(set) = left {
                set.working_with(to_left, &mut found);
            }
            if found.is_empty() {
                *complete = true;
                return true;
            }
            let i = b.core.rng.rand(found.len() as i32) as usize;
            *params = found.swap_remove(i);
            ctx.pawn
                .anim
                .enqueue(params.clone(), cb(CB_COMPLETE), cb(CB_COMPLETE));
            b.set_behavior(
                ctx,
                BEHAVIOR_ROTATE_TO_FOCUS
                    | BEHAVIOR_HEAD_TRACKING
                    | BEHAVIOR_AIMER
                    | BEHAVIOR_WEAPON_TRACKING
                    | BEHAVIOR_SHOOTER
                    | BEHAVIOR_LEDGE_FALL,
                false,
            );
            b.core.set_ws(htn::WS_LOCATION, 0);
            true
        }
        Kind::MoveAwayFromEnemy {
            current_time,
            destination,
            enemy,
        } => {
            let Some(e) = b.core.enemy else {
                return false;
            };
            let Some(a) = ctx.view.actor(e) else {
                return false;
            };
            *enemy = Some(e);
            // CalculateMoveDestination: straight away from it, to the distance
            // from it asked for (as far as the mesh goes).
            let p = &*ctx.pawn;
            let away = ue::normal((p.loc - a.loc) * Vec3::new(1.0, 1.0, 0.0));
            let enemy_distance = ue::range_2d(p.loc, p.radius, a.loc, 0.0);
            let distance = task.int(1) as f32;
            if distance - enemy_distance <= 0.0 {
                return false;
            }
            let mut dest = p.feet() + away * (distance - enemy_distance);
            let (ok, end) = super::sensors::can_walk_line(p, ctx.view, dest, true);
            if !ok {
                dest = end;
            }
            *destination = dest;
            let r = ctx.pawn.radius;
            motion::move_to_point(ctx.pawn, dest, 0, r);
            motion::set_speed(ctx.pawn, task.int(0) as u8);
            *current_time = 0.0;
            true
        }
        Kind::EquipWeapon {
            finish_immediately,
            aim_location,
        } => {
            let Asset::Weapon(weapon) = bp.asset(task.int(0)) else {
                return false;
            };
            // (FindOrCreateInventory: its blueprint's gun of that class, else the
            // one its pawn carries; SetCurrentWeapon unless it is its weapon.)
            let data = bp
                .gun(&weapon)
                .unwrap_or_else(|| ctx.pawn.weapon.data.clone());
            if !ctx.pawn.weapon.equipped || ctx.pawn.weapon.data.class != data.class {
                super::weapon::equip(ctx.pawn, data);
                let fx = super::gun_fx(&ctx.pawn.weapon.data.class);
                if !fx.equip.is_empty() {
                    let at = ctx.pawn.loc;
                    ctx.shared.sounds.push((ctx.pawn.entity, fx.equip, at));
                }
            }
            if task.n > 1 {
                op.interruptible = task.int(1) == 1;
            }
            if task.n > 2 {
                *finish_immediately = task.int(2) == 1;
            }
            if task.n > 3 {
                *aim_location = task.int(3).max(0) as usize;
            }
            true
        }
        Kind::UnequipWeapon { finish_immediately } => {
            if ctx.pawn.weapon.equipped {
                let fx = super::gun_fx(&ctx.pawn.weapon.data.class);
                if !fx.holster.is_empty() {
                    let at = ctx.pawn.loc;
                    ctx.shared.sounds.push((ctx.pawn.entity, fx.holster, at));
                }
            }
            super::weapon::unequip(b, ctx.pawn);
            if task.n > 0 {
                *finish_immediately = task.int(0) == 1;
            }
            true
        }
        Kind::TriggerAbility {
            complete_on_activate,
        } => {
            *complete_on_activate = task.bool(0);
            // (_AbilityIndex is read only with more than none: as the script has
            // it.)
            let index = if task.n > 0 { task.int(1) } else { 0 };
            let disallow_retrigger = task.n > 1 && task.bool(2);
            b.set_behavior(ctx, 0, false);
            let active = if ctx.pawn.cloak.active { 0 } else { -1 };
            if (!disallow_retrigger || index != active) && index == 0 {
                ctx.pawn.cloak.trigger();
            }
            true
        }
        Kind::ScanSniperEnemy {
            last_enemy,
            scan_duration,
            scan_time,
            hold_lock_on_duration,
            lock_on_duration,
            lock_on_time,
        } => {
            let Some(enemy) = b.core.enemy else {
                return false;
            };
            *last_enemy = Some(enemy);
            *hold_lock_on_duration = task.float(2);
            *lock_on_duration = task.float(0);
            *lock_on_time = 0.0;
            *scan_duration = task.float(1);
            *scan_time = 0.0;
            let at = enemy_view_location(ctx, enemy);
            ctx.pawn.laser.start(at);
            b.set_behavior(ctx, BEHAVIOR_HEAD_TRACKING | BEHAVIOR_AIMER, false);
            ctx.pawn.set_look_at_point(at);
            true
        }
        Kind::Base => false,
        Kind::Behavior => true,
        Kind::PlayReaction { time_remaining } => {
            b.set_behavior(ctx, 0, false);
            *time_remaining = 1.0;
            true
        }
        Kind::PlayInjuredReaction { params, playing } => {
            // (Its injured sensor's reaction: the Snipers' HitReactions; a type
            // with no such sensor gets empty params, an animation that fails at
            // once.)
            *params = b
                .sensors
                .injured
                .as_ref()
                .map_or_else(AnimParams::default, |s| s.reaction_animation());
            b.set_behavior(ctx, 0, false);
            if params.is_none() {
                *playing = false;
            } else {
                ctx.pawn
                    .anim
                    .enqueue(params.clone(), cb(CB_COMPLETE), cb(CB_FAILED));
            }
            true
        }
        Kind::ReloadWeapon { interruptible } => {
            if task.n > 0 {
                *interruptible = task.int(0) == 1;
            }
            ctx.pawn.weapon.reload();
            true
        }
        Kind::ShootAtTrackedLocation {
            location_ref,
            duration,
            time_remaining,
            tracking_speed,
            ignore_aiming,
        } => {
            *location_ref = task.int(0).max(0) as usize;
            *duration = task.float(1);
            *time_remaining = *duration;
            *tracking_speed = task.float(2);
            *ignore_aiming = task.int(3) == 1;
            true
        }
        Kind::MoveTo(m) => m.start(&task, b, ctx),
        Kind::MoveToClearAttackChannel {
            current_time,
            destination,
        } => {
            let Some(enemy) = b.core.enemy.and_then(|e| ctx.view.actor(e)) else {
                return false;
            };
            if b.core.ws_bool(htn::WS_CLEAR_ATTACK_CHANNEL) {
                return true;
            }
            let dir = enemy.loc - ctx.pawn.loc;
            let mut rng = b.core.rng.clone();
            let found = b
                .sensors
                .channel
                .as_mut()
                .and_then(|c| c.calculate_move_to_destination(ctx, dir, &mut rng));
            b.core.rng = rng;
            let Some(d) = found else {
                // (No side step open: a failed one, for the Scrappers' way round.)
                if let Some(c) = b.sensors.channel.as_mut() {
                    c.side_step_failed();
                }
                return false;
            };
            *destination = d;
            let r = ctx.pawn.radius;
            motion::move_to_point(ctx.pawn, d, 1, r);
            motion::set_speed(ctx.pawn, task.int(0) as u8);
            *current_time = 0.0;
            true
        }
        Kind::IdleInAttackChannel => {
            // (It takes none: one with any fails.)
            if task.n != 0 {
                return false;
            }
            b.set_behavior(
                ctx,
                BEHAVIOR_HEAD_TRACKING | BEHAVIOR_ROTATE_TO_FOCUS,
                false,
            );
            true
        }
        Kind::SquadMeleeEnemy {
            melee_type,
            satisfied_squad_command,
        } => {
            *melee_type = super::melee::MELEE_ANY;
            let chance = task.float(0);
            *satisfied_squad_command = task.int(1).max(0) as u8;
            if chance > b.core.rng.frand() {
                *melee_type = super::melee::MELEE_COMBO;
            }
            b.set_behavior(
                ctx,
                BEHAVIOR_ROTATE_TO_FOCUS | BEHAVIOR_HEAD_TRACKING | BEHAVIOR_AIMER,
                false,
            );
            super::melee::start_attack(b, ctx, *melee_type);
            true
        }
        Kind::React {
            params,
            world_state,
            interruptible,
            complete,
        } => {
            *world_state = task.int(1).max(0) as usize;
            // (Htn.GetHighPriorityAnimParameterValue, with no stance.)
            let mut p = bp.anim(task.int(0), stance::NONE);
            p.priority = 1;
            p.override_mode = super::anim::OVERRIDE_IF_HIGHER_OR_EQUAL;
            *params = p;
            b.set_behavior(ctx, 0, false);
            // PlayAnimation.
            b.core.set_ws_bool(*world_state, false);
            ctx.pawn
                .anim
                .enqueue(params.clone(), cb(CB_COMPLETE), cb(CB_COMPLETE));
            *complete = false;
            if task.n > 2 {
                *interruptible = task.int(2) == 1;
            }
            // (Its fourth parameter, CanPlayHitReaction, unless 1: none while it
            // plays.)
            if task.n < 3 || task.int(3) != 1 {
                ctx.pawn.can_play_hit_reaction = false;
            }
            true
        }
        Kind::ReactForDuration {
            current_time,
            duration,
        } => {
            b.set_behavior(ctx, 0, false);
            motion::stop(ctx.pawn);
            let mut p = bp.anim(task.int(0), stance::NONE);
            p.priority = 1;
            p.override_mode = super::anim::OVERRIDE_IF_HIGHER_OR_EQUAL;
            p.root_motion = super::anim::RMM_IGNORE;
            p.looping = true;
            p.blend_in = 0.5;
            p.blend_out = 0.5;
            ctx.pawn.anim.enqueue(p, None, None);
            *duration = task.int(1) as f32;
            *current_time = 0.0;
            true
        }
        Kind::StopMove => {
            b.remove_behavior(ctx, BEHAVIOR_ALLOW_MOTION);
            motion::stop(ctx.pawn);
            true
        }
        Kind::ChangeRobotForm {
            target_form,
            look_at,
            succeeded,
        } => {
            *target_form = task.int(0);
            *look_at = task.int(2).max(0) as usize;
            *succeeded = false;
            b.set_behavior(ctx, BEHAVIOR_AIMER | BEHAVIOR_HEAD_TRACKING, false);
            super::guard::prepare_to_activate_form(b, ctx.pawn, *target_form);
            // (ResolveAnimationParameters: its blueprint's, as its own priority
            // has it.)
            let params = bp.anim(task.int(1), stance::NONE);
            ctx.pawn
                .anim
                .enqueue(params, cb(CB_COMPLETE), cb(CB_COMPLETE));
            true
        }
        Kind::ChooseNearbyLocation => {
            choose_nearby_location(b, ctx, task.int(0) as f32);
            true
        }
        Kind::AreaOfEffectAttack { attack, complete } => {
            b.set_behavior(ctx, 0, false);
            let Asset::AreaOfEffect(a) = bp.asset(task.int(0)) else {
                op.fail_context = "No Area Of Effect Blueprint";
                return false;
            };
            *complete = false;
            ctx.pawn
                .anim
                .enqueue(a.attack_animation.clone(), cb(CB_COMPLETE), cb(CB_COMPLETE));
            *attack = Some(a);
            true
        }
        Kind::GuardScanPatrolPoint {
            observe,
            yaw,
            half_cone,
            scan_duration,
            index,
            animations,
            ..
        } => {
            // (Its patrol point: the route's at its index; Escalation's AIs have
            // no patrols. The demo's route keeps no facing: the way it faces as it
            // gets there.)
            let store = &b.sensors.patrol;
            let Some(&(_, observe_for)) = store.route.get(store.index.max(0) as usize) else {
                return false;
            };
            *observe = observe_for;
            *yaw = ctx.pawn.yaw;
            *index = 0;
            animations.clear();
            b.set_behavior(ctx, BEHAVIOR_ROTATE_TO_FOCUS, false);
            ctx.pawn.set_focal_rotation(*yaw);
            // ShouldScan (no custom animations of its own): its look swept.
            b.add_behavior(ctx, BEHAVIOR_HEAD_TRACKING | BEHAVIOR_AIMER);
            *half_cone = task.int(0) as f32 * 0.5;
            *scan_duration = task.int(1) as f32;
            true
        }
        Kind::PlayTellFacingFocus { complete } => {
            // (Htn.GetHighPriorityAnimParameterValue, with no stance.)
            let mut p = bp.anim(task.int(0), stance::NONE);
            p.priority = 1;
            p.override_mode = super::anim::OVERRIDE_IF_HIGHER_OR_EQUAL;
            ctx.pawn.anim.enqueue(p, cb(CB_COMPLETE), cb(CB_COMPLETE));
            *complete = false;
            b.set_behavior(ctx, BEHAVIOR_ROTATE_TO_FOCUS, false);
            // (A biped motion controller it would disable: the Leaper's is a
            // locomotion controller.)
            let at = b.core.fetch(super::brain::LR_CONFIRMED_ENEMY);
            ctx.pawn.set_focal_point(at);
            ctx.pawn.set_look_at_point(at);
            true
        }
        Kind::StopRotating => {
            b.set_behavior(ctx, BEHAVIOR_ROTATE_TO_FOCUS, false);
            let ahead = ctx.pawn.loc + ctx.pawn.forward() * 2000.0;
            ctx.pawn.set_focal_point(ahead);
            ctx.pawn.set_look_at_point(ahead);
            true
        }
        Kind::ClaimLeapSmash => {
            let mut writes = std::mem::take(&mut ctx.shared.writes);
            ctx.shared
                .squad
                .claim_leap_smash(&mut b.core.rng, &mut writes);
            ctx.shared.writes = writes;
            true
        }
        Kind::MeleeLunge(l) => l.start(b, ctx, &task),
        Kind::AnimatedSkid(s) => s.start(b, ctx, &task, cb(CB_COMPLETE)),
        Kind::LeapSmash(l) => l.start(b, ctx, &task),
        Kind::SpawnDrones(s) => s.start(b, ctx, &task),
        Kind::ChooseNewSniperPoint => super::sniper::choose_point(b, ctx),
        Kind::ChooseNewSniperPointTowardsGoal => super::sniper::choose_point_towards_goal(b, ctx),
        Kind::ChooseNewSniperSearchPoint => super::sniper::choose_search_point(b, ctx),
        Kind::ChooseNewSniperAttackPoint => super::sniper::choose_attack_point(b, ctx, &task),
        Kind::FlyToSniperPoint(f) => f.start(b, ctx, &task),
        Kind::AttachToWall(a) => a.start(b, ctx, &task, serial),
        Kind::DetachFromWall(d) => d.start(b, ctx, &task),
        Kind::CrawlOnSniperPoint(c) => c.start(b, ctx, &task),
        Kind::DroneMoveTo => super::drones::drone_move_to(b, ctx, &task),
        Kind::HoverAtLocation(h) => h.start(b, ctx, &task),
        Kind::Explode => super::drones::explode(b, ctx, &task),
        Kind::ChooseNewDefendActorLocation => {
            super::drones::choose_new_defend_actor_location(b, ctx)
        }
        Kind::PlaneHoverTo => super::jet::plane_hover_to(b, ctx, &task),
        Kind::ChooseNewAirAttackPoint => super::jet::choose_new_air_attack_point(b, ctx),
        Kind::ChooseNewAerialPursuitPoint => super::jet::choose_new_aerial_pursuit_point(b, ctx),
        Kind::ChooseNewFlyingCirclingPoint => {
            super::jet::choose_new_flying_circling_point(b, ctx, &task)
        }
        Kind::ChooseJetRobotAttackPoint => super::jet::choose_jet_robot_attack_point(b, ctx, &task),
        Kind::ChooseNewFlyingInvestigateLocation => {
            super::jet::choose_new_flying_investigate_location(b, ctx, &task)
        }
        Kind::HoverToFace(h) => h.start(b, ctx, &task),
        Kind::PlaneDash(d) => d.start(b, ctx, &task),
        Kind::Transform(t) => t.start(b, ctx, &task),
        Kind::Jump(j) => j.start(b, ctx, &task),
        Kind::AnimatedAttack {
            params,
            additive,
            complete,
        } => {
            // (Its animation the additive animator's for AnimationType 1, its
            // animator's else; high priority, overriding its equal.)
            *additive = task.int(0) == 1;
            b.set_behavior(ctx, BEHAVIOR_AIMER | BEHAVIOR_ROTATE_TO_FOCUS, false);
            let mut p = bp.anim(task.int(1), ctx.pawn.current_stance());
            p.priority = 1;
            p.override_mode = super::anim::OVERRIDE_IF_HIGHER_OR_EQUAL;
            *params = p.clone();
            *complete = false;
            if *additive {
                ctx.pawn
                    .anim
                    .enqueue_additive(p, cb(CB_COMPLETE), cb(CB_COMPLETE));
            } else {
                ctx.pawn.anim.enqueue(p, cb(CB_COMPLETE), cb(CB_COMPLETE));
            }
            true
        }
        Kind::ShootAtEnemyHeadOn(d) => d.start(b, ctx, &task, false),
        Kind::ShootAtEnemyFireForEffect(d) => d.start(b, ctx, &task, true),
        Kind::Unknown(class) => {
            warn!("AI operator {class} isn't ported: its task fails");
            false
        }
    }
}

/// TnChooseNearbyLocationOperator.OnStart: from the cell it is anchored on, the
/// first ten cells reached across their edges (the next always the one most
/// ahead of it: GetDirectionWeight), the best by how long since its squad looked
/// there (GetAgeWeight: the least of the cell's and its neighbours', 10 to 30 s),
/// how far ahead of it (GetDirectionWeight) and how near `distance` away
/// (GetDistanceWeight: full within 5 m of it, less down to 3 m or out to 75 m),
/// not too near it (3 m: ShouldExcludeCell) or a wall (an edge with no cell
/// across within 3 m: IsCellTooCloseToGeometry), not through a blocked cell or one
/// a buddy stands still in (IsCellOccupied). It becomes the suspected enemy's
/// place (UpdateSuspectedLocation, LR_SuspectedEnemy) and is marked searched.
fn choose_nearby_location(b: &mut Brain, ctx: &mut Ctx, distance: f32) {
    let Some(nav) = ctx.view.nav else { return };
    let p = &*ctx.pawn;
    let current = p.loc;
    let desired = p.forward();
    let ideal = (distance - 0.5 * 1000.0, distance + 0.5 * 1000.0);
    let Some(start) = nav.cell_at(ue::from_ue(p.feet())).or(p.anchor) else {
        return;
    };
    let now = ctx.view.now;
    let loc = |c: u32| ue::to_ue(nav.points[c as usize].at);
    let direction_weight = |c: u32| 0.5 * (ue::normal(loc(c) - current).dot(desired) + 1.0);
    let since_searched = |c: u32| {
        let at = loc(c);
        b.sensors.search.as_ref().map_or(now, |s| {
            now - s
                .searched
                .iter()
                .find(|(q, _)| *q == at)
                .map_or(0.0, |(_, t)| *t)
        })
    };
    let age_weight = |c: u32| {
        let mut age = since_searched(c);
        for n in nav.cells[c as usize].neighbours.into_iter().flatten() {
            age = age.min(since_searched(n));
        }
        ue::clamp_pct_by_range(age, 10.0, 30.0)
    };
    let distance_weight = |c: u32| {
        let d = (loc(c) - current).length();
        if d >= ideal.0 && d <= ideal.1 {
            1.0
        } else if d < ideal.0 {
            ue::clamp_pct_by_range(d, 300.0, ideal.0)
        } else {
            1.0 - ue::clamp_pct_by_range(d, ideal.1, 7500.0)
        }
    };
    let too_close_to_geometry = |c: u32| {
        let at = loc(c);
        (0..3).any(|k| {
            let e = nav.edge(c, k);
            if e.neighbour.is_some() {
                return false;
            }
            let near = closest_point_on_segment(ue::to_ue(e.start), ue::to_ue(e.end), at);
            (at - near).length_squared() < 300.0 * 300.0
        })
    };
    let exclude = |c: u32| {
        ue::range_point_to_point(p.loc, p.radius, loc(c), 0.0, p.half_height, 0.0) < 300.0
            || too_close_to_geometry(c)
    };
    let occupied = |c: u32| {
        ctx.view.ais.iter().any(|a| {
            a.alive
                && a.entity != p.entity
                && a.team == b.core.team
                && nav.inside2d(c, ue::from_ue(a.loc), 0.0)
                && a.vel.length() < 50.0
        })
    };
    // OpenCell: in its place among the open ones, the most ahead first.
    let mut open: Vec<(u32, f32)> = Vec::new();
    let open_cell = |open: &mut Vec<(u32, f32)>, c: u32| {
        let w = direction_weight(c);
        let i = open
            .iter()
            .position(|(_, ow)| w > *ow)
            .unwrap_or(open.len());
        open.insert(i, (c, w));
    };
    let mut closed: Vec<u32> = Vec::new();
    open_cell(&mut open, start);
    let (mut best, mut best_weight) = (None, 0.0);
    for _ in 0..10 {
        if open.is_empty() {
            break;
        }
        let (cell, _) = open.remove(0);
        closed.push(cell);
        let weight = age_weight(cell) + direction_weight(cell) + distance_weight(cell);
        if weight > best_weight && !exclude(cell) {
            best = Some(cell);
            best_weight = weight;
        }
        for n in nav.cells[cell as usize].neighbours.into_iter().flatten() {
            if nav.points[n as usize].blocked
                || closed.contains(&n)
                || open.iter().any(|(c, _)| *c == n)
                || occupied(n)
            {
                continue;
            }
            open_cell(&mut open, n);
        }
    }
    let Some(cell) = best else { return };
    let at = loc(cell);
    let view = ctx.view;
    b.sensors.threats.update_suspected(view, at, at);
    b.core
        .store(super::brain::LR_SUSPECTED_ENEMY, at, Vec3::ZERO);
    // TnSquadSearchDataStore.PointSearched.
    if let Some(s) = b.sensors.search.as_mut() {
        match s.searched.iter_mut().find(|(q, _)| *q == at) {
            Some(q) => q.1 = now,
            None => s.searched.push((at, now)),
        }
    }
}

/// Core.Math.ClosestPointToLineSegment.
fn closest_point_on_segment(a: Vec3, b: Vec3, p: Vec3) -> Vec3 {
    let d = b - a;
    let len = d.length_squared();
    if len < 1e-8 {
        return a;
    }
    a + d * ((p - a).dot(d) / len).clamp(0.0, 1.0)
}

/// A pawn's GetViewLocation: a player's eyes, an actor's middle.
pub(crate) fn enemy_view_location(ctx: &Ctx, e: Entity) -> Vec3 {
    if let Some(p) = ctx.view.player(e) {
        return p.eyes;
    }
    ctx.view.actor(e).map_or(ctx.pawn.loc, |a| a.loc)
}

/// Core.Math.GetPointAtEndOfArc: `start` turned about `centre` (about the up axis)
/// through an arc of `length` uu.
fn point_at_end_of_arc(start: Vec3, centre: Vec3, length: f32) -> Vec3 {
    let c = start - centre;
    let angle = length / ue::vsize2d(c).max(1e-3);
    centre + ue::rotate_yaw(c, angle * 65536.0 / std::f32::consts::TAU)
}

/// TnSideStepOperator.GetTraversableDistance: how far the pawn could step round
/// its enemy along the arc (`step` uu: to its left when positive), tried in two
/// halves.
fn traversable_distance(ctx: &Ctx, centre: Vec3, start: Vec3, step: f32) -> f32 {
    let half = step * 0.5;
    let p = &*ctx.pawn;
    let end = point_at_end_of_arc(start, centre, half);
    let (ok, result) = super::sensors::can_walk_line_from(p, ctx.view, start, end);
    if !ok {
        return ue::vsize2d(result - start) - p.radius;
    }
    let start = end;
    let end = point_at_end_of_arc(start, centre, half);
    let (_, result) = super::sensors::can_walk_line_from(p, ctx.view, start, end);
    ue::vsize2d(result - start) + half.abs() - p.radius
}

/// TnClaimSquadActionOperator.OnStart's ClaimSquadAction.
fn claim_squad_action(b: &mut Brain, ctx: &mut Ctx, action: usize) -> bool {
    let suspected = b.core.fetch(super::brain::LR_SUSPECTED_ENEMY);
    let enemy_half_height = b
        .core
        .enemy
        .and_then(|e| ctx.view.actor(e))
        .map(|a| a.half_height);
    let mut info = ClaimInfo {
        member: MemberInfo {
            entity: b.core.entity,
            ai_type: b.core.ai_type,
        },
        suspected_enemy: suspected,
        enemy_half_height,
        eyes: ctx.pawn.eyes(),
        loc: ctx.pawn.loc,
        view: ctx.view,
        grenade_aim_point: None,
    };
    let now = ctx.view.now;
    let mut writes = std::mem::take(&mut ctx.shared.writes);
    let ok = ctx.shared.squad.claim(&mut info, action, now, &mut writes);
    ctx.shared.writes = writes;
    if let Some(p) = info.grenade_aim_point {
        b.core
            .store(super::brain::LR_GRENADE_AIM_POINT, p, Vec3::ZERO);
    }
    ok
}

/// TnLookForEnemiesOperator.GetLocation.
fn look_for_location(b: &Brain, ctx: &Ctx, r: usize) -> Vec3 {
    if r != 0 {
        b.core.fetch(r)
    } else {
        ctx.pawn.loc + ctx.pawn.forward() * 2000.0
    }
}

/// TnShootAtLocationOperator.UpdateLocation.
fn update_shoot_location(b: &Brain, ctx: &mut Ctx, r: usize) {
    let loc = b.core.fetch(r);
    ctx.pawn.set_look_at_point(loc);
    if ctx.pawn.rotate_toward_focus {
        ctx.pawn.set_focal_point(loc);
    }
}

/// TnShootNearLocationOperator.UpdateLookAtRotation: a sweep up and down and
/// side to side across the place.
fn update_look_near(
    ctx: &mut Ctx,
    direction: (f32, f32),
    pct: f32,
    pitch_half: f32,
    yaw_half: f32,
) {
    let pitch = -(pct * std::f32::consts::PI).sin() * pitch_half;
    let yaw = (pct * std::f32::consts::TAU).sin() * yaw_half;
    ctx.pawn.set_look_at_rotation(
        direction.0 + pitch * ue::ROT_PER_DEG,
        direction.1 + yaw * ue::ROT_PER_DEG,
    );
}

/// TnPeekFromCoverOperator.GetFocusLocation.
fn peek_focus(b: &Brain, t: &TacticalNode, r: usize) -> Vec3 {
    if r != 0 {
        b.core.fetch(r)
    } else {
        t.location + tactical_forward(t) * 1000.0
    }
}

/// TnPeekFromCoverOperator.GetDestination: a corner's step out past its edge.
fn peek_destination(radius: f32, t: &TacticalNode) -> Vec3 {
    let offset = super::weapon::from_local(Vec3::new(0.0, radius, 0.0), 0.0, t.yaw);
    match t.cover {
        Cover::LeftCorner | Cover::LowLeftCorner => t.location + offset,
        Cover::RightCorner | Cover::LowRightCorner => t.location - offset,
        _ => t.location,
    }
}

/// TnMoveInRandomDirectionOperator.CalculateMove_Destination.
fn random_move_destination(b: &mut Brain, ctx: &Ctx, distance: f32) -> Option<Vec3> {
    let a = b.core.rng.frand() * std::f32::consts::TAU;
    let dir = Vec3::new(a.cos(), a.sin(), 0.0) * distance;
    let start = ctx.pawn.feet();
    let (ok, end) = super::sensors::can_walk_line(ctx.pawn, ctx.view, start + dir, true);
    if ok || ue::vsize2d(end - start) >= 100.0 {
        return Some(end);
    }
    let (ok, end) = super::sensors::can_walk_line(ctx.pawn, ctx.view, start - dir, true);
    if ok || ue::vsize2d(end - start) >= 100.0 {
        return Some(end);
    }
    None
}

/// TnStrafeAndShootOperator.GetCenterAndCorners: the point `range` straight
/// back from the enemy through the pawn, and the two `side` degrees either way.
fn centre_and_corners(
    pawn_start: Vec3,
    enemy_start: Vec3,
    side: f32,
    range: f32,
) -> (Vec3, Vec3, Vec3) {
    let away = ue::normal((pawn_start - enemy_start) * Vec3::new(1.0, 1.0, 0.0));
    let yaw = ue::yaw_of(away);
    let centre = enemy_start + away * range;
    let right = enemy_start + ue::yaw_vec(yaw - side * ue::ROT_PER_DEG) * range;
    let left = enemy_start + ue::yaw_vec(yaw + side * ue::ROT_PER_DEG) * range;
    (centre, right, left)
}

/// TnStrafeAndShootOperator.CalculateMoveDestination.
#[allow(clippy::too_many_arguments)]
fn strafe_destination(
    b: &mut Brain,
    ctx: &Ctx,
    centre: Vec3,
    right: Vec3,
    left: Vec3,
    enemy_start: Vec3,
    pawn_start: Vec3,
    min_range: f32,
    max_range: f32,
) -> Vec3 {
    let forward = b.core.rng.frand().min(1.0 - min_range / max_range);
    let side_distance = b.core.rng.frand().min(1.0 - forward);
    let side = if b.core.rng.frand() < 0.5 {
        right
    } else {
        left
    };
    let mut target = (side - centre) * side_distance;
    target += (enemy_start - centre) * forward;
    target += centre;
    let (ok, end) = super::sensors::can_walk_line(ctx.pawn, ctx.view, target, true);
    if !ok {
        target = end + ue::normal(pawn_start - end) * ctx.pawn.radius;
    }
    target
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

/// HtnOperator.Update's OnUpdate.
pub fn update(op: &mut Op, b: &mut Brain, ctx: &mut Ctx) -> Status {
    let dt = ctx.dt;
    let serial = op.serial;
    let cb = |k: u8| {
        Some(Callback {
            owner: serial,
            kind: k,
        })
    };
    match &mut op.kind {
        Kind::Idle | Kind::NavigatingIdle => Status::Running,
        Kind::IdleForDuration { time_remaining } => {
            *time_remaining -= dt;
            if *time_remaining <= 0.0 {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::LookAt {
            time_remaining,
            location,
        } => {
            let at = b.core.fetch(*location);
            ctx.pawn.set_look_at_point(at);
            *time_remaining -= dt;
            if *time_remaining > 0.0 {
                Status::Running
            } else {
                Status::Succeeded
            }
        }
        Kind::TurnToFocus {
            use_look_direction,
            min_cos,
            focus,
        } => {
            let at = b.core.fetch(*focus);
            ctx.pawn.set_focal_point(at);
            ctx.pawn.set_look_at_point(at);
            let look = if *use_look_direction {
                ctx.pawn.look_direction()
            } else {
                ctx.pawn.forward()
            };
            let cos = ue::dot2d(ue::normal2d(look), ue::normal2d(at - ctx.pawn.loc));
            if cos > *min_cos && !ctx.pawn.motion.is_turning() {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::LookForEnemies {
            location_ref,
            location,
            should_shoot,
            half_cone,
            duration,
            direction,
            time_remaining,
        } => {
            let now_loc = look_for_location(b, ctx, *location_ref);
            if (now_loc - *location).length() >= 500.0 {
                return Status::Failed;
            }
            *time_remaining -= dt;
            let angle =
                (*time_remaining / duration.max(1e-3) * 2.0 * std::f32::consts::PI).max(0.0);
            let yaw = ue::yaw_of(*direction) + angle.sin() * *half_cone * ue::ROT_PER_DEG;
            let pitch = ue::pitch_of(*direction);
            let aim = ctx.pawn.loc + ue::vec_of(pitch, yaw) * 2000.0;
            ctx.pawn.set_look_at_point(aim);
            ctx.pawn.set_focal_point(now_loc);
            let shoot = if *should_shoot && b.core.ws_bool(htn::WS_WEAPON_HAS_CLEAR_LINE_OF_FIRE) {
                BEHAVIOR_SHOOTER
            } else {
                0
            };
            b.set_behavior(
                ctx,
                BEHAVIOR_HEAD_TRACKING | BEHAVIOR_ROTATE_TO_FOCUS | BEHAVIOR_AIMER | shoot,
                false,
            );
            if *time_remaining > 0.0 {
                Status::Running
            } else {
                Status::Succeeded
            }
        }
        Kind::ScanPatrolPoint {
            time_left,
            index,
            animations,
            ..
        } => {
            *time_left -= dt;
            if *time_left <= 0.0 && *index >= animations.len() {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::ShootAtEnemy {
            time_remaining,
            lead_target,
            ..
        } => {
            *time_remaining -= dt;
            if *time_remaining <= 0.0 {
                return Status::Succeeded;
            }
            let Some(enemy) = b.core.enemy else {
                op.fail_context = "No Enemy";
                return Status::Failed;
            };
            if !ctx.pawn.weapon.is_equipped() && !ctx.pawn.weapon.is_equipping() {
                op.fail_context = "No Weapon";
                return Status::Failed;
            }
            if ctx.view.actor(enemy).is_none_or(|a| a.dead) {
                return Status::Succeeded;
            }
            if shooting_desired(b, ctx) {
                b.add_behavior(ctx, BEHAVIOR_SHOOTER);
            } else {
                b.remove_behavior(ctx, BEHAVIOR_SHOOTER);
            }
            let mut aim = find_aim_location_on_target(b, ctx, enemy);
            let speed = ctx.pawn.weapon.data.projectile_speed;
            if *lead_target
                && speed > 0.0
                && let Some(a) = ctx.view.actor(enemy)
            {
                aim = intercept_position(
                    aim,
                    a.vel,
                    super::sensors::muzzle_location(ctx.pawn),
                    speed,
                );
            }
            ctx.pawn.set_focal_point(aim);
            ctx.pawn.set_look_at_point(aim);
            Status::Running
        }
        Kind::ShootAtLocation {
            time_remaining,
            location,
            ..
        } => {
            *time_remaining -= dt;
            if *time_remaining <= 0.0 {
                return Status::Succeeded;
            }
            update_shoot_location(b, ctx, *location);
            Status::Running
        }
        Kind::ShootNearLocation {
            location,
            direction,
            duration,
            time_remaining,
            pitch_half,
            yaw_half,
        } => {
            let at = b.core.fetch(*location);
            let desired = ue::normal(at - ctx.pawn.loc);
            if ue::dot2d(desired, ue::vec_of(direction.0, direction.1)) < 0.0 {
                op.fail_context = "Location Moved";
                return Status::Failed;
            }
            *time_remaining -= dt;
            update_look_near(
                ctx,
                *direction,
                *time_remaining / duration.max(1e-3),
                *pitch_half,
                *yaw_half,
            );
            if *time_remaining > 0.0 {
                Status::Running
            } else {
                Status::Succeeded
            }
        }
        Kind::PlayAnimation {
            complete, focus, ..
        } => {
            if *focus != 0 {
                let at = b.core.fetch(*focus);
                ctx.pawn.set_focal_point(at);
                ctx.pawn.set_look_at_point(at);
            }
            if *complete {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::PlayHitReaction { complete } | Kind::HitReaction { complete, .. } => {
            if *complete {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::Knockedback {
            direction,
            landing,
            complete,
            playing,
            land,
            take_off,
            ..
        } => {
            if *complete {
                return Status::Succeeded;
            }
            if ctx.pawn.physics == Physics::Walking
                && !ctx.pawn.anim.is_playing_named(&take_off.name)
                && !*landing
            {
                *landing = true;
                *playing = land.clone();
                ctx.pawn
                    .anim
                    .enqueue(land.clone(), cb(CB_LAND_COMPLETE), cb(CB_LAND_COMPLETE));
            }
            let yaw = ue::yaw_of(*direction);
            ctx.pawn.set_focal_rotation(yaw);
            ctx.pawn.set_look_at_rotation(0.0, yaw);
            Status::Running
        }
        Kind::PlayPositionRelativeAnimation {
            found_out,
            complete,
            opening_complete,
            playing_out,
            ..
        } => {
            if *opening_complete && !*playing_out {
                ctx.pawn
                    .anim
                    .enqueue(found_out.clone(), cb(CB_COMPLETE), cb(CB_COMPLETE));
                *opening_complete = false;
                *playing_out = true;
            }
            if *complete {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::MoveInRandomDirection {
            current_time,
            destination,
        } => {
            *current_time += dt;
            if *current_time > 2.0 {
                return Status::Failed;
            }
            ctx.pawn.set_focal_point(*destination);
            ctx.pawn.set_look_at_point(*destination);
            if ((ctx.pawn.feet() - *destination) * Vec3::new(1.0, 1.0, 0.0)).length_squared()
                < 300.0 * 300.0
            {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::EnterCover { time_left, .. } => {
            *time_left -= dt;
            if motion::reached_destination(ctx.pawn) {
                motion::stop(ctx.pawn);
            }
            if *time_left > 0.0 {
                Status::Running
            } else {
                Status::Succeeded
            }
        }
        Kind::PeekFromCover {
            current_time,
            tactical,
            focus,
        } => {
            *current_time += dt;
            let Some(t) = tactical else {
                return Status::Succeeded;
            };
            let f = peek_focus(b, t, *focus);
            match t.cover {
                Cover::Low | Cover::None | Cover::Ledge => ctx.pawn.set_focal_point(f),
                _ => ctx.pawn.set_focal_rotation(t.yaw),
            }
            ctx.pawn.set_look_at_point(f);
            let reached = motion::reached_destination(ctx.pawn);
            if reached {
                motion::stop(ctx.pawn);
            }
            if (*current_time > 0.5 && reached) || *current_time > 1.0 {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::MeleeEnemy {
            succeed_when_complete,
            melee_type,
            ..
        } => {
            let enemy = b.core.enemy.and_then(|e| ctx.view.actor(e));
            if enemy.is_none() || !super::melee::is_meleeing(ctx.pawn, *melee_type) {
                return if *succeed_when_complete {
                    Status::Succeeded
                } else {
                    Status::Failed
                };
            }
            let a = enemy.unwrap();
            if a.dead {
                return Status::Succeeded;
            }
            ctx.pawn.set_focal_point(a.loc);
            ctx.pawn.set_look_at_point(a.loc);
            Status::Running
        }
        Kind::ThrowGrenade { status, .. } => match status {
            3 => Status::Succeeded,
            2 => Status::Failed,
            _ => Status::Running,
        },
        Kind::StrafeAndShoot {
            relative_ref,
            current_time,
            destination,
            ..
        } => {
            *current_time += dt;
            if *current_time > 2.0 && !ctx.pawn.motion.is_in_motion() {
                return Status::Succeeded;
            }
            if shooting_desired(b, ctx) {
                b.add_behavior(ctx, BEHAVIOR_SHOOTER);
            } else {
                b.remove_behavior(ctx, BEHAVIOR_SHOOTER);
            }
            let focus = b.core.fetch(*relative_ref);
            ctx.pawn.set_focal_point(focus);
            ctx.pawn.set_look_at_point(focus);
            if ue::vsize_sq2d(ctx.pawn.loc - *destination) < 50.0 * 50.0 {
                motion::stop(ctx.pawn);
            }
            Status::Running
        }
        Kind::MoveTo(m) => m.update(b, ctx),
        Kind::Behavior => Status::Succeeded,
        Kind::PlayReaction { time_remaining } => {
            *time_remaining -= dt;
            if *time_remaining < 0.0 {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::PlayInjuredReaction { playing, .. } => {
            if *playing {
                Status::Running
            } else {
                Status::Succeeded
            }
        }
        Kind::ReloadWeapon { .. } => {
            if b.core.behavior & BEHAVIOR_AIMER != 0 {
                let at = b.core.fetch(super::brain::LR_CONFIRMED_ENEMY);
                ctx.pawn.set_look_at_point(at);
            }
            if ctx.pawn.weapon.reloading() {
                Status::Running
            } else {
                Status::Succeeded
            }
        }
        Kind::ShootAtTrackedLocation {
            location_ref,
            duration,
            time_remaining,
            tracking_speed,
            ignore_aiming,
        } => {
            // HasAnEnemy: one not dying or dead.
            let has_enemy = b
                .core
                .enemy
                .and_then(|e| ctx.view.actor(e))
                .is_some_and(|a| !a.dead);
            if *location_ref == super::brain::LR_CONFIRMED_ENEMY && !has_enemy {
                return Status::Failed;
            }
            // UpdateLocation: the aim dragged towards where it wants it.
            let desired = match b.core.enemy {
                Some(e)
                    if *location_ref == super::brain::LR_CONFIRMED_ENEMY
                        && b.core.ws_bool(htn::WS_ENEMY_VISIBLE) =>
                {
                    find_aim_location_on_target(b, ctx, e)
                }
                _ => b.core.fetch(*location_ref),
            };
            let current = super::weapon::aim_location(ctx.pawn);
            let at = ue::vinterp_to(current, desired, dt, *tracking_speed);
            ctx.pawn.set_look_at_point(at);
            ctx.pawn.set_focal_point(at);
            if tracked_shooting_desired(*ignore_aiming, ctx) {
                b.add_behavior(ctx, BEHAVIOR_SHOOTER);
            } else {
                b.remove_behavior(ctx, BEHAVIOR_SHOOTER);
            }
            if *duration <= 0.0 {
                return Status::Running;
            }
            *time_remaining -= dt;
            if *time_remaining > 0.0 {
                Status::Running
            } else {
                Status::Succeeded
            }
        }
        Kind::MoveToClearAttackChannel {
            current_time,
            destination,
        } => {
            if b.core.ws_bool(htn::WS_CLEAR_ATTACK_CHANNEL) {
                return Status::Succeeded;
            }
            // (A side step that fails, by time or arriving without a clear
            // channel, counts toward the Scrappers' way round: sensors.rs's
            // WayRound.)
            *current_time += dt;
            if *current_time > 2.5 {
                if let Some(c) = b.sensors.channel.as_mut() {
                    c.side_step_failed();
                }
                return Status::Failed;
            }
            let Some(enemy) = b.core.enemy.and_then(|e| ctx.view.actor(e)) else {
                return Status::Failed;
            };
            ctx.pawn.set_focal_point(enemy.loc);
            ctx.pawn.set_look_at_point(enemy.loc);
            // (As the script has it: arriving without a clear channel fails.)
            if ue::vsize_sq2d(ctx.pawn.feet() - *destination) < 50.0 * 50.0 {
                if let Some(c) = b.sensors.channel.as_mut() {
                    c.side_step_failed();
                }
                Status::Failed
            } else {
                Status::Running
            }
        }
        Kind::IdleInAttackChannel => {
            if b.core.ws(htn::WS_SQUAD_COMMAND) == 1 {
                return Status::Succeeded;
            }
            let Some(enemy) = b.core.enemy.and_then(|e| ctx.view.actor(e)) else {
                return Status::Failed;
            };
            ctx.pawn.set_focal_point(enemy.loc);
            ctx.pawn.set_look_at_point(enemy.loc);
            Status::Running
        }
        Kind::SquadMeleeEnemy { melee_type, .. } => {
            // Its plain attack struck his raised shield and hurt him not: it is
            // frustrated (WS_Frustrated).
            let shielded = b
                .core
                .enemy
                .and_then(|e| ctx.view.player(e))
                .is_some_and(|pl| pl.shielded);
            let m = &ctx.pawn.melee;
            if shielded && *melee_type == super::melee::MELEE_ANY && m.hit_pawn && !m.hurt_pawn {
                b.core.set_ws_bool(htn::WS_FRUSTRATED, true);
                return Status::Failed;
            }
            let enemy = b.core.enemy.and_then(|e| ctx.view.actor(e));
            let Some(a) = enemy else {
                return Status::Failed;
            };
            if !super::melee::is_meleeing(ctx.pawn, *melee_type) {
                return Status::Failed;
            }
            if a.dead {
                return Status::Succeeded;
            }
            ctx.pawn.set_focal_point(a.loc);
            ctx.pawn.set_look_at_point(a.loc);
            Status::Running
        }
        Kind::React {
            params,
            world_state,
            complete,
            ..
        } => {
            if b.core.ws_bool(*world_state) {
                b.core.set_ws_bool(*world_state, false);
                ctx.pawn
                    .anim
                    .enqueue(params.clone(), cb(CB_COMPLETE), cb(CB_COMPLETE));
                *complete = false;
            }
            if *complete {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::ReactForDuration {
            current_time,
            duration,
        } => {
            *current_time += dt;
            if *current_time >= *duration {
                ctx.pawn.anim.stop(0.0, None);
                return Status::Succeeded;
            }
            Status::Running
        }
        Kind::TriggerAbility {
            complete_on_activate,
        } => {
            if *complete_on_activate || !ctx.pawn.cloak.active {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::ScanSniperEnemy {
            last_enemy,
            scan_duration,
            scan_time,
            hold_lock_on_duration,
            lock_on_duration,
            lock_on_time,
        } => {
            let has_an_enemy = b.core.enemy.is_some() && b.core.ws_bool(htn::WS_ENEMY_VISIBLE);
            if b.core.enemy != *last_enemy {
                *lock_on_time = 0.0;
                *scan_time = 0.0;
                *last_enemy = b.core.enemy;
            }
            *scan_time += dt;
            if *scan_time >= *scan_duration || *lock_on_time >= *lock_on_duration {
                *lock_on_time += dt;
            } else {
                // (kEnemyMovementSpeed: it locks on while its enemy holds under
                // 250 uu/s, and loses it while it moves.)
                let still = b
                    .core
                    .enemy
                    .and_then(|e| ctx.view.actor(e))
                    .is_some_and(|a| a.vel.length() <= 250.0);
                let delta = if has_an_enemy && still { dt } else { -dt };
                *lock_on_time = (*lock_on_time + delta).max(0.0);
            }
            if *lock_on_time < *lock_on_duration {
                if has_an_enemy && let Some(e) = b.core.enemy {
                    let at = enemy_view_location(ctx, e);
                    ctx.pawn.laser.target = at;
                    b.core.store(super::brain::LR_ATTACK_FOCUS, at, Vec3::ZERO);
                    ctx.pawn.set_look_at_point(at);
                }
                ctx.pawn.laser.spread =
                    (1.0 - *lock_on_time / lock_on_duration.max(1e-3)).clamp(0.0, 1.0);
            } else {
                ctx.pawn.laser.spread = 0.0;
            }
            if *lock_on_time >= *lock_on_duration + *hold_lock_on_duration {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::EquipWeapon {
            finish_immediately,
            aim_location,
        } => {
            if *aim_location != 0 {
                let at = b.core.fetch(*aim_location);
                ctx.pawn.set_look_at_point(at);
            }
            if *finish_immediately || super::weapon::weapon_equipped(ctx.pawn) {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::UnequipWeapon { finish_immediately } => {
            if *finish_immediately || !ctx.pawn.weapon.equipped {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::AnimatedShootingAttack {
            complete, focus, ..
        } => {
            if *focus != 0 {
                let at = b.core.fetch(*focus);
                ctx.pawn.set_focal_point(at);
                ctx.pawn.set_look_at_point(at);
            }
            if *complete {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::SideStep { complete, .. } => {
            let Some(enemy) = b.core.enemy.filter(|e| ctx.view.actor(*e).is_some()) else {
                return Status::Succeeded;
            };
            if b.core.ws_bool(htn::WS_WEAPON_HAS_CLEAR_LINE_OF_FIRE) {
                b.add_behavior(ctx, BEHAVIOR_SHOOTER);
            } else {
                b.remove_behavior(ctx, BEHAVIOR_SHOOTER);
            }
            let aim = find_aim_location_on_target(b, ctx, enemy);
            ctx.pawn.set_focal_point(aim);
            ctx.pawn.set_look_at_point(aim);
            if *complete {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::MoveAwayFromEnemy {
            current_time,
            destination,
            enemy,
        } => {
            *current_time += dt;
            if *current_time > 2.0 {
                return Status::Failed;
            }
            let Some(a) = enemy.and_then(|e| ctx.view.actor(e)) else {
                return Status::Failed;
            };
            ctx.pawn.set_focal_point(a.loc);
            ctx.pawn.set_look_at_point(a.loc);
            if ue::vsize_sq2d(ctx.pawn.feet() - *destination) < 50.0 * 50.0 {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::ChangeRobotForm {
            look_at, succeeded, ..
        } => {
            let focus = b.core.fetch(*look_at);
            ctx.pawn.set_focal_point(focus);
            ctx.pawn.set_look_at_point(focus);
            if *succeeded {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::AreaOfEffectAttack { complete, .. } => {
            if *complete {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::GuardScanPatrolPoint {
            observe,
            yaw,
            half_cone,
            scan_duration,
            time_elapsed,
            index,
            animations,
        } => {
            *time_elapsed += dt;
            if animations.is_empty() {
                let angle = *time_elapsed / scan_duration.max(1e-3) * 2.0 * std::f32::consts::PI;
                let look = *yaw + angle.sin() * *half_cone * ue::ROT_PER_DEG;
                ctx.pawn.set_look_at_rotation(0.0, look);
            }
            // get_CanFinish: observed long enough, its animations played.
            if *observe > 0.0 && *time_elapsed >= *observe && *index >= animations.len() {
                if !animations.is_empty() {
                    ctx.pawn.anim.stop(0.5, None);
                }
                return Status::Succeeded;
            }
            Status::Running
        }
        Kind::PlayTellFacingFocus { complete } => {
            if *complete {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::MeleeLunge(l) => l.update(b, ctx),
        Kind::AnimatedSkid(s) => s.update(b, ctx),
        Kind::LeapSmash(l) => l.update(b, ctx),
        Kind::SpawnDrones(s) => s.update(b, ctx),
        Kind::HoverAtLocation(h) => h.update(b, ctx),
        Kind::FlyToSniperPoint(f) => f.update(b, ctx),
        Kind::AttachToWall(a) => a.update(b, ctx),
        Kind::DetachFromWall(d) => d.update(b, ctx),
        Kind::CrawlOnSniperPoint(c) => c.update(b, ctx),
        Kind::HoverToFace(h) => h.update(b, ctx),
        Kind::PlaneDash(d) => d.update(ctx),
        Kind::Transform(t) => t.update(ctx),
        Kind::Jump(j) => j.update(ctx),
        Kind::AnimatedAttack { complete, .. } => {
            // Its enemy looked at and its focus, done as its animation ends.
            let Some(at) = b.core.enemy.and_then(|e| ctx.view.actor(e)).map(|a| a.loc) else {
                return Status::Failed;
            };
            ctx.pawn.set_look_at_point(at);
            ctx.pawn.set_focal_point(at);
            if *complete {
                Status::Succeeded
            } else {
                Status::Running
            }
        }
        Kind::ShootAtEnemyHeadOn(d) | Kind::ShootAtEnemyFireForEffect(d) => d.update(b, ctx),
        _ => Status::Succeeded,
    }
}

/// Core.Math.GetInterceptPosition: where to aim to meet a target moving at
/// `vel` with a round of `speed` from `from`.
fn intercept_position(target: Vec3, vel: Vec3, from: Vec3, speed: f32) -> Vec3 {
    let d = target - from;
    let a = vel.dot(vel) - speed * speed;
    let b = 2.0 * d.dot(vel);
    let c = d.dot(d);
    let t = if a.abs() < 1e-6 {
        if b.abs() < 1e-6 {
            0.0
        } else {
            (-c / b).max(0.0)
        }
    } else {
        let disc = b * b - 4.0 * a * c;
        if disc < 0.0 {
            0.0
        } else {
            let s = disc.sqrt();
            let t1 = (-b - s) / (2.0 * a);
            let t2 = (-b + s) / (2.0 * a);
            match (t1 > 0.0, t2 > 0.0) {
                (true, true) => t1.min(t2),
                (true, false) => t1,
                (false, true) => t2,
                _ => 0.0,
            }
        }
    };
    target + vel * t
}

// ---------------------------------------------------------------------------
// Events, callbacks and finishing
// ---------------------------------------------------------------------------

/// OnHandleEvent.
pub fn handle_event(op: &mut Op, b: &mut Brain, ctx: &mut Ctx, e: &Event) {
    match &mut op.kind {
        Kind::MeleeEnemy { .. } => {
            if e.id != EventId::AnimNotify || e.notify.as_ref().is_none_or(|n| n.event != 2) {
                return;
            }
            ctx.pawn.desired_yaw = ctx.pawn.yaw;
            b.set_behavior(ctx, 0, false);
        }
        Kind::ThrowGrenade { status, grenade } => {
            if e.id != EventId::AnimNotify {
                return;
            }
            let Some(n) = &e.notify else { return };
            if n.event != 0 {
                return;
            }
            let socket = if ctx.pawn.stance.handedness == 0 {
                n.socket.clone()
            } else {
                n.mirrored_socket.clone()
            };
            let target = b.core.fetch(super::brain::LR_GRENADE_AIM_POINT);
            let thrown = throw_grenade(b, ctx, grenade, target, &socket);
            *status = if thrown { 1 } else { 2 };
            let now = ctx.view.now;
            let sas = if thrown {
                shared::SAS_SUCCEEDED
            } else {
                shared::SAS_FAILED
            };
            let mut writes = std::mem::take(&mut ctx.shared.writes);
            ctx.shared.squad.release(
                b.core.entity,
                shared::SAT_THROW_GRENADE,
                sas,
                now,
                &mut writes,
            );
            ctx.shared.writes = writes;
        }
        Kind::PlayPositionRelativeAnimation {
            opening_complete,
            end_event,
            ..
        } => {
            if e.id as i32 == *end_event {
                *opening_complete = true;
            }
        }
        Kind::AreaOfEffectAttack {
            attack: Some(a), ..
        } => {
            if e.id == EventId::AnimNotify {
                area_of_effect_blast(b, ctx, a);
            }
        }
        Kind::LeapSmash(l) => l.handle_event(ctx, e),
        Kind::MeleeLunge(l) => l.handle_event(e),
        Kind::AnimatedSkid(s) => s.handle_event(ctx, e),
        Kind::FlyToSniperPoint(f) => f.handle_event(b, ctx, e),
        _ => {}
    }
}

/// TnAreaOfEffectAttackOperator.OnHandleEvent's blast (as its animation's notify
/// comes): each pawn of the other side within its radius of the attacker takes
/// its damage and a push away along the ground and up (the players: shared's
/// player damage, with its momentum and its damage type's victim buff, the jam).
fn area_of_effect_blast(b: &Brain, ctx: &mut Ctx, a: &super::blueprint::AreaOfEffect) {
    let p = &*ctx.pawn;
    for pl in &ctx.view.players {
        if !pl.alive || pl.team == b.core.team || (pl.loc - p.loc).length() > a.radius {
            continue;
        }
        let mut momentum = ue::normal2d(pl.loc - p.loc) * a.momentum_away;
        momentum.z = a.momentum_up;
        ctx.shared.player_damage.push(shared::PlayerDamage {
            player: pl.entity,
            by: p.entity,
            amount: a.damage,
            at: pl.loc,
            dir: momentum,
            melee: false,
            momentum: Some(momentum),
            jam: a.damage_type == "TnDamageTypeGuardKnockback",
            shot: None,
            damage: crate::killfeed::damage_name(&a.damage_type),
        });
    }
}

/// TnThrowGrenadeOperator.ThrowGrenade: the grenade from the hand's socket, tossed
/// to land on the target in a second.
fn throw_grenade(b: &Brain, ctx: &mut Ctx, class: &str, target: Vec3, socket: &str) -> bool {
    let from = ctx.pawn.socket_location(socket);
    let g = shared::GRENADE_GRAVITY_Z;
    let d = target - from;
    let time = 1.0;
    let mut v = d / time;
    v.z = (d.z - 0.5 * g * time * time) / time;
    ctx.shared.throws.push(shared::Throw {
        by: b.core.entity,
        class: class.to_string(),
        from,
        velocity: v,
    });
    true
}

/// An animation's callback for the operator (its delegates).
pub fn anim_callback(op: &mut Op, b: &mut Brain, ctx: &mut Ctx, kind: u8) {
    let serial = op.serial;
    let cb = |k: u8| {
        Some(Callback {
            owner: serial,
            kind: k,
        })
    };
    match &mut op.kind {
        Kind::PlayAnimation { complete, .. }
        | Kind::PlayHitReaction { complete }
        | Kind::HitReaction { complete, .. }
        | Kind::AnimatedShootingAttack { complete, .. }
        | Kind::SideStep { complete, .. }
        | Kind::React { complete, .. }
        | Kind::AreaOfEffectAttack { complete, .. }
        | Kind::PlayTellFacingFocus { complete }
        | Kind::AnimatedAttack { complete, .. }
        | Kind::ChangeRobotForm {
            succeeded: complete,
            ..
        } => {
            if kind == CB_COMPLETE {
                *complete = true;
            }
        }
        Kind::AnimatedSkid(s) => {
            if kind == CB_COMPLETE {
                s.anim_complete = true;
            }
        }
        Kind::PlayInjuredReaction { playing, .. } => {
            if kind == CB_COMPLETE || kind == CB_FAILED {
                *playing = false;
            }
        }
        Kind::PlayPositionRelativeAnimation {
            complete,
            opening_complete,
            ..
        } => match kind {
            CB_COMPLETE => *complete = true,
            CB_OPENING_COMPLETE => *opening_complete = true,
            _ => {}
        },
        Kind::Knockedback {
            landing,
            complete,
            playing,
            land,
            in_air,
            ..
        } => match kind {
            CB_TAKE_OFF_COMPLETE => {
                if ctx.pawn.physics == Physics::Falling {
                    *playing = in_air.clone();
                    ctx.pawn.anim.enqueue(in_air.clone(), None, None);
                }
                if ctx.pawn.physics == Physics::Walking && !*landing {
                    *landing = true;
                    *playing = land.clone();
                    ctx.pawn
                        .anim
                        .enqueue(land.clone(), cb(CB_LAND_COMPLETE), cb(CB_LAND_COMPLETE));
                }
            }
            CB_LAND_COMPLETE => *complete = true,
            _ => {}
        },
        Kind::ScanPatrolPoint {
            time_left,
            index,
            animations,
            active,
        } => match kind {
            CB_SCAN_FINISHED => {
                *index += 1;
                if *time_left <= 0.0 && *index >= animations.len() {
                    return;
                }
                if !animations.is_empty() {
                    *index %= animations.len();
                    *active = animations[*index].clone();
                    let mut p = AnimParams::make(active, 0.5, 0.5, super::anim::RMM_ACCEL);
                    p.mask = super::anim::MASK_UPPER_BODY;
                    ctx.pawn
                        .anim
                        .enqueue(p, cb(CB_SCAN_FINISHED), cb(CB_SCAN_FAILED));
                }
            }
            CB_SCAN_FAILED => *index = animations.len(),
            _ => {}
        },
        Kind::ThrowGrenade { status, .. } => {
            if kind == CB_THROW_COMPLETE {
                *status = if *status == 1 { 3 } else { 2 };
            }
        }
        Kind::AttachToWall(a) if kind == super::sniper::CB_TRANSFORM_COMPLETE => {
            a.transformed();
        }
        _ => {}
    }
    let _ = b;
}

/// OnFinish.
pub fn finish(op: &mut Op, b: &mut Brain, ctx: &mut Ctx, status: Status) {
    match &mut op.kind {
        Kind::SpawnDrones(s) => s.finish(b, ctx),
        Kind::HoverToFace(h) => h.finish(ctx),
        Kind::PlaneDash(d) => d.finish(b),
        Kind::AnimatedAttack {
            params, additive, ..
        } => {
            if matches!(status, Status::Failed | Status::Obsolete | Status::Aborted) {
                if *additive {
                    ctx.pawn.anim.stop_additive(0.1, Some(&params.name));
                } else {
                    ctx.pawn.anim.stop(0.1, Some(&params.name));
                }
            }
        }
        Kind::ShootAtEnemy {
            old_check_for_aiming,
            ..
        } => {
            if let Some(w) = b.sensors.weapon.as_mut() {
                w.check_for_aiming = *old_check_for_aiming;
            }
            b.shooter.set_unrestrained_fire(false);
        }
        Kind::ShootAtLocation {
            stop_shooting_when_finished,
            ..
        } => {
            if *stop_shooting_when_finished {
                b.remove_behavior(ctx, BEHAVIOR_SHOOTER);
            }
        }
        Kind::ShootNearLocation { .. } => {
            b.remove_behavior(ctx, BEHAVIOR_SHOOTER);
            b.shooter.set_unrestrained_fire(false);
        }
        Kind::PlayAnimation {
            params, additive, ..
        } => {
            if matches!(
                status,
                Status::Failed | Status::Obsolete | Status::Aborted | Status::Invalid
            ) {
                if *additive {
                    ctx.pawn.anim.stop_additive(0.1, Some(&params.name));
                } else {
                    ctx.pawn.anim.stop(0.1, Some(&params.name));
                }
            }
        }
        Kind::HitReaction { .. } => ctx.pawn.anim.stop(0.1, None),
        Kind::Knockedback { playing, .. } => {
            if matches!(status, Status::Failed | Status::Obsolete | Status::Aborted) {
                ctx.pawn.anim.stop(0.0, Some(&playing.name));
            }
        }
        Kind::PlayPositionRelativeAnimation {
            found,
            found_out,
            playing_out,
            ..
        } => {
            ctx.pawn.walk_off_ledge = true;
            if status != Status::Succeeded {
                let name = if *playing_out {
                    found_out.name.clone()
                } else {
                    found.name.clone()
                };
                ctx.pawn.anim.stop(0.1, Some(&name));
            }
        }
        Kind::ScanPatrolPoint { active, .. } => {
            let blend = if status == Status::Succeeded {
                0.5
            } else {
                0.1
            };
            ctx.pawn.anim.stop(blend, Some(active.as_str()));
        }
        Kind::EnterCover { .. } | Kind::PeekFromCover { .. } => {
            motion::stop(ctx.pawn);
            with_sensor!(b, vision, s => s.refresh_line_of_sight(b, ctx));
            let clear = match op.kind {
                Kind::EnterCover { .. } => status != Status::Succeeded,
                _ => status != Status::Succeeded,
            };
            if clear {
                ctx.pawn.stance.clear();
            }
        }
        Kind::MeleeEnemy { .. } => super::melee::abort_attack(b, ctx),
        Kind::AnimatedShootingAttack { params, .. } => {
            if matches!(status, Status::Failed | Status::Obsolete | Status::Aborted) {
                ctx.pawn.anim.stop(0.0, Some(&params.name));
            }
        }
        Kind::SideStep { params, .. } => ctx.pawn.anim.stop(0.1, Some(&params.name)),
        Kind::ScanSniperEnemy { .. } => ctx.pawn.laser.stop(),
        Kind::MoveTo(m) => m.finish(b, ctx),
        Kind::SquadMeleeEnemy {
            satisfied_squad_command,
            ..
        } => {
            if matches!(status, Status::Obsolete | Status::Aborted | Status::Invalid) {
                super::melee::abort_attack(b, ctx);
            }
            // SatisfiedSquadCommand: its attack had, back to waiting its turn.
            if *satisfied_squad_command == 1 {
                b.core.set_ws(htn::WS_SQUAD_COMMAND, 0);
            }
        }
        Kind::React { params, .. } => {
            ctx.pawn.anim.stop(0.1, Some(&params.name));
            ctx.pawn.can_play_hit_reaction = true;
        }
        Kind::PlayInjuredReaction { params, .. } => {
            ctx.pawn.anim.stop(params.blend_out, Some(&params.name));
        }
        // (Its form taken however it ended.)
        Kind::ChangeRobotForm { target_form, .. } => {
            super::guard::activate_form(b, ctx.pawn, *target_form);
        }
        Kind::AreaOfEffectAttack {
            attack: Some(a), ..
        } => {
            if matches!(status, Status::Failed | Status::Obsolete | Status::Aborted) {
                ctx.pawn.anim.stop(0.1, Some(&a.attack_animation.name));
            }
        }
        Kind::PlayTellFacingFocus { .. } => motion::stop(ctx.pawn),
        Kind::MeleeLunge(l) => l.finish(b, ctx),
        Kind::AnimatedSkid(s) => s.finish(ctx, status),
        Kind::LeapSmash(l) => l.finish(b, ctx),
        Kind::FlyToSniperPoint(f) => f.finish(b, ctx),
        Kind::AttachToWall(a) => a.finish(b),
        Kind::DetachFromWall(d) => d.finish(b, ctx, status),
        Kind::CrawlOnSniperPoint(c) => c.finish(ctx),
        _ => {}
    }
}

/// OnPlanFinish.
pub fn plan_finish(op: &mut Op, b: &mut Brain, ctx: &mut Ctx, status: Status) {
    if let Kind::ClaimSquadAction {
        action,
        release_on_plan_finish,
    } = op.kind
        && release_on_plan_finish
        && ctx.shared.squad.is_claiming(b.core.entity, action)
    {
        let sas = if status == Status::Succeeded {
            shared::SAS_SUCCEEDED
        } else {
            shared::SAS_FAILED
        };
        let now = ctx.view.now;
        let mut writes = std::mem::take(&mut ctx.shared.writes);
        ctx.shared
            .squad
            .release(b.core.entity, action, sas, now, &mut writes);
        ctx.shared.writes = writes;
    }
}
