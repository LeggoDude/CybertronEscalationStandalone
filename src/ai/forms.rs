//! A transforming enemy's two forms (TnPawn's _RobotForm and _AltForm, as its
//! TnTransformer blueprint builds them: the Snipers' flying robot and their insect
//! on its perch, the jets' plane and robot), and its changes between them
//! (TnPawn.Transform: TnRobotToVehicleTransformation, TnVehicleToRobotTransformation).
//!
//! * **Its forms**: the one it is in (_CurrentForm: the one it changes to from the
//!   change's start, as TnTransformation.Execute activates it: its mesh is the
//!   pawn's from then on, its cylinder the pawn's) and the one it wants
//!   (_TargetForm); each has its own anim tree and clips (its own set:
//!   ai_anims_<set>.txt) and its own animator. The pawn's are the form it is in;
//!   the other's are kept here ([`OtherForm`]).
//! * **A change**: each form plays its half in its full body slot (TnTransformation's
//!   EnqueueLocalAnimation: the form it leaves blends its half in over BlendInTime,
//!   the form it takes blends its own out over BlendOutTime at its end), for as long
//!   as the shorter half lasts. Their TnAnimNotify_ToggleHidden notifies hide the
//!   one and show the other partway; as it ends, the form it is in is shown and the
//!   other hidden (InRobotForm's and InAltForm's BeginState), and its
//!   TransformComplete delegate is called. A change asked for during one follows it
//!   (InAltForm.BeginState: the target form checked again).

use std::sync::Arc;

use super::anim::{self, AnimParams, Animator, Callback, ClipBook};
use super::clips::{ClipNotify, Clips};
use super::pawn::Pawn;
use super::tree::{AnimTree, Pose};

/// ETransformerForm.
pub const ROBOT: u8 = 0;
pub const ALT: u8 = 1;

/// TnTransformation's _BlendInTime and _BlendOutTime (its defaults: no pawn class
/// gives its transformations others).
const BLEND_IN: f32 = 0.115;
const BLEND_OUT: f32 = 0.25;

/// The halves of a change to `to`: the form it leaves plays the first (its
/// _SourceAnimationName), the form it takes the second (_TargetAnimationName).
pub fn halves(to: u8) -> [&'static str; 2] {
    if to == ALT {
        ["Transform_ToVehicle_ROBO", "Transform_ToVehicle_VEH"]
    } else {
        ["Transform_ToRobot_VEH", "Transform_ToRobot_ROBO"]
    }
}

/// A form's movement capabilities as the pawn takes them up (SetMovementCapabilities:
/// its cylinder, uu).
#[derive(Clone, Copy, Debug, Default)]
pub struct FormCaps {
    pub radius: f32,
    pub half_height: f32,
}

/// A change under way (TnTransformation): the form it changes to, and its time left
/// of its duration.
#[derive(Clone, Copy, Debug)]
pub struct Change {
    pub to: u8,
    pub left: f32,
    pub duration: f32,
}

/// The form it isn't in: its clips, its animator, its anim tree and the pose they
/// made (its model plays its half of a change with them).
#[derive(Clone, Debug, Default)]
pub struct OtherForm {
    pub clips: Option<Arc<Clips>>,
    pub anim: Animator,
    pub tree: AnimTree,
    pub pose: Pose,
}

/// A transforming type's forms as its AI comes in: the form it starts in (its
/// type's own model's) and each form's cylinder (by ETransformerForm).
#[derive(Clone, Copy, Debug)]
pub struct Setup {
    pub start: u8,
    pub caps: [FormCaps; 2],
}

/// A transforming pawn's forms.
#[derive(Clone, Debug, Default)]
pub struct Forms {
    /// The form of its type's own model (the one it starts in: the other's is its
    /// other form's model, enemies.rs's FormModel).
    pub main: u8,
    /// _CurrentForm's TransformerForm and _TargetForm.
    pub current: u8,
    pub target: u8,
    pub change: Option<Change>,
    /// A change asked for (Transform) that begins with the pawn's frame.
    pending: bool,
    /// Each form's model hidden (by ETransformerForm: the robot's, the alt's).
    pub hidden: [bool; 2],
    /// Its alt form's effects allowed (_FxAllowed: TnAnimNotify_ToggleVehicleFx).
    pub fx_allowed: bool,
    /// Each form's cylinder.
    pub caps: [FormCaps; 2],
    pub other: OtherForm,
    /// The TransformComplete delegate (_TransformCompleteEvent).
    complete: Option<Callback>,
}

impl Forms {
    /// A pawn's forms as it comes in: in its `start` form (its starting form's model
    /// shown, the other's hidden), the other's clips `other_clips`.
    pub fn new(start: u8, caps: [FormCaps; 2], other_clips: Arc<Clips>) -> Forms {
        let mut anim = Animator::default();
        anim.set_slots(other_clips.slots);
        Forms {
            main: start,
            current: start,
            target: start,
            change: None,
            pending: false,
            hidden: [start != ROBOT, start != ALT],
            fx_allowed: true,
            caps,
            other: OtherForm {
                clips: Some(other_clips),
                anim,
                ..Default::default()
            },
            complete: None,
        }
    }

    /// get_IsTransforming (and a change asked for this frame).
    pub fn transforming(&self) -> bool {
        self.pending || self.change.is_some()
    }
}

/// get_TransformerForm: the form it is in (the robot's, for a pawn of one form).
pub fn form(p: &Pawn) -> u8 {
    p.forms.as_ref().map_or(ROBOT, |f| f.current)
}

/// get_IsTransforming.
pub fn is_transforming(p: &Pawn) -> bool {
    p.forms.as_ref().is_some_and(|f| f.transforming())
}

/// TnPawn.Transform: the change to `target` (with its frame: [`begin`]), `complete`
/// called back as it ends; whether it is changing form now (not for the form it
/// is in, nor for a pawn of one form). One asked for during a change follows it.
pub fn transform(p: &mut Pawn, target: u8, complete: Option<Callback>) -> bool {
    let Some(f) = p.forms.as_mut() else {
        return false;
    };
    f.target = target;
    f.complete = complete;
    if f.change.is_none() && target != f.current {
        f.pending = true;
    }
    f.transforming()
}

/// BeginTransformation, for a change asked for (or due after the last one): the
/// forms' animators, anim trees and poses swapped (the pawn's are the form it takes
/// from now on, as its clips `clips` are), each form's half played, its cylinder the
/// new form's (SetMovementCapabilities).
pub fn begin(p: &mut Pawn, clips: &mut Arc<Clips>) {
    let Some(f) = p.forms.as_mut() else {
        return;
    };
    if !f.pending || f.change.is_some() {
        return;
    }
    f.pending = false;
    let to = f.target;
    if to == f.current {
        return;
    }
    let Some(theirs) = f.other.clips.take() else {
        return;
    };
    f.other.clips = Some(std::mem::replace(clips, theirs));
    std::mem::swap(&mut p.anim, &mut f.other.anim);
    std::mem::swap(&mut p.tree, &mut f.other.tree);
    std::mem::swap(&mut p.pose, &mut f.other.pose);
    // (The callbacks its operators are owed stay with the pawn.)
    let mut owed = std::mem::take(&mut f.other.anim.fired);
    p.anim.fired.append(&mut owed);
    f.current = to;
    let [leave, take] = halves(to);
    let half = |name: &str, blend_in: f32, blend_out: f32| AnimParams {
        mask: anim::MASK_FULL_BODY,
        name: Arc::from(name),
        blend_in,
        blend_out,
        root_motion: anim::RMM_IGNORE,
        priority: 1,
        override_mode: anim::OVERRIDE_IF_HIGHER_OR_EQUAL,
        ..Default::default()
    };
    f.other.anim.enqueue(half(leave, BLEND_IN, 0.0), None, None);
    p.anim.enqueue(half(take, 0.0, BLEND_OUT), None, None);
    let left = f
        .other
        .clips
        .as_ref()
        .and_then(|c| c.length(leave))
        .unwrap_or(0.0);
    let taken = clips.length(take).unwrap_or(0.0);
    let duration = left.min(taken).max(0.0);
    f.change = Some(Change {
        to,
        left: duration,
        duration,
    });
    let caps = f.caps[to as usize];
    if caps.radius > 0.0 {
        p.radius = caps.radius;
        p.half_height = caps.half_height;
    }
    // A plane's form taken or left (a jet's: its plane form's rigid body flying, or
    // its robot falling where it was).
    if p.plane.is_some() {
        if to == ALT {
            super::jet::activate_plane(p);
        } else {
            super::jet::activate_robot(p);
        }
    }
}

/// A frame of a change: the form it leaves a frame on (its animator and its anim
/// tree, its half of the change: the notifies it passes, its toggles taken up here,
/// the rest for the caller), the change's countdown and its end (EndTransformation:
/// its delegate called back; the form it is in shown, the other hidden; a change
/// asked for since, begun with the next frame).
pub fn update(p: &mut Pawn, dt: f32, roll: &mut dyn FnMut() -> f32) -> Vec<ClipNotify> {
    let Some(mut f) = p.forms.take() else {
        return Vec::new();
    };
    let mut notes = Vec::new();
    if f.change.is_some() {
        let leaving = 1 - f.current;
        if let Some(c) = f.other.clips.clone() {
            std::mem::swap(&mut p.anim, &mut f.other.anim);
            p.anim.update(dt, &*c, roll);
            let mut tree = std::mem::take(&mut f.other.tree);
            f.other.pose = tree.update(&c, p, dt, roll);
            f.other.tree = tree;
            std::mem::swap(&mut p.anim, &mut f.other.anim);
            let mut owed = std::mem::take(&mut f.other.anim.fired);
            p.anim.fired.append(&mut owed);
        }
        for (_, n, _) in std::mem::take(&mut f.other.anim.notifies) {
            match n {
                ClipNotify::Toggle { hide, .. } => f.hidden[leaving as usize] = hide,
                ClipNotify::VehicleFx { allowed, .. } => f.fx_allowed = allowed,
                n => notes.push(n),
            }
        }
        let change = f.change.as_mut().expect("its change");
        change.left -= dt;
        if change.left <= 0.0 {
            f.change = None;
            if let Some(cb) = f.complete.take() {
                p.anim.fired.push(cb);
            }
            f.hidden = [f.current != ROBOT, f.current != ALT];
            if f.target != f.current {
                f.pending = true;
            }
        }
    }
    p.forms = Some(f);
    notes
}

/// TnAnimNotify_ToggleHidden on the form it is in: its model hidden or shown.
pub fn toggle(p: &mut Pawn, hide: bool) {
    if let Some(f) = p.forms.as_mut() {
        f.hidden[f.current as usize] = hide;
    }
}

/// TnAnimNotify_ToggleVehicleFx: its alt form's effects allowed or not.
pub fn vehicle_fx(p: &mut Pawn, allowed: bool) {
    if let Some(f) = p.forms.as_mut() {
        f.fx_allowed = allowed;
    }
}

/// Its death: its change cut short, the form it is in shown and the other hidden.
pub fn died(p: &mut Pawn) {
    if let Some(f) = p.forms.as_mut() {
        f.change = None;
        f.pending = false;
        f.hidden = [f.current != ROBOT, f.current != ALT];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::clips;
    use bevy::prelude::{Entity, Vec3};

    /// A frame of a pawn's forms, as the AI's frame runs them (forms' begin, its
    /// animator and anim tree, the form it leaves, the toggles of the form it is in).
    fn frame(p: &mut Pawn, clips: &mut Arc<Clips>, dt: f32) {
        begin(p, clips);
        let c = clips.clone();
        p.anim.update(dt, &*c, &mut || 0.5);
        let mut tree = std::mem::take(&mut p.tree);
        p.pose = tree.update(&c, p, dt, &mut || 0.5);
        p.tree = tree;
        update(p, dt, &mut || 0.5);
        for (_, n, _) in std::mem::take(&mut p.anim.notifies) {
            if let ClipNotify::Toggle { hide, .. } = n {
                toggle(p, hide);
            }
        }
    }

    #[test]
    fn a_sniper_changes_form() {
        let (Some(robot), Some(insect)) = (
            clips::for_type("Sniper_AUT"),
            clips::for_type("Sniper_AUT~other"),
        ) else {
            eprintln!("the Snipers' clips aren't built: skipped");
            return;
        };
        let mut p = Pawn::new(
            Entity::from_raw_u32(1).unwrap(),
            Vec3::ZERO,
            0.0,
            175.0,
            200.0,
            200.0,
        );
        p.anim.set_slots(robot.slots);
        let caps = [
            FormCaps {
                radius: 175.0,
                half_height: 200.0,
            },
            FormCaps {
                radius: 200.0,
                half_height: 200.0,
            },
        ];
        p.forms = Some(Box::new(Forms::new(ROBOT, caps, insect.clone())));
        let mut clips = robot.clone();
        let dt = 1.0 / 60.0;
        for _ in 0..10 {
            frame(&mut p, &mut clips, dt);
        }
        let f = p.forms.as_ref().unwrap();
        assert_eq!((f.current, f.hidden), (ROBOT, [false, true]));
        // Into its insect: its forms' halves play, the robot hidden and the insect
        // shown partway, the callback as it ends.
        let cb = Callback { owner: 7, kind: 3 };
        assert!(transform(&mut p, ALT, Some(cb)));
        assert!(is_transforming(&p));
        let mut t = 0.0;
        let mut robot_hidden_at = None;
        let mut insect_shown_at = None;
        let mut ended_at = None;
        let mut first = true;
        while t < 2.0 {
            frame(&mut p, &mut clips, dt);
            t += dt;
            let f = p.forms.as_ref().unwrap();
            if first {
                first = false;
                assert_eq!(f.current, ALT, "the form it takes from the start");
                assert!(
                    Arc::ptr_eq(&clips, &insect),
                    "the insect's clips the pawn's"
                );
                assert!(p.anim.is_playing_named("Transform_ToVehicle_VEH"));
                assert!(f.other.anim.is_playing_named("Transform_ToVehicle_ROBO"));
                assert_eq!(p.radius, 200.0);
            }
            if f.hidden[ROBOT as usize] && robot_hidden_at.is_none() {
                robot_hidden_at = Some(t);
            }
            if !f.hidden[ALT as usize] && insect_shown_at.is_none() {
                insect_shown_at = Some(t);
            }
            if f.change.is_none() && ended_at.is_none() {
                ended_at = Some(t);
                assert!(p.anim.fired.contains(&cb), "its delegate called back");
            }
        }
        let (rh, is, end) = (
            robot_hidden_at.expect("the robot hidden"),
            insect_shown_at.expect("the insect shown"),
            ended_at.expect("the change over"),
        );
        assert!((rh - 0.44).abs() < 0.05, "robot hidden at {rh}");
        assert!((is - 0.35).abs() < 0.05, "insect shown at {is}");
        assert!((end - 1.167).abs() < 0.05, "over at {end}");
        let f = p.forms.as_ref().unwrap();
        assert_eq!((f.current, f.hidden), (ALT, [true, false]));
        assert!(!is_transforming(&p));
        // And back: one asked for during it follows it.
        p.anim.fired.clear();
        assert!(transform(&mut p, ROBOT, None));
        for _ in 0..10 {
            frame(&mut p, &mut clips, dt);
        }
        assert!(
            transform(&mut p, ALT, None),
            "a change asked for during one"
        );
        let mut back_to_alt = false;
        for _ in 0..200 {
            frame(&mut p, &mut clips, dt);
            let f = p.forms.as_ref().unwrap();
            if f.current == ALT && f.change.is_some() {
                back_to_alt = true;
            }
        }
        assert!(
            back_to_alt,
            "the change asked for during the last followed it"
        );
        let f = p.forms.as_ref().unwrap();
        assert_eq!(
            (f.current, f.hidden, f.change.is_none()),
            (ALT, [true, false], true)
        );
        assert!(Arc::ptr_eq(&clips, &insect));
    }
}
