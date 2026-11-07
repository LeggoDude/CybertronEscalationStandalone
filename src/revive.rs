//! Escalation's downed players, revives, spectating and the team's lives, as the
//! game's Escalation (TnSurvivalGame) runs them: TnPawn's and TnPlayerPawn's
//! RobotDowned state, TnPlayerPawnSurvival, TnPlayerController's revive,
//! TnSpectateTargetChooser and TnSurvivalGoalTimePressure (TransGame.xxx and
//! TransContent in the Escalation map packages, PC-TransGame.ini).
//!
//! * **Going down** (TnPawn.TakeDamageHealthBelowZero): with more than one player in
//!   the game, a Jazz whose health runs out goes down instead of dying (alone he
//!   dies at once: GlobalCanBeDowned). He falls (Revive_Fall), in robot form (a
//!   vehicle changes back), and lies where he fell: he can't move, jump, dodge,
//!   melee, fine aim, change form, use his ability or support items, buy or pick up
//!   anything, swap weapons or switch hands; he can look round (lying, his aim
//!   follows the view: the nine Aim_Downed_* poses, blended as the game's
//!   AnimNodeAimOffset blends them; turned past 75 degrees, he turns over where he
//!   lies: Nav_Downed_IdlePivot90L/R) and fire his gun (a heavy weapon gives way to
//!   his other gun: ForceEquipRobotWeapon). His shield doesn't recharge. Down, his
//!   health runs from 0 to -540 (TnPlayerPawnSurvival's DownedHealth), draining
//!   18 a second (DownedHealthDrainPerSecond), times the difficulty's damage taken
//!   (TnBuffSurvivalDifficultyPlayerDamageTaken: 0.7, 1 and 1.5 for Easy, Normal
//!   and Hard: 43, 30 and 20 s); for his first 3 s nothing but the drain hurts him
//!   (DownedInvulnerableLength), after that whatever hurts him takes it off as
//!   well. At -540 he dies. Holding G for a second (DownedSuicide) ends it at once.
//!   His barrier goes and his ball goes off as he goes down (ForceEndActiveAbilities);
//!   his buffs run on. The camera comes in close over his shoulder (main.rs's
//!   DOWNED_CAMERA: Downed_Robot_STRATEGY's 60-degree view and its 0.75 s
//!   TransitionTime), and the HUD shows the downed meter in place of its corners
//!   (hud.rs: "YOU ARE DOWNED", "HOLD G TO SELF DESTRUCT", the ring running down).
//! * **Reviving** (UpdateRevivablePawn, TnContextualCommandRevive): a teammate on
//!   his feet in robot form within 5 m of him (the nearest downed teammate nobody
//!   else is reviving) is offered "Hold E to Revive"; E held 0.3 s starts it
//!   (XButtonInputHandler's ButtonTimeOut): he kneels to it facing him (Revive_Heal,
//!   3.83 s, rooted where he is, his HUD's bar filling under "Reviving <name>"), and
//!   the downed one stops draining. Letting go of E stops it (the drain starts
//!   again); held to the end, the downed one is revived: health 115 (RevivalHealth,
//!   health first, then shield: 100 and 15) and he gets up (Revive_Getup, 1.83 s,
//!   still held where he lies), and the reviver's Revives go up. The others' HUDs
//!   show "An Ally is Downed" while one is, and his marker his downed health in red
//!   with its downed icon strobing.
//! * **Dead** (no timed respawn in Escalation: TnRespawnHelperRoundEnd): the dead
//!   watch their own death for 3 s (MinRespawnDelay), then spectate a teammate who
//!   is up (downed counts as up), [ and ] cycling through them
//!   (SwitchSpectatorTarget), the camera his (SpectateOverTheShoulder_STRATEGY:
//!   main.rs's SPECTATE_*), under the respawn screen (MultiplayerRespawn_GFX:
//!   "WAITING FOR NEXT WAVE TO RESPAWN", "CURRENTLY SPECTATING" and his name: hud.rs);
//!   with nobody to watch, the level's camera.
//! * **The team's lives** (TnSurvivalActor.NumAttempts 3): once nobody is up (all
//!   dead) for 3 s (FailureEndRoundTime), the team loses one (its marks on the HUD,
//!   bottom left, go red from the left) and the round ends in failure: the wave
//!   starts again (waves.rs: "Wave 1 Failed, Restarting Wave", its fail stinger).
//!   Every round's end, failed or won (RoundEnded), brings the dead back at their
//!   starts and the downed up (GetRevive). Losing the third ends the game (EndGame):
//!   the HUD's power down plays and the final scores show (EndGameStats_GFX:
//!   scoreboard.rs) before the host takes everyone back to the lobby (menu.rs).
//!   The host decides (net.rs's roster carries the lives); each PC runs its own
//!   player's down, drain and revive, and tells the others (JazzSnapshot::revive,
//!   JazzEvent::Revive from the reviver to the downed one's PC).

use bevy::prelude::*;

use crate::hud::{HudState, Prompt};
use crate::net::{JazzEvent, Net, ReviveSnap};
use crate::sound::Sfx;
use crate::{Form, LocalPlayer, PLAYER_MAX_SHIELD, Player, PlayerInput, PlayerSlot, RemotePlayer};

// ---------------------------------------------------------------------------
// The game's numbers
// ---------------------------------------------------------------------------

/// TnPlayerPawnSurvival's DownedHealth: the health he has down (from 0 to -540).
pub const DOWNED_HEALTH: f32 = 540.0;
/// Its DownedHealthDrainPerSecond.
const DOWNED_DRAIN: f32 = 18.0;
/// TnBuffSurvivalDifficultyPlayerDamageTaken's FloatModifier for Easy, Normal and
/// Hard (TnSurvivalGame.GetDifficultyBuffLevel): the drain is damage, so it scales
/// (as with the wave's own buff: waves.rs's taken).
pub(crate) const DIFFICULTY_DAMAGE: [f32; 3] = [0.7, 1.0, 1.5];
/// TnBuffSurvivalDifficultyPlayerDamageDone's FloatModifier for Easy, Normal and
/// Hard (TransGame's; the difficulty's other player buff, TnSurvivalGame's
/// DifficultyBuffsPlayer): what the damage a player does is scaled by (waves.rs's
/// done). Like the damage taken, its levels (the difficulty times 4, plus the
/// players less one) have the same value for any number of players.
pub(crate) const DIFFICULTY_DAMAGE_DONE: [f32; 3] = [1.5, 1.0, 0.6];
/// TnPlayerPawn's DownedInvulnerableLength: nothing but the drain hurts him for so
/// long after he goes down.
const DOWNED_IMMUNE_SECS: f32 = 3.0;
/// DownedSuicideButtonInputHandler's ButtonTimeOut: G held this long.
const SELF_DESTRUCT_SECS: f32 = 1.0;
/// The revive's XButtonInputHandler ButtonTimeOut: E held this long starts it.
const REVIVE_HOLD_SECS: f32 = 0.3;
/// TnPlayerController's ReviveDistance (500 units).
const REVIVE_DISTANCE: f32 = 5.0;
/// TnPawn's RevivalHealth, healed health first then shield (TnHealTypeRevive:
/// SH_HealBaseThenShield).
const REVIVAL_HEALTH: f32 = 115.0;
/// Revive_Heal (115 frames at 30), the reviver's: the revive is done as it ends
/// (GiveReviveSucceed).
pub const HEAL_SECS: f32 = 115.0 / 30.0;
/// Revive_Getup (55 frames at 30): the revived one getting up.
pub const GETUP_SECS: f32 = 55.0 / 30.0;
/// Revive_Fall (90 frames at 30): going down.
pub const FALL_SECS: f32 = 90.0 / 30.0;
/// Nav_Downed_IdlePivot90L/R (25 frames at 30): a turn over where he lies.
pub const PIVOT_SECS: f32 = 25.0 / 30.0;
/// The turn lying down: past this from his facing, he turns over (as the standing
/// robot's PIVOT_START), a quarter turn.
const PIVOT_START: f32 = 75.0 * std::f32::consts::PI / 180.0;
const PIVOT_TURN: f32 = std::f32::consts::FRAC_PI_2;
/// TnOnlineGameSettingsBase's MinRespawnDelay: the dead watch their death so long
/// before they spectate.
pub const DEATH_VIEW_SECS: f32 = 3.0;
/// TnSurvivalGoalTimePressure's FailureEndRoundTime: nobody up this long, and the
/// team loses a life.
const FAILURE_END_ROUND: f32 = 3.0;
/// TnSurvivalActor's NumAttempts (every map's).
pub const LIVES: u8 = 3;
/// TnSpectateTargetChooser checks its target every second.
const SPECTATE_CHECK_SECS: f32 = 1.0;

/// The announcer's "An ally was destroyed" (TnPlayerPawnSurvival.PlayDying:
/// MP_ESC_ALLY_DESTROYED): Optimus Prime's for the Autobots, Megatron's on the
/// Decepticon maps (build_sounds.py writes his line under its own name).
const SND_ALLY_DESTROYED: &str = "BL_DX_SWITCHBOARD.SC019358";
const SND_ALLY_DESTROYED_DECEPTICONS: &str = "BL_DX_SWITCHBOARD.SC019358_MGTRON";
/// The game over (TnPlayerController.OnCampaignGameOver's DeathSound).
const SND_GAME_OVER: &str = "BL_LVL_HUD_INTERFACE.HUD_DEATH_POWER_DOWN";

// ---------------------------------------------------------------------------
// A Jazz's state
// ---------------------------------------------------------------------------

/// Where a Jazz is with all this (on Player as `revive`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Revive {
    /// More than one player in the game: his health running out takes him down
    /// rather than killing him (this PC's player's: mark_downable).
    pub downable: bool,
    /// Down: see [`Downed`].
    pub downed: Option<Downed>,
    /// Getting up after a revive: seconds into Revive_Getup (None when not).
    pub getting_up: Option<f32>,
    /// Reviving a downed teammate: see [`Healing`].
    pub healing: Option<Healing>,
    /// E held near a downed teammate (seconds) and who (his slot).
    pub hold: f32,
    pub hold_target: Option<usize>,
    /// G held while down (seconds).
    pub self_destruct: f32,
    /// Dead, and out until the team's round ends (no respawn on a timer).
    pub out: bool,
    /// Seconds since he died (his death's view, then spectating).
    pub dead_for: f32,
    /// He died down (his body goes limp as it lies, no death clip: downed.rs).
    pub died_downed: bool,
    /// Brought back by the team's round ending (TnRespawnHelperRoundEnd respawns
    /// every dead player then): at once, his body gone or not (main.rs's
    /// player_health).
    pub back: bool,
}

/// A downed Jazz: his downed health left (DOWNED_HEALTH to 0), seconds since he
/// went down, the player reviving him (his slot: the drain stops meanwhile), and a
/// turn over where he lies playing.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Downed {
    pub left: f32,
    pub age: f32,
    pub reviver: Option<usize>,
    pub pivot: Option<Turn>,
}

/// A turn lying down (Nav_Downed_IdlePivot90L/R): left (positive) or right, from
/// the facing it started at, seconds into it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Turn {
    pub left: bool,
    pub from: f32,
    pub t: f32,
}

/// Reviving a teammate (his slot), seconds into Revive_Heal.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Healing {
    pub target: usize,
    pub t: f32,
}

impl Revive {
    /// Down (or getting up after a revive, or reviving another): held where he is,
    /// doing nothing but what that allows.
    pub fn locked(&self) -> bool {
        self.downed.is_some() || self.getting_up.is_some() || self.healing.is_some()
    }

    /// Down or getting up (the game's IsDownedOrReviving): his gun only.
    pub fn down(&self) -> bool {
        self.downed.is_some() || self.getting_up.is_some()
    }

    /// His downed health as the HUD's meter shows it (1 full, 0 dead), if down.
    pub fn downed_fraction(&self) -> Option<f32> {
        self.downed
            .map(|d| (d.left / DOWNED_HEALTH).clamp(0.0, 1.0))
    }

    /// For his snapshot (net.rs).
    pub fn snap(&self) -> ReviveSnap {
        let none = u8::MAX;
        let slot = |s: Option<usize>| s.map_or(none, |s| s as u8);
        let (state, clock) = if let Some(d) = self.downed {
            (ReviveSnap::DOWNED, d.age)
        } else if let Some(t) = self.getting_up {
            (ReviveSnap::GETTING_UP, t)
        } else if let Some(h) = self.healing {
            (ReviveSnap::HEALING, h.t)
        } else if self.out {
            (ReviveSnap::OUT, self.dead_for)
        } else {
            (ReviveSnap::UP, 0.0)
        };
        let pivot = self.downed.and_then(|d| d.pivot);
        ReviveSnap {
            state,
            clock,
            downed_left: self.downed.map_or(0.0, |d| d.left),
            reviver: slot(self.downed.and_then(|d| d.reviver)),
            target: slot(self.healing.map(|h| h.target)),
            pivot: pivot.map_or(0, |p| if p.left { 1 } else { -1 }),
            pivot_t: pivot.map_or(0.0, |p| p.t),
            died_downed: self.died_downed,
        }
    }

    /// Another player's copy, from his snapshot.
    pub fn follow(&mut self, s: &ReviveSnap, heading: f32) {
        let slot = |b: u8| (b != u8::MAX).then_some(b as usize);
        self.out = s.state == ReviveSnap::OUT;
        self.died_downed = s.died_downed;
        self.downed = (s.state == ReviveSnap::DOWNED).then(|| Downed {
            left: s.downed_left.clamp(0.0, DOWNED_HEALTH),
            age: s.clock.max(0.0),
            reviver: slot(s.reviver),
            pivot: (s.pivot != 0).then(|| Turn {
                left: s.pivot > 0,
                // (His PC turns him: the clip is what matters here.)
                from: heading,
                t: s.pivot_t.clamp(0.0, PIVOT_SECS),
            }),
        });
        self.getting_up = (s.state == ReviveSnap::GETTING_UP).then_some(s.clock.max(0.0));
        self.healing = (s.state == ReviveSnap::HEALING).then(|| Healing {
            target: s.target as usize,
            t: s.clock.max(0.0),
        });
        if self.out {
            self.dead_for = s.clock.max(0.0);
        }
    }

    /// Runs on a copy's state between his snapshots.
    pub fn run_on(&mut self, dt: f32) {
        if let Some(d) = self.downed.as_mut() {
            d.age += dt;
            if let Some(p) = d.pivot.as_mut() {
                p.t = (p.t + dt).min(PIVOT_SECS);
            }
        }
        if let Some(t) = self.getting_up.as_mut() {
            *t += dt;
        }
        if let Some(h) = self.healing.as_mut() {
            h.t += dt;
        }
        if self.out {
            self.dead_for += dt;
        }
    }
}

/// His health ran out (TnPawn.TakeDamageHealthBelowZero), able to go down: he goes
/// down. (main.rs's damage_player; player_health turns a vehicle back.)
pub(crate) fn go_down(player: &mut Player) {
    player.health = 0.0;
    player.shield = 0.0;
    player.revive.downed = Some(Downed {
        left: DOWNED_HEALTH,
        age: 0.0,
        reviver: None,
        pivot: None,
    });
    player.revive.getting_up = None;
    player.revive.healing = None;
    player.revive.hold = 0.0;
    player.revive.hold_target = None;
    player.revive.self_destruct = 0.0;
    // (Right handed, his hands not changing over: RobotDowned's BeginState. What
    // he was doing stops; a heavy weapon gives way to his other gun.)
    player.left_handed = false;
    player.hand_switch = None;
    player.start_hand_switch = false;
    player.action = None;
    player.melee_hit_in = None;
    player.melee_sweep = crate::melee::Sweep::default();
    player.dash_left = 0.0;
    player.sprinting = false;
    player.aiming = false;
    player.pivot = None;
    crate::put_away_heavy(player);
}

/// A blow to him down (main.rs's damage_player): after his first seconds it comes
/// off his downed health, and at none he dies. Returns whether it hurt him.
pub(crate) fn hurt_downed(player: &mut Player, amount: f32) -> bool {
    let Some(d) = player.revive.downed.as_mut() else {
        return false;
    };
    if d.age < DOWNED_IMMUNE_SECS || amount <= 0.0 {
        return false;
    }
    d.left -= amount;
    if d.left <= 0.0 {
        die_downed(player);
    }
    true
}

/// He dies down: his downed health gone, or G held (his normal death from there,
/// limp where he lies: downed.rs).
fn die_downed(player: &mut Player) {
    player.revive.downed = None;
    player.revive.died_downed = true;
    player.health = 0.0;
    player.shield = 0.0;
    player.dying = true;
}

/// Revived (GetRevive): health 115, his health first then his shield, and he gets
/// up.
fn get_revive(player: &mut Player) {
    player.revive.downed = None;
    let health = REVIVAL_HEALTH.min(player.max_health());
    player.health = health;
    player.shield = (REVIVAL_HEALTH - health).min(PLAYER_MAX_SHIELD);
    player.revive.getting_up = Some(0.0);
    player.revive.self_destruct = 0.0;
    player.since_hit = 0.0;
}

/// Where his body faces lying down (in place of main.rs's face_camera): it stays
/// put while the view turns and his aim follows it, until the view has turned
/// PIVOT_START from it: then he turns over a quarter turn (Nav_Downed_IdlePivot90L/R,
/// turning as the clip does). Getting up or reviving, he faces as he is.
pub(crate) fn face_downed(player: &mut Player, view_yaw: f32, dt: f32) {
    let heading = player.heading;
    if let Some(d) = player.revive.downed.as_mut() {
        // (Not while he falls.)
        let falling = d.age < FALL_SECS;
        if let Some(mut p) = d.pivot {
            p.t += dt;
            let k = (p.t / PIVOT_SECS).min(1.0);
            let turn = if p.left { PIVOT_TURN } else { -PIVOT_TURN };
            player.heading = p.from + turn * k;
            d.pivot = (k < 1.0).then_some(p);
        } else {
            let off = crate::angle_to(heading, view_yaw);
            if !falling && off.abs() > PIVOT_START {
                d.pivot = Some(Turn {
                    left: off > 0.0,
                    from: heading,
                    t: 0.0,
                });
            }
        }
    }
    player.heading = player.heading.rem_euclid(std::f32::consts::TAU);
    player.twist = crate::angle_to(player.heading, view_yaw);
}

// ---------------------------------------------------------------------------
// The team
// ---------------------------------------------------------------------------

/// The team's lives and rounds: the host's (or this PC's, playing alone), which
/// the roster carries to the others (net.rs).
#[derive(Resource, Debug)]
pub struct Team {
    /// Lives left (NumAttempts).
    pub lives: u8,
    /// How many rounds have ended in failure, and how many won (waves.rs): each
    /// brings the dead back and the downed up on every PC.
    pub round: u8,
    pub cleared: u8,
    /// The game is over.
    pub over: bool,
    /// (The host's:) someone has been up since the match began (the count starts
    /// with the first living player), and how long nobody has been.
    armed: bool,
    nobody_for: f32,
    /// (Every PC's:) the rounds and game over as this PC has played them out.
    seen_round: u8,
    seen_cleared: u8,
    seen_over: bool,
    /// Seconds since the game ended.
    pub over_for: f32,
}

impl Default for Team {
    fn default() -> Self {
        Team {
            lives: LIVES,
            round: 0,
            cleared: 0,
            over: false,
            armed: false,
            nobody_for: 0.0,
            seen_round: 0,
            seen_cleared: 0,
            seen_over: false,
            over_for: 0.0,
        }
    }
}

/// Revive events for this PC's player from the others' (net_apply's: from whom).
#[derive(Resource, Default)]
pub(crate) struct ReviveInbox(pub Vec<(usize, JazzEvent)>);

/// Who the dead player here watches (a slot), and when his choice was last
/// checked.
#[derive(Resource, Default)]
pub struct Spectate {
    pub target: Option<usize>,
    since_check: f32,
}

/// The revive event's stages (JazzEvent::Revive's `stage`).
pub const STAGE_START: u8 = 0;
pub const STAGE_ABORT: u8 = 1;
pub const STAGE_DONE: u8 = 2;

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

pub struct RevivePlugin;

impl Plugin for RevivePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Team>()
            .init_resource::<ReviveInbox>()
            .init_resource::<Spectate>()
            .add_systems(OnEnter(crate::menu::AppState::Game), reset)
            .add_systems(
                Update,
                (
                    mark_downable.before(crate::read_local_input),
                    mask_input
                        .after(crate::read_local_input)
                        .before(crate::net_apply),
                    (
                        bleed,
                        revive_others,
                        take_revives,
                        team_lives,
                        round_end,
                        spectate,
                    )
                        .chain()
                        .after(crate::player_health)
                        .before(crate::toggle_form),
                    feed_hud
                        .after(crate::feed_hud_allies)
                        .before(crate::hud::HudSystems),
                )
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

/// A new match: the team's three lives, nobody down.
fn reset(
    mut team: ResMut<Team>,
    mut inbox: ResMut<ReviveInbox>,
    mut spectate: crate::split::BySeat<Spectate>,
) {
    *team = Team::default();
    inbox.0.clear();
    for spectate in spectate.each() {
        *spectate = Spectate::default();
    }
}

/// This PC's player goes down rather than dying with others in the game
/// (GlobalCanBeDowned: GetNumPlayers() > 1).
fn mark_downable(net: Res<Net>, mut players: Query<&mut Player, With<LocalPlayer>>) {
    let others = net.player_count() > 1;
    for mut player in &mut players {
        if player.revive.downable != others {
            player.revive.downable = others;
        }
    }
}

/// What a player down (or getting up, or reviving another) can't do: his keys for
/// it are let go here, before anything acts on them.
fn mask_input(mut players: Query<(&Player, &mut PlayerInput), With<LocalPlayer>>) {
    for (player, mut input) in &mut players {
        let r = &player.revive;
        if !r.locked() {
            continue;
        }
        input.movement = Vec2::ZERO;
        input.boost = false;
        input.aim = false;
        input.jump = false;
        input.dash = false;
        input.melee = false;
        input.transform = false;
        input.switch_hands = false;
        input.ability = false;
        input.tech = [false; 2];
        input.ascend = false;
        input.descend = false;
        // (Down or getting up, his gun only: no other to swap to.)
        input.swap_weapon = false;
        if r.healing.is_some() {
            // (Kneeling to it, his hands are busy: E stays held for it.)
            input.fire = false;
            input.fire_pressed = false;
            input.reload = false;
        } else {
            // (Down, E does nothing: he picks nothing up and buys nothing.)
            input.pick_up = false;
        }
    }
}

/// This PC's downed player: the drain (stopped while he is revived), G held to end
/// it (a controller's B: pad.rs), and the get-up after a revive running out.
#[allow(clippy::type_complexity)]
fn bleed(
    time: Res<Time>,
    (keys, pads): (Res<ButtonInput<KeyCode>>, crate::pad::Pads),
    mut players: Query<
        (&mut Player, &PlayerInput, Option<&crate::split::Seat>),
        (With<LocalPlayer>, Without<RemotePlayer>),
    >,
) {
    let dt = time.delta_secs();
    // (The difficulty's damage taken, and the wave's: waves.rs.)
    let rate = DOWNED_DRAIN * crate::waves::taken();
    for (mut player, input, seat) in &mut players {
        let player = &mut *player;
        // (Each by his own controls: split screen's guest by his controller.)
        let seat = crate::split::seat_of(seat);
        let pad = pads.seat(seat);
        let by_pad = pad.active || seat == 1;
        if let Some(t) = player.revive.getting_up.as_mut() {
            *t += dt;
            if *t >= GETUP_SECS {
                player.revive.getting_up = None;
            }
        }
        if player.dead_left > 0.0 || player.dying {
            player.revive.dead_for += dt;
        } else {
            player.revive.dead_for = 0.0;
        }
        let Some(d) = player.revive.downed.as_mut() else {
            player.revive.self_destruct = 0.0;
            continue;
        };
        d.age += dt;
        if d.reviver.is_none() {
            d.left -= rate * dt;
        }
        // (Hold G: DownedSuicide, as the HUD says; not with a menu up.)
        let give_up = if by_pad {
            pad.held(crate::pad::Button::B)
        } else {
            keys.pressed(KeyCode::KeyG)
        };
        if input.control && give_up {
            player.revive.self_destruct += dt;
        } else {
            player.revive.self_destruct = 0.0;
        }
        let gone = d.left <= 0.0;
        if gone || player.revive.self_destruct >= SELF_DESTRUCT_SECS {
            // (The kill feed's: bleeding out by TnDamageTypeDownedDrain, giving up
            // by DmgType_Suicided, each by his own hand.)
            player.last_attack = (
                crate::killfeed::Who::Nobody,
                if gone {
                    "TnDamageTypeDownedDrain"
                } else {
                    "DmgType_Suicided"
                },
            );
            die_downed(player);
        }
    }
}

/// This PC's player reviving a downed teammate: the nearest within reach that
/// nobody else is reviving, E held to start, held to the end (Revive_Heal) to
/// revive him, let go to stop; each step goes to the downed one's PC (split
/// screen's other player here: take_revives takes it from his `played`).
#[allow(clippy::type_complexity)]
fn revive_others(
    time: Res<Time>,
    mut tally: crate::split::BySeat<crate::scoreboard::Tally>,
    mut me: Query<
        (
            &Transform,
            &mut Player,
            &PlayerInput,
            &PlayerSlot,
            Option<&crate::split::Seat>,
        ),
        With<LocalPlayer>,
    >,
    others: Query<(&PlayerSlot, &Transform, &Player), (With<RemotePlayer>, Without<LocalPlayer>)>,
) {
    let dt = time.delta_secs();
    // Everyone he could revive: the others online and this PC's other player.
    let everyone: Vec<Mate> = others
        .iter()
        .chain(me.iter().map(|(t, p, _, s, _)| (s, t, p)))
        .map(|(s, t, p)| Mate {
            slot: s.0,
            at: t.translation,
            downed: p.revive.downed,
        })
        .collect();
    for (transform, mut player, input, slot, seat) in &mut me {
        let mates: Vec<Mate> = everyone
            .iter()
            .filter(|m| m.slot != slot.0)
            .copied()
            .collect();
        let tally = tally.seat(crate::split::seat_of(seat));
        revive_from(
            dt,
            (transform, &mut player, input, Some(slot.0)),
            &mates,
            tally,
        );
    }
}

/// A teammate as a player who might revive him sees him: his slot, where he is,
/// and his being down.
#[derive(Clone, Copy)]
struct Mate {
    slot: usize,
    at: Vec3,
    downed: Option<Downed>,
}

/// One of this PC's players reviving (revive_others): `my_slot` his, `mates` his
/// teammates.
fn revive_from(
    dt: f32,
    (transform, player, input, my_slot): (&Transform, &mut Player, &PlayerInput, Option<usize>),
    mates: &[Mate],
    tally: &mut crate::scoreboard::Tally,
) {
    let at = transform.translation;
    // Who he could revive (FindRevivablePawnByDistance): up himself, in robot form,
    // on the ground; the nearest downed teammate within reach whom nobody else is
    // reviving.
    let able = !player.revive.down()
        && player.dead_left <= 0.0
        && !player.dying
        && player.form == Form::Robot
        && player.transform_left <= 0.0
        && player.grounded;
    let target = |slot: usize| mates.iter().find(|m| m.slot == slot);
    let nearest = mates
        .iter()
        .filter(|m| {
            m.downed
                .is_some_and(|d| d.reviver.is_none() || d.reviver == my_slot)
                && m.at.distance(at) <= REVIVE_DISTANCE
        })
        .min_by(|a, b| a.at.distance(at).total_cmp(&b.at.distance(at)))
        .map(|m| m.slot);
    if let Some(mut h) = player.revive.healing {
        // Healing: on while E is held and he is still down (and his to revive).
        let still = target(h.target)
            .and_then(|m| m.downed)
            .is_some_and(|d| d.reviver.is_none() || d.reviver == my_slot);
        h.t += dt;
        if !input.pick_up || !still || player.dead_left > 0.0 || player.dying {
            player.revive.healing = None;
            player.played.push(JazzEvent::Revive {
                target: h.target as u8,
                stage: STAGE_ABORT,
            });
        } else if h.t >= HEAL_SECS {
            player.revive.healing = None;
            player.played.push(JazzEvent::Revive {
                target: h.target as u8,
                stage: STAGE_DONE,
            });
            // (AddRevive: the scoreboard's Revives.)
            tally.revives = tally.revives.saturating_add(1);
        } else {
            player.revive.healing = Some(h);
        }
        return;
    }
    // The prompt and the hold.
    player.revive.hold_target = nearest.filter(|_| able);
    match player.revive.hold_target {
        Some(slot) if input.pick_up => {
            player.revive.hold += dt;
            if player.revive.hold >= REVIVE_HOLD_SECS {
                player.revive.hold = 0.0;
                player.revive.healing = Some(Healing {
                    target: slot,
                    t: 0.0,
                });
                // (He kneels to it facing the downed one.)
                if let Some(m) = target(slot) {
                    let to = m.at - at;
                    if to.x.abs() + to.z.abs() > 1e-3 {
                        player.heading = (-to.x).atan2(-to.z);
                    }
                }
                player.played.push(JazzEvent::Revive {
                    target: slot as u8,
                    stage: STAGE_START,
                });
            }
        }
        _ => player.revive.hold = 0.0,
    }
}

/// Revive events from the others for this PC's downed player: one reviver at a
/// time (the first; the drain stops while he revives), his stop (the drain starts
/// again), his revive done (GetRevive). A reviver who stops showing as reviving him
/// (gone, or turned to something else) lets him go too. (Split screen: this PC's
/// other player's straight from him, revive_others.)
#[allow(clippy::type_complexity)]
fn take_revives(
    mut inbox: ResMut<ReviveInbox>,
    mut me: Query<(&mut Player, &PlayerSlot), (With<LocalPlayer>, Without<RemotePlayer>)>,
    others: Query<(&PlayerSlot, &Player), With<RemotePlayer>>,
) {
    let mut events = std::mem::take(&mut inbox.0);
    let mut healers: Vec<(usize, Option<Healing>)> = others
        .iter()
        .map(|(s, p)| (s.0, p.revive.healing))
        .collect();
    for (p, s) in &me {
        healers.push((s.0, p.revive.healing));
        events.extend(
            p.played
                .iter()
                .filter(|e| matches!(e, JazzEvent::Revive { .. }))
                .map(|e| (s.0, e.clone())),
        );
    }
    for (mut player, slot) in &mut me {
        let mine = slot.0;
        for (from, event) in &events {
            let (from, JazzEvent::Revive { target, stage }) = (*from, event) else {
                continue;
            };
            if *target as usize != mine || from == mine {
                continue;
            }
            let Some(d) = player.revive.downed.as_mut() else {
                continue;
            };
            match *stage {
                STAGE_START if d.reviver.is_none() => d.reviver = Some(from),
                STAGE_ABORT if d.reviver == Some(from) => d.reviver = None,
                STAGE_DONE if d.reviver == Some(from) => get_revive(&mut player),
                _ => {}
            }
        }
        if let Some(d) = player.revive.downed.as_mut()
            && let Some(by) = d.reviver
        {
            let healing = healers
                .iter()
                .find(|(s, _)| *s == by)
                .is_some_and(|(_, h)| h.is_some_and(|h| h.target == mine));
            if !healing {
                d.reviver = None;
            }
        }
    }
}

/// Up: in the match, and not dead (downed counts).
pub(crate) fn up(player: &Player) -> bool {
    !player.waiting && player.dead_left <= 0.0 && !player.dying && !player.revive.out
}

/// The host's (or the lone player's) count of the team (TnSurvivalGoalTimePressure.
/// OnUpdate): once nobody has been up for FAILURE_END_ROUND, a life goes; the
/// last ends the game. The roster takes it to the others.
pub(crate) fn team_lives(
    time: Res<Time>,
    mut net: ResMut<Net>,
    mut team: ResMut<Team>,
    players: Query<&Player>,
) {
    let online = net.my_slot().is_some() && net.player_count() > 0;
    if online && !net.is_host() {
        // (A client takes the host's.)
        if let Some((lives, round, over)) = net.team() {
            team.lives = lives;
            team.round = round;
            team.over = over;
        }
        return;
    }
    if !team.over {
        let anyone = players.iter().any(up);
        // (Counting from the first living player: NumAttempts copied then.)
        team.armed |= anyone;
        if anyone || !team.armed {
            team.nobody_for = 0.0;
        } else {
            team.nobody_for += time.delta_secs();
            if team.nobody_for >= FAILURE_END_ROUND {
                team.nobody_for = 0.0;
                team.lives = team.lives.saturating_sub(1);
                if team.lives == 0 {
                    team.over = true;
                } else {
                    team.round = team.round.wrapping_add(1);
                }
            }
        }
    }
    net.set_team(team.lives, team.round, team.over);
}

/// Every PC plays out the team's round ends and the game over for its own
/// player: a round's end, failed or won (waves.rs), brings him back if he is dead
/// (at his start, as player_health brings the dead back) and up if he is down
/// (GetRevive, no reviver); the game over stops the music, plays the power down and
/// ends his part (the final scores: hud.rs, scoreboard.rs).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn round_end(
    time: Res<Time>,
    current_map: Res<crate::CurrentMap>,
    mut team: ResMut<Team>,
    mut sfx: ResMut<Sfx>,
    mut players: ParamSet<(
        Query<&mut Player, (With<LocalPlayer>, Without<RemotePlayer>)>,
        Query<(Entity, &Player)>,
    )>,
    mut seen_dead: Local<Vec<Entity>>,
) {
    // The announcer's "An ally was destroyed" for every death, here as everywhere
    // (TnPlayerPawnSurvival.PlayDying runs on every PC).
    let decepticons = current_map
        .0
        .and_then(|m| crate::MAPS.get(m))
        .is_some_and(|m| m.decepticons);
    for (e, p) in &players.p1() {
        let dead = p.dead_left > 0.0 || p.dying;
        let known = seen_dead.contains(&e);
        if dead && !known {
            seen_dead.push(e);
            sfx.play(if decepticons {
                SND_ALLY_DESTROYED_DECEPTICONS
            } else {
                SND_ALLY_DESTROYED
            });
        } else if !dead && known {
            seen_dead.retain(|x| *x != e);
        }
    }
    if team.round != team.seen_round || team.cleared != team.seen_cleared {
        team.seen_round = team.round;
        team.seen_cleared = team.cleared;
        for mut player in &mut players.p0() {
            if player.revive.out || player.dead_left > 0.0 || player.dying {
                // (player_health brings him back at his start now, his body gone
                // or not.)
                player.revive.out = false;
                player.revive.back = true;
                player.dead_left = player.dead_left.min(crate::DEAD_HOLD);
            } else if player.revive.downed.is_some() {
                get_revive(&mut player);
            }
        }
    }
    if team.over && !team.seen_over {
        team.seen_over = true;
        team.over_for = 0.0;
        // (The match has no music playing to stop.)
        sfx.play(SND_GAME_OVER);
    }
    if team.over {
        team.over_for += time.delta_secs();
    }
}

/// The HUD's part in all this (hud.rs): this player's downed meter, the team's
/// lives, another player down, the revive he is giving ("Reviving <name>") or
/// offering ("Hold E to Revive"), and the respawn screen once his death's view is
/// over, with the teammate he watches. (Each of this PC's players' HUD: split
/// screen's guest's his own.)
#[allow(clippy::type_complexity)]
fn feed_hud(
    net: Res<Net>,
    team: Res<Team>,
    spectate: crate::split::BySeatRef<Spectate>,
    mut huds: crate::split::BySeat<HudState>,
    me: Query<(&Player, &PlayerSlot, Option<&crate::split::Seat>), With<LocalPlayer>>,
    others: Query<&Player, With<RemotePlayer>>,
) {
    let name = |slot: usize| {
        net.players()
            .find(|(s, _)| *s == slot)
            .map(|(_, p)| p.name.clone())
            .unwrap_or_default()
    };
    let remote_downed = others.iter().any(|p| p.revive.downed.is_some());
    for (seat, hud) in huds.each().into_iter().enumerate() {
        hud.lives = team.lives;
        // (Another player down: online, or this PC's other.)
        hud.ally_downed = remote_downed
            || me
                .iter()
                .any(|(p, _, s)| crate::split::seat_of(s) != seat && p.revive.downed.is_some());
        // (The final scores take the HUD's place at the game over: scoreboard.rs.)
        if team.over {
            hud.hidden = true;
        }
        let Some((player, _, _)) = me.iter().find(|(.., s)| crate::split::seat_of(*s) == seat)
        else {
            continue;
        };
        let r = &player.revive;
        hud.downed = r.downed_fraction();
        hud.revive_bar = r
            .healing
            .map(|h| (name(h.target), (h.t / HEAL_SECS).clamp(0.0, 1.0)));
        if r.locked() {
            hud.prompt = None;
        } else if r.hold_target.is_some() {
            hud.prompt = Some(Prompt::Revive);
        }
        let watching = r.out && r.dead_for >= DEATH_VIEW_SECS;
        hud.spectating = watching.then(|| spectate.seat(seat).target.map(name));
    }
}

/// Who the dead player here watches (TnSpectateTargetChooser): after his death's
/// view, a teammate who is up (downed counts), checked every second; [ and ] go
/// through them (SwitchSpectatorTarget -1, +1: a controller's RB and LB). None: the
/// level's camera. (Each of this PC's players his own: split screen's guest
/// watches by his controller, his partner here among those he can watch.)
#[allow(clippy::type_complexity)]
fn spectate(
    time: Res<Time>,
    (keys, pads): (Res<ButtonInput<KeyCode>>, crate::pad::Pads),
    mut spectate: crate::split::BySeat<Spectate>,
    me: Query<(&Player, &PlayerSlot, Option<&crate::split::Seat>), With<LocalPlayer>>,
    others: Query<(&PlayerSlot, &Player), With<RemotePlayer>>,
) {
    for (player, slot, seat) in &me {
        let seat = crate::split::seat_of(seat);
        let spectate = spectate.seat(seat);
        let watching = (player.dead_left > 0.0 || player.dying || player.revive.out)
            && player.revive.dead_for >= DEATH_VIEW_SECS;
        if !watching {
            spectate.target = None;
            spectate.since_check = 0.0;
            continue;
        }
        let mut up_slots: Vec<usize> = others
            .iter()
            .chain(me.iter().map(|(p, s, _)| (s, p)))
            .filter(|(s, p)| s.0 != slot.0 && up(p))
            .map(|(s, _)| s.0)
            .collect();
        up_slots.sort_unstable();
        spectate.since_check += time.delta_secs();
        let valid = spectate.target.is_some_and(|t| up_slots.contains(&t));
        if !valid && (spectate.target.is_none() || spectate.since_check >= SPECTATE_CHECK_SECS) {
            spectate.since_check = 0.0;
            spectate.target = up_slots.first().copied();
        }
        let pad = pads.seat(seat);
        let step = if pad.active || seat == 1 {
            i32::from(pad.pressed(crate::pad::Button::LB))
                - i32::from(pad.pressed(crate::pad::Button::RB))
        } else {
            i32::from(keys.just_pressed(KeyCode::BracketRight))
                - i32::from(keys.just_pressed(KeyCode::BracketLeft))
        };
        if step != 0 && !up_slots.is_empty() {
            let at = spectate
                .target
                .and_then(|t| up_slots.iter().position(|s| *s == t))
                .unwrap_or(0) as i32;
            let n = up_slots.len() as i32;
            spectate.target = Some(up_slots[(at + step).rem_euclid(n) as usize]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_times_by_difficulty() {
        let secs: Vec<f32> = DIFFICULTY_DAMAGE
            .iter()
            .map(|k| DOWNED_HEALTH / (DOWNED_DRAIN * k))
            .collect();
        assert!((secs[0] - 42.857).abs() < 0.01);
        assert!((secs[1] - 30.0).abs() < 1e-4);
        assert!((secs[2] - 20.0).abs() < 1e-4);
    }

    #[test]
    fn snapshot_round_trip() {
        let mut r = Revive::default();
        r.downed = Some(Downed {
            left: 300.0,
            age: 4.5,
            reviver: Some(2),
            pivot: Some(Turn {
                left: true,
                from: 0.0,
                t: 0.4,
            }),
        });
        let s = r.snap();
        let mut copy = Revive::default();
        copy.follow(&s, 1.0);
        let d = copy.downed.expect("downed");
        assert_eq!(d.reviver, Some(2));
        assert!((d.left - 300.0).abs() < 1e-4);
        assert!(d.pivot.is_some_and(|p| p.left && (p.t - 0.4).abs() < 1e-4));
        assert!(copy.locked() && copy.down());
    }
}
