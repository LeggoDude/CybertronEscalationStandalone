//! Fall of Cybertron's HTN planner (High Moon's Htn package: Htn, HtnPlanningDomain,
//! HtnPlanBuilder, HtnPlanner, HtnPlanRunner's bookkeeping), as its script has it.
//!
//! * **The world state**: 50 attributes (AI.EWorldState), each a value and a
//!   context. A precondition compares an attribute's value and its context with
//!   its data sources (Htn.CompareValues: ==, !=, <=, <, >=, >, a bit set or not,
//!   bits set or not, an enum's bit in a mask, true) and combines the two (and, or);
//!   an effect sets, adds, subtracts, sets a bit or clears one, on both.
//! * **A data source** is a constant (0 or more) or a parameter of its task
//!   (-(index + 1): Htn.ResolveDataSource), already resolved in the cooked domains.
//! * **Planning** (HtnPlanBuilder): the goal task on the pending stack; each step
//!   pops a task: a compound one is decomposed by its first method (from the start
//!   method index) whose preconditions hold, its subtasks pushed (their parameters
//!   resolved against it), the decomposition recorded (up to 10); a primitive one
//!   needs its operator's preconditions, applies its effects (recorded against the
//!   last decomposition) and is resolved. A failed step rolls back to the last
//!   decomposition, which tries its next method. 50 steps at most. The plan's
//!   priority is each decomposition's method index shifted into its own 3 bits,
//!   from the top (MethodIndex << (27 - 3 x i)); lower is better.

use std::fmt;

/// The world state's attributes (AI.EWorldState).
pub const WS_NUM: usize = 50;

pub const WS_CURRENT_TASK: usize = 0;
pub const WS_CURRENT_FORM: usize = 1;
pub const WS_HEARD_NOISE: usize = 2;
pub const WS_ENEMY_VISIBLE: usize = 3;
pub const WS_ENEMY_RANGE: usize = 4;
pub const WS_ENEMY_HEALTH: usize = 5;
pub const WS_ENEMY_AIRBORNE: usize = 6;
pub const WS_ENEMY_TRANSFORMING: usize = 7;
pub const WS_ENEMY_FORM: usize = 8;
pub const WS_ENEMY_WEAKENED: usize = 9;
pub const WS_ENEMY_PHYSICS: usize = 10;
pub const WS_ENEMY_MOVEMENT: usize = 11;
pub const WS_PATROL_AVAILABLE: usize = 12;
pub const WS_SEARCH_AVAILABLE: usize = 13;
pub const WS_ATTACK_POINT_AVAILABLE: usize = 14;
pub const WS_SPECIAL_ATTACK_AVAILABLE: usize = 15;
pub const WS_ATTACK_READY: usize = 16;
pub const WS_HURT_BY_ENEMY_RECENTLY: usize = 17;
pub const WS_WEAPON_EQUIPPED: usize = 18;
pub const WS_WEAPON_HAS_CLEAR_LINE_OF_FIRE: usize = 19;
pub const WS_CLEAR_ATTACK_CHANNEL: usize = 20;
pub const WS_IS_IN_PAIN: usize = 21;
pub const WS_ON_FIRE: usize = 22;
pub const WS_FRUSTRATED: usize = 23;
pub const WS_TIRED: usize = 24;
pub const WS_SQUAD_COMMAND: usize = 25;
pub const WS_PATH_BLOCKED: usize = 26;
pub const WS_BALANCED: usize = 27;
pub const WS_ON_NAV_MESH: usize = 28;
pub const WS_AREA_CLEAR: usize = 29;
pub const WS_ACROBATICS: usize = 30;
pub const WS_DEFEND_ACTOR_RANGE: usize = 31;
pub const WS_HEALTH: usize = 32;
pub const WS_ENEMY_LOCATION_SUSPICIOUS: usize = 33;
pub const WS_HAS_AMMO: usize = 34;
pub const WS_SUSPICION: usize = 35;
pub const WS_LOCATION: usize = 36;
pub const WS_VULNERABILITY: usize = 37;
pub const WS_COVER_USAGE: usize = 38;
pub const WS_ALERT_LEVEL: usize = 39;
pub const WS_ASLEEP: usize = 40;
pub const WS_KISMET_BEHAVIOR: usize = 41;
pub const WS_AVAILABLE_SQUAD_ACTIONS: usize = 42;
pub const WS_ALERT_TO_ENEMY_PRESENCE: usize = 43;
pub const WS_NAVIGATION: usize = 44;
pub const WS_DANGER_CLOSE: usize = 45;
pub const WS_DAMAGED: usize = 46;
pub const WS_LOS_SEARCH_AVAILABLE: usize = 47;
pub const WS_CUSTOM: usize = 48;

/// WS_DangerClose's bit for a grenade near (AI.EDangerClose's DC_Grenade).
pub const DC_GRENADE: i32 = 0;

/// An attribute: its value and its context.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ws {
    pub value: i32,
    pub context: i32,
}

pub type WorldState = [Ws; WS_NUM];

/// Htn.SetBit.
pub fn set_bit(current: i32, bit: i32, enable: bool) -> i32 {
    let mask = 1i32.wrapping_shl(bit as u32);
    if enable {
        current | mask
    } else {
        current & !mask
    }
}

/// Htn.EHtnComparisonOperator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Equal,
    NotEqual,
    LessOrEqual,
    Less,
    GreaterOrEqual,
    Greater,
    BitIsSet,
    BitIsNotSet,
    BitsAreSet,
    BitsAreNotSet,
    AnyEnumSet,
    True,
}

impl Cmp {
    fn from(n: i32) -> Cmp {
        match n {
            0 => Cmp::Equal,
            1 => Cmp::NotEqual,
            2 => Cmp::LessOrEqual,
            3 => Cmp::Less,
            4 => Cmp::GreaterOrEqual,
            5 => Cmp::Greater,
            6 => Cmp::BitIsSet,
            7 => Cmp::BitIsNotSet,
            8 => Cmp::BitsAreSet,
            9 => Cmp::BitsAreNotSet,
            10 => Cmp::AnyEnumSet,
            _ => Cmp::True,
        }
    }

    /// Htn.CompareValues.
    pub fn test(self, lhs: i32, rhs: i32) -> bool {
        match self {
            Cmp::Equal => lhs == rhs,
            Cmp::NotEqual => lhs != rhs,
            Cmp::LessOrEqual => lhs <= rhs,
            Cmp::Less => lhs < rhs,
            Cmp::GreaterOrEqual => lhs >= rhs,
            Cmp::Greater => lhs > rhs,
            Cmp::BitIsSet => lhs & 1i32.wrapping_shl(rhs as u32) != 0,
            Cmp::BitIsNotSet => lhs & 1i32.wrapping_shl(rhs as u32) == 0,
            Cmp::BitsAreSet => lhs & rhs != 0,
            Cmp::BitsAreNotSet => lhs & rhs == 0,
            Cmp::AnyEnumSet => 1i32.wrapping_shl(lhs as u32) & rhs != 0,
            Cmp::True => true,
        }
    }
}

/// Htn.EHtnEffectOperator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffOp {
    Set,
    Add,
    Subtract,
    SetBit,
    ClearBit,
}

impl EffOp {
    fn from(n: i32) -> EffOp {
        match n {
            1 => EffOp::Add,
            2 => EffOp::Subtract,
            3 => EffOp::SetBit,
            4 => EffOp::ClearBit,
            _ => EffOp::Set,
        }
    }

    /// Htn.GetEffectResult.
    pub fn apply(self, old: i32, new: i32) -> i32 {
        match self {
            EffOp::Set => new,
            EffOp::Add => old.wrapping_add(new),
            EffOp::Subtract => old.wrapping_sub(new),
            EffOp::SetBit => set_bit(old, new, true),
            EffOp::ClearBit => set_bit(old, new, false),
        }
    }
}

/// The most parameters a task has (Htn.kMaxNumParameters).
pub const MAX_PARAMS: usize = 5;

/// A task (Htn.HtnTask): its symbol and its parameters' values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Task {
    pub symbol: i32,
    pub n: u8,
    pub params: [i32; MAX_PARAMS],
}

impl Task {
    pub fn new(symbol: i32, params: &[i32]) -> Task {
        let mut p = [0; MAX_PARAMS];
        for (i, v) in params.iter().take(MAX_PARAMS).enumerate() {
            p[i] = *v;
        }
        Task {
            symbol,
            n: params.len().min(MAX_PARAMS) as u8,
            params: p,
        }
    }

    /// Htn.GetIntParameterValue.
    pub fn int(&self, i: usize) -> i32 {
        self.params.get(i).copied().unwrap_or(0)
    }

    /// Htn.GetFloatParameterValue (Htn.UnpackFloat: thousandths).
    pub fn float(&self, i: usize) -> f32 {
        self.int(i) as f32 * 0.001
    }

    /// Htn.GetBoolParameterValue.
    pub fn bool(&self, i: usize) -> bool {
        self.int(i) == 1
    }
}

/// Htn.PackFloat.
pub fn pack_float(v: f32) -> i32 {
    (v * 1000.0) as i32
}

/// Htn.ResolveDataSource.
pub fn resolve(task: &Task, source: i32) -> i32 {
    if source >= 0 {
        source
    } else {
        task.int((-(source + 1)) as usize)
    }
}

/// A precondition (Htn.HtnPreconditionDescription).
#[derive(Clone, Copy, Debug)]
pub struct Pre {
    pub attr: usize,
    pub cmp: Cmp,
    pub value: i32,
    pub ctx_cmp: Cmp,
    pub ctx: i32,
    pub or: bool,
}

impl Pre {
    /// Htn.PreconditionIsSatisfied.
    pub fn holds(&self, task: &Task, ws: &WorldState) -> bool {
        let w = ws[self.attr];
        let v = self.cmp.test(w.value, resolve(task, self.value));
        let c = self.ctx_cmp.test(w.context, resolve(task, self.ctx));
        if self.or { v || c } else { v && c }
    }
}

/// An effect (Htn.HtnEffectDescription).
#[derive(Clone, Copy, Debug)]
pub struct Eff {
    pub attr: usize,
    pub op: EffOp,
    pub value: i32,
    pub ctx_op: EffOp,
    pub ctx: i32,
    /// WillBeSetExternally: the operator's sensors set it; the plan only expects it.
    pub external: bool,
}

impl Eff {
    pub fn apply(&self, task: &Task, ws: &mut WorldState) {
        let w = &mut ws[self.attr];
        w.value = self.op.apply(w.value, resolve(task, self.value));
        w.context = self.ctx_op.apply(w.context, resolve(task, self.ctx));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskKind {
    Primitive,
    Compound,
}

/// A task's description.
#[derive(Clone, Debug)]
pub struct TaskDesc {
    pub name: String,
    pub kind: TaskKind,
    pub params: Vec<String>,
    pub defaults: Vec<i32>,
}

/// An operator's description: the class that runs the task.
#[derive(Clone, Debug)]
pub struct OpDesc {
    pub class: String,
    pub benefit: i32,
    pub flags: u32,
    pub pre: Vec<Pre>,
    pub eff: Vec<Eff>,
}

/// A method of a compound task.
#[derive(Clone, Debug, Default)]
pub struct Method {
    pub pre: Vec<Pre>,
    pub tasks: Vec<Task>,
}

/// A planning domain (HtnPlanningDomain).
#[derive(Clone, Debug, Default)]
pub struct Domain {
    pub class: String,
    pub tasks: Vec<Option<TaskDesc>>,
    pub ops: Vec<Option<OpDesc>>,
    pub methods: Vec<Vec<Method>>,
    pub used: Vec<bool>,
    pub resetting: Vec<(usize, Ws)>,
    pub debug_flags: i32,
}

impl Domain {
    fn grow(&mut self, symbol: usize) {
        if self.tasks.len() <= symbol {
            self.tasks.resize(symbol + 1, None);
            self.ops.resize(symbol + 1, None);
            self.methods.resize(symbol + 1, Vec::new());
        }
    }

    /// Reads a domain's lines (tools/build_ai.py's ai.txt, after its "domain" line).
    pub fn parse<'a>(class: &str, lines: impl Iterator<Item = &'a str>) -> Domain {
        let mut d = Domain {
            class: class.to_string(),
            used: vec![false; WS_NUM],
            ..Default::default()
        };
        enum Into {
            Nothing,
            Op(usize),
            Method(usize),
        }
        let mut into = Into::Nothing;
        let num = |s: &str| s.trim().parse::<i32>().unwrap_or(0);
        for line in lines {
            let r: Vec<&str> = line.split('\t').collect();
            match (r[0], r.len()) {
                ("task", 6..) => {
                    let s = num(r[1]).max(0) as usize;
                    d.grow(s);
                    let list = |x: &str| -> Vec<String> {
                        if x == "-" {
                            Vec::new()
                        } else {
                            x.split(',').map(|v| v.to_string()).collect()
                        }
                    };
                    d.tasks[s] = Some(TaskDesc {
                        name: r[2].to_string(),
                        kind: if r[3] == "compound" {
                            TaskKind::Compound
                        } else {
                            TaskKind::Primitive
                        },
                        params: list(r[4]),
                        defaults: list(r[5]).iter().map(|v| num(v)).collect(),
                    });
                    into = Into::Nothing;
                }
                ("op", 5..) => {
                    let s = num(r[1]).max(0) as usize;
                    d.grow(s);
                    d.ops[s] = Some(OpDesc {
                        class: r[2].to_string(),
                        benefit: num(r[3]),
                        flags: num(r[4]) as u32,
                        pre: Vec::new(),
                        eff: Vec::new(),
                    });
                    into = Into::Op(s);
                }
                ("method", 3..) => {
                    let s = num(r[1]).max(0) as usize;
                    d.grow(s);
                    d.methods[s].push(Method::default());
                    into = Into::Method(s);
                }
                ("pre", 7..) => {
                    let p = Pre {
                        attr: (num(r[1]).max(0) as usize).min(WS_NUM - 1),
                        cmp: Cmp::from(num(r[2])),
                        value: num(r[3]),
                        ctx_cmp: Cmp::from(num(r[4])),
                        ctx: num(r[5]),
                        or: r[6].trim() == "or",
                    };
                    match into {
                        Into::Op(s) => {
                            if let Some(o) = d.ops[s].as_mut() {
                                o.pre.push(p)
                            }
                        }
                        Into::Method(s) => {
                            if let Some(m) = d.methods[s].last_mut() {
                                m.pre.push(p)
                            }
                        }
                        Into::Nothing => {}
                    }
                }
                ("eff", 7..) => {
                    if let Into::Op(s) = into
                        && let Some(o) = d.ops[s].as_mut()
                    {
                        o.eff.push(Eff {
                            attr: (num(r[1]).max(0) as usize).min(WS_NUM - 1),
                            op: EffOp::from(num(r[2])),
                            value: num(r[3]),
                            ctx_op: EffOp::from(num(r[4])),
                            ctx: num(r[5]),
                            external: r[6].trim() == "1",
                        });
                    }
                }
                ("sub", 2..) => {
                    if let Into::Method(s) = into
                        && let Some(m) = d.methods[s].last_mut()
                    {
                        let vals: Vec<i32> = r[2..].iter().map(|v| num(v)).collect();
                        m.tasks.push(Task::new(num(r[1]), &vals));
                    }
                }
                ("used", _) => {
                    for a in &r[1..] {
                        let a = num(a);
                        if (0..WS_NUM as i32).contains(&a) {
                            d.used[a as usize] = true;
                        }
                    }
                }
                ("reset", 4..) => d.resetting.push((
                    (num(r[1]).max(0) as usize).min(WS_NUM - 1),
                    Ws {
                        value: num(r[2]),
                        context: num(r[3]),
                    },
                )),
                ("debugflags", 2..) => d.debug_flags = num(r[1]),
                _ => {}
            }
        }
        d
    }

    pub fn task(&self, symbol: i32) -> Option<&TaskDesc> {
        usize::try_from(symbol)
            .ok()
            .and_then(|s| self.tasks.get(s))
            .and_then(|t| t.as_ref())
    }

    pub fn op(&self, symbol: i32) -> Option<&OpDesc> {
        usize::try_from(symbol)
            .ok()
            .and_then(|s| self.ops.get(s))
            .and_then(|t| t.as_ref())
    }

    pub fn symbol_named(&self, name: &str) -> Option<i32> {
        self.tasks
            .iter()
            .position(|t| t.as_ref().is_some_and(|t| t.name == name))
            .map(|i| i as i32)
    }

    pub fn name(&self, symbol: i32) -> &str {
        self.task(symbol).map(|t| t.name.as_str()).unwrap_or("?")
    }

    /// HtnPlanningDomain.IsWorldStateUsed.
    pub fn is_used(&self, attr: usize) -> bool {
        self.used.get(attr).copied().unwrap_or(false)
    }

    /// A task of `symbol` with its parameters' defaults (as Kismet and the alerter
    /// make them), `set` over them by name.
    pub fn make_task(&self, symbol: i32, set: &[(&str, i32)]) -> Task {
        let Some(desc) = self.task(symbol) else {
            return Task::new(symbol, &[]);
        };
        let mut vals = desc.defaults.clone();
        vals.resize(desc.params.len(), 0);
        for (name, v) in set {
            if let Some(i) = desc.params.iter().position(|p| p == name) {
                vals[i] = *v;
            }
        }
        Task::new(symbol, &vals)
    }

    /// HtnPlanningDomain.SetResettingWorldStatesBackToDefault.
    pub fn reset_world_states(&self, ws: &mut WorldState) {
        for (attr, w) in &self.resetting {
            ws[*attr] = *w;
        }
    }

    /// The plan's benefit (HtnPlanRunner.GetPlanBenefit).
    pub fn benefit(&self, plan: &[Task]) -> i32 {
        plan.iter()
            .map(|t| self.op(t.symbol).map_or(0, |o| o.benefit))
            .sum()
    }
}

// ---------------------------------------------------------------------------
// A domain built in code
// ---------------------------------------------------------------------------

/// HtnPlanningDomainBuilder (as HtnGlobalPlanningDomainBuilder hands it out): a
/// planning domain built by script, as a character's OnBuildPlanningDomain builds
/// its own (the Titan's drones': theirs aren't cooked). A task is registered (with
/// the parameters declared so far) as its first operator or method begins
/// (SetActiveTaskType); a precondition goes to the method being built, else to
/// the operator; a variable's value is its parameter's index in the task's own
/// parameters, packed (ResolveVariable: -(index + 1)); a sub task starts from its
/// task's parameter defaults (BeginSubTask). The domain's used world states are
/// its operators' preconditions' and effects' and its methods' preconditions'
/// (BuildUsedWorldStates).
pub struct DomainBuilder {
    d: Domain,
    names: Vec<String>,
    /// The task being built: its symbol, its description, and whether it has
    /// been registered (as which kind).
    task: Option<(usize, TaskDesc, Option<TaskKind>)>,
    op: Option<OpDesc>,
    method: Option<Method>,
    sub: Option<Task>,
}

impl DomainBuilder {
    /// HtnPlanningDomain.Initialize (a slot for each of the task symbols, named
    /// by their enum) and BeginDomain.
    pub fn new(class: &str, names: &[&str]) -> DomainBuilder {
        let n = names.len();
        DomainBuilder {
            d: Domain {
                class: class.to_string(),
                tasks: vec![None; n],
                ops: vec![None; n],
                methods: vec![Vec::new(); n],
                used: vec![false; WS_NUM],
                resetting: Vec::new(),
                debug_flags: 0,
            },
            names: names.iter().map(|s| s.to_string()).collect(),
            task: None,
            op: None,
            method: None,
            sub: None,
        }
    }

    /// PlanningDomain.DebugFlags |= flags (kUsePriority: 1).
    pub fn debug_flags(&mut self, flags: i32) {
        self.d.debug_flags |= flags;
    }

    /// BeginTask.
    pub fn begin_task(&mut self, symbol: usize) {
        let name = self
            .names
            .get(symbol)
            .cloned()
            .unwrap_or_else(|| symbol.to_string());
        self.task = Some((
            symbol,
            TaskDesc {
                name,
                kind: TaskKind::Primitive,
                params: Vec::new(),
                defaults: Vec::new(),
            },
            None,
        ));
    }

    /// EndTask.
    pub fn end_task(&mut self) {
        self.task = None;
    }

    /// DeclareParameter.
    pub fn declare(&mut self, name: &str, default: i32) {
        if let Some((_, t, _)) = self.task.as_mut()
            && t.params.len() < MAX_PARAMS
        {
            t.params.push(name.to_string());
            t.defaults.push(default);
        }
    }

    /// SetActiveTaskType: the task registered as `kind` the first time.
    fn set_kind(&mut self, kind: TaskKind) {
        let Some((symbol, t, registered)) = self.task.as_mut() else {
            return;
        };
        if *registered == Some(kind) {
            return;
        }
        *registered = Some(kind);
        t.kind = kind;
        if let Some(slot) = self.d.tasks.get_mut(*symbol) {
            *slot = Some(t.clone());
        }
    }

    /// BeginOperator.
    pub fn begin_operator(&mut self, class: &str) {
        self.set_kind(TaskKind::Primitive);
        self.op = Some(OpDesc {
            class: class.to_string(),
            benefit: 0,
            flags: 0,
            pre: Vec::new(),
            eff: Vec::new(),
        });
    }

    /// EndOperator (RegisterOperator).
    pub fn end_operator(&mut self) {
        let (Some(op), Some((symbol, ..))) = (self.op.take(), self.task.as_ref()) else {
            return;
        };
        if let Some(slot) = self.d.ops.get_mut(*symbol)
            && slot.is_none()
        {
            *slot = Some(op);
        }
    }

    /// AddOperator: one with nothing of its own.
    pub fn add_operator(&mut self, class: &str) {
        self.begin_operator(class);
        self.end_operator();
    }

    /// AddBenefit.
    pub fn benefit(&mut self, benefit: i32) {
        if let Some(o) = self.op.as_mut() {
            o.benefit = benefit;
        }
    }

    /// AddBehavior.
    pub fn behavior(&mut self, flags: u32) {
        if let Some(o) = self.op.as_mut() {
            o.flags = flags;
        }
    }

    /// AddPrecondition with HtnGlobalPlanningDomainBuilder's defaults: its value
    /// compared by `cmp`, its context always true, and'ed.
    pub fn pre(&mut self, attr: usize, value: i32, cmp: i32) {
        self.pre_full(attr, value, cmp, 0, 11, false);
    }

    /// AddPrecondition (MakePreconditionConstantDescription).
    pub fn pre_full(
        &mut self,
        attr: usize,
        value: i32,
        cmp: i32,
        ctx: i32,
        ctx_cmp: i32,
        or: bool,
    ) {
        let p = Pre {
            attr: attr.min(WS_NUM - 1),
            cmp: Cmp::from(cmp),
            value,
            ctx_cmp: Cmp::from(ctx_cmp),
            ctx,
            or,
        };
        if let Some(m) = self.method.as_mut() {
            m.pre.push(p);
        } else if let Some(o) = self.op.as_mut() {
            o.pre.push(p);
        }
    }

    /// The packed index of one of the task's own parameters (ResolveVariable).
    fn variable(&self, name: &str) -> i32 {
        self.task
            .as_ref()
            .and_then(|(_, t, _)| t.params.iter().position(|p| p == name))
            .map_or(0, |i| -(i as i32 + 1))
    }

    fn effect(&mut self, attr: usize, value: i32, external: bool) {
        if let Some(o) = self.op.as_mut() {
            o.eff.push(Eff {
                attr: attr.min(WS_NUM - 1),
                op: EffOp::Set,
                value,
                ctx_op: EffOp::Set,
                ctx: 0,
                external,
            });
        }
    }

    /// AddEffect: the attribute set to the value (its context to 0).
    pub fn eff(&mut self, attr: usize, value: i32) {
        self.effect(attr, value, false);
    }

    /// AddExpectedEffect: one its sensors will set (WillBeSetExternally).
    pub fn expected_eff(&mut self, attr: usize, value: i32) {
        self.effect(attr, value, true);
    }

    /// AddVariableEffect: the attribute set to one of the task's parameters.
    pub fn variable_eff(&mut self, attr: usize, name: &str) {
        let v = self.variable(name);
        self.effect(attr, v, false);
    }

    /// BeginMethod.
    pub fn begin_method(&mut self) {
        self.set_kind(TaskKind::Compound);
        self.method = Some(Method::default());
    }

    /// EndMethod (RegisterMethod: eight at most).
    pub fn end_method(&mut self) {
        let (Some(m), Some((symbol, ..))) = (self.method.take(), self.task.as_ref()) else {
            return;
        };
        if let Some(list) = self.d.methods.get_mut(*symbol)
            && list.len() < 8
        {
            list.push(m);
        }
    }

    /// BeginSubTask: its parameters as its task's defaults.
    pub fn begin_sub(&mut self, symbol: usize) {
        let defaults = self
            .d
            .tasks
            .get(symbol)
            .and_then(|t| t.as_ref())
            .map(|t| {
                let mut v = t.defaults.clone();
                v.resize(t.params.len(), 0);
                v
            })
            .unwrap_or_default();
        self.sub = Some(Task::new(symbol as i32, &defaults));
    }

    /// SetParameter: one of the sub task's parameters, by name.
    pub fn set_param(&mut self, name: &str, value: i32) {
        let Some(s) = self.sub.as_mut() else { return };
        let Some(i) = self
            .d
            .tasks
            .get(s.symbol as usize)
            .and_then(|t| t.as_ref())
            .and_then(|t| t.params.iter().position(|p| p == name))
        else {
            return;
        };
        s.params[i] = value;
    }

    /// EndSubTask.
    pub fn end_sub(&mut self) {
        if let (Some(s), Some(m)) = (self.sub.take(), self.method.as_mut()) {
            m.tasks.push(s);
        }
    }

    /// AddSubTask: one with its parameters' defaults.
    pub fn sub(&mut self, symbol: usize) {
        self.begin_sub(symbol);
        self.end_sub();
    }

    /// EndDomain, its used world states built.
    pub fn build(mut self) -> Domain {
        let mut used = vec![false; WS_NUM];
        for o in self.d.ops.iter().flatten() {
            for p in &o.pre {
                used[p.attr] = true;
            }
            for e in &o.eff {
                used[e.attr] = true;
            }
        }
        for m in self.d.methods.iter().flatten() {
            for p in &m.pre {
                used[p.attr] = true;
            }
        }
        self.d.used = used;
        self.d
    }
}

// ---------------------------------------------------------------------------
// The plan builder
// ---------------------------------------------------------------------------

/// HtnPlanBuilder.kMaxIterations.
const MAX_ITERATIONS: u32 = 50;
/// The decompositions recorded (HtnRollbackHistory's attempts).
const MAX_HISTORY: usize = 10;
/// HtnPlanner.kInvalidPlan.
pub const INVALID_PLAN: i32 = i32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanState {
    Succeeded,
    Failed,
    Building,
}

#[derive(Clone, Debug, Default)]
struct Attempt {
    compound: Task,
    method: usize,
    pending_len: usize,
    resolved_len: usize,
    changes: Vec<(usize, Ws)>,
}

/// HtnPlanBuilder's task stack (HtnTaskList: a static array and its length, so a
/// rollback's longer length brings back what was popped).
#[derive(Default)]
struct Stack {
    tasks: Vec<Task>,
    len: usize,
}

impl Stack {
    fn push(&mut self, t: Task) {
        if self.tasks.len() <= self.len {
            self.tasks.resize(self.len + 1, Task::default());
        }
        self.tasks[self.len] = t;
        self.len += 1;
    }

    fn pop(&mut self) -> Option<Task> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        Some(self.tasks[self.len])
    }

    fn set_len(&mut self, len: usize) {
        if self.tasks.len() < len {
            self.tasks.resize(len, Task::default());
        }
        self.len = len;
    }
}

/// HtnPlanBuilder.
pub struct PlanBuilder {
    resolved: Vec<Task>,
    pending: Stack,
    history: Vec<Attempt>,
    start_method: usize,
    iterations: u32,
    state: PlanState,
    ws: WorldState,
}

/// What a plan came to.
#[derive(Clone, Debug, Default)]
pub struct Built {
    pub plan: Vec<Task>,
    /// Its priority (INVALID_PLAN when none was found).
    pub priority: i32,
    pub decompositions: usize,
}

impl PlanBuilder {
    fn new(goal: Task, ws: &WorldState) -> PlanBuilder {
        let mut pending = Stack::default();
        pending.push(goal);
        PlanBuilder {
            resolved: Vec::new(),
            pending,
            history: Vec::new(),
            start_method: 0,
            iterations: 0,
            state: PlanState::Building,
            ws: *ws,
        }
    }

    fn step(&mut self, d: &Domain) {
        self.iterations += 1;
        let Some(task) = self.pending.pop() else {
            self.state = PlanState::Succeeded;
            return;
        };
        if self.process(task, d) {
            self.state = if self.pending.len == 0 {
                PlanState::Succeeded
            } else {
                PlanState::Building
            };
        } else {
            self.state = PlanState::Failed;
        }
        self.start_method = 0;
    }

    fn rollback(&mut self) -> bool {
        let Some(a) = self.history.pop() else {
            return false;
        };
        self.resolved.truncate(a.resolved_len);
        self.pending.set_len(a.pending_len.max(1));
        let top = self.pending.len - 1;
        self.pending.tasks[top] = a.compound;
        self.start_method = a.method + 1;
        for (attr, old) in a.changes.iter().rev() {
            self.ws[*attr] = *old;
        }
        self.state = PlanState::Building;
        true
    }

    fn process(&mut self, task: Task, d: &Domain) -> bool {
        match d.task(task.symbol).map(|t| t.kind) {
            Some(TaskKind::Compound) => self.process_compound(task, d),
            Some(TaskKind::Primitive) => self.process_primitive(task, d),
            None => false,
        }
    }

    fn process_compound(&mut self, task: Task, d: &Domain) -> bool {
        let Some(methods) = d.methods.get(task.symbol as usize) else {
            return false;
        };
        let Some(index) = (self.start_method..methods.len())
            .find(|&i| methods[i].pre.iter().all(|p| p.holds(&task, &self.ws)))
        else {
            return false;
        };
        // RecordDecomposition.
        if self.history.len() < MAX_HISTORY {
            self.history.push(Attempt {
                compound: task,
                method: index,
                pending_len: self.pending.len + 1,
                resolved_len: self.resolved.len(),
                changes: Vec::new(),
            });
        }
        // SubstituteTaskNetwork.
        for sub in methods[index].tasks.iter().rev() {
            let mut s = *sub;
            for j in 0..s.n as usize {
                s.params[j] = resolve(&task, s.params[j]);
            }
            self.pending.push(s);
        }
        true
    }

    fn process_primitive(&mut self, task: Task, d: &Domain) -> bool {
        let Some(op) = d.op(task.symbol) else {
            return false;
        };
        if !op.pre.iter().all(|p| p.holds(&task, &self.ws)) {
            return false;
        }
        for e in &op.eff {
            // RecordCurrentWorldState.
            if let Some(a) = self.history.last_mut() {
                a.changes.push((e.attr, self.ws[e.attr]));
            }
            e.apply(&task, &mut self.ws);
        }
        self.resolved.push(task);
        true
    }

    /// CalculatePriority.
    fn priority(&self) -> i32 {
        let mut p: i32 = 0;
        for (i, a) in self.history.iter().enumerate() {
            let shift = 27 - 3 * i as i32;
            if shift >= 0 {
                p |= (a.method as i32).wrapping_shl(shift as u32);
            } else {
                p |= (a.method as i32) >> (-shift);
            }
        }
        p
    }
}

/// HtnPlanner.BuildPlan: a plan for `goal` from `ws`.
pub fn build_plan(d: &Domain, goal: Task, ws: &WorldState) -> Built {
    let mut b = PlanBuilder::new(goal, ws);
    while b.state == PlanState::Building && b.iterations < MAX_ITERATIONS {
        b.step(d);
        if b.state == PlanState::Failed {
            b.rollback();
        }
    }
    let ok = b.state == PlanState::Succeeded;
    Built {
        plan: if ok { b.resolved.clone() } else { Vec::new() },
        priority: if ok { b.priority() } else { INVALID_PLAN },
        decompositions: b.history.len(),
    }
}

/// Htn.EHtnStatus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Pending,
    Running,
    Failed,
    Succeeded,
    Aborted,
    Obsolete,
    Invalid,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// The preconditions of a plan's tasks from `index` on, applying each one's effects
/// as it goes (HtnPlanRunner.FindInvalidTask): the first whose don't hold, and
/// which attribute.
pub fn find_invalid_task(
    d: &Domain,
    plan: &[Task],
    index: usize,
    ws: &WorldState,
) -> Option<(i32, usize)> {
    let mut w = *ws;
    for t in plan.iter().skip(index) {
        let Some(op) = d.op(t.symbol) else {
            return Some((t.symbol, 0));
        };
        if let Some(p) = op.pre.iter().find(|p| !p.holds(t, &w)) {
            return Some((t.symbol, p.attr));
        }
        for e in &op.eff {
            e.apply(t, &mut w);
        }
    }
    None
}

/// HtnPlanRunner.ApplyEffects: a finished task's effects (those set externally
/// only if `external`).
pub fn apply_effects(d: &Domain, task: &Task, ws: &mut WorldState, external: bool) {
    if let Some(op) = d.op(task.symbol) {
        for e in &op.eff {
            if !external && e.external {
                continue;
            }
            e.apply(task, ws);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domain() -> Domain {
        // Main: [0] if WS3 == 1: Shoot(Duration=2000); [1] Idle. Shoot needs WS18 == 1.
        let text = "task\t0\tMain\tcompound\t-\t-\n\
task\t1\tShoot\tprimitive\tDuration\t2000\n\
task\t2\tIdle\tprimitive\t-\t-\n\
op\t1\tTnShootAtEnemyOperator\t0\t8\n\
pre\t18\t0\t1\t11\t0\tand\n\
eff\t37\t1\t1\t0\t0\t0\n\
op\t2\tTnIdleOperator\t0\t0\n\
method\t0\t0\n\
pre\t3\t0\t1\t11\t0\tand\n\
sub\t1\t2000\n\
method\t0\t1\n\
sub\t2\n\
used\t3\t18\t37\n\
debugflags\t1\n";
        Domain::parse("Test", text.lines())
    }

    #[test]
    fn plans_by_methods_and_rolls_back() {
        let d = domain();
        let mut ws = [Ws::default(); WS_NUM];
        // Nothing seen: Idle (method 1).
        let b = build_plan(&d, Task::new(0, &[]), &ws);
        assert_eq!(b.plan.len(), 1);
        assert_eq!(b.plan[0].symbol, 2);
        assert_eq!(b.priority, 1 << 27);
        // Seen, but no weapon: method 0 fails at Shoot, rolls back to method 1.
        ws[WS_ENEMY_VISIBLE].value = 1;
        let b = build_plan(&d, Task::new(0, &[]), &ws);
        assert_eq!(b.plan[0].symbol, 2);
        // Seen and armed: Shoot, its parameter passed on.
        ws[WS_WEAPON_EQUIPPED].value = 1;
        let b = build_plan(&d, Task::new(0, &[]), &ws);
        assert_eq!(b.plan[0].symbol, 1);
        assert_eq!(b.plan[0].int(0), 2000);
        assert_eq!(b.priority, 0);
        // Its effect, applied.
        let mut w2 = ws;
        apply_effects(&d, &b.plan[0], &mut w2, false);
        assert_eq!(w2[WS_VULNERABILITY].value, 1);
    }

    #[test]
    fn comparisons() {
        assert!(Cmp::BitIsSet.test(0b100, 2));
        assert!(Cmp::BitIsNotSet.test(0b100, 1));
        assert!(Cmp::BitsAreSet.test(66, 2));
        assert!(Cmp::AnyEnumSet.test(2, 0b100));
        assert!(!Cmp::AnyEnumSet.test(1, 0b100));
        assert_eq!(EffOp::SetBit.apply(0, 3), 8);
        assert_eq!(EffOp::ClearBit.apply(15, 0), 14);
    }
}
