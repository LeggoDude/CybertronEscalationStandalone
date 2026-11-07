//! The enemies' AI data (models/enemies/ai.txt, tools/build_ai.py): each behaviour
//! class's planning domain and each enemy type's behaviour values.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use bevy::math::Vec3;

use super::htn::Domain;

pub const AI_FILE: &str = "models/enemies/ai.txt";

/// An enemy type's behaviour: its blueprint's class, its domain, its goal task and
/// its values (class defaults under its blueprint's own, flattened: a struct's
/// fields as key.field, an array's items as key[i]).
#[derive(Clone, Debug, Default)]
pub struct Behaviour {
    pub id: String,
    pub class: String,
    pub domain: Option<Arc<Domain>>,
    pub goal: i32,
    pub props: HashMap<String, String>,
}

impl Behaviour {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.props.get(key).map(|s| s.as_str())
    }

    pub fn f(&self, key: &str, default: f32) -> f32 {
        self.get(key)
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(default)
    }

    pub fn i(&self, key: &str, default: i32) -> i32 {
        self.get(key)
            .and_then(|v| v.trim().parse::<f32>().ok())
            .map(|v| v as i32)
            .unwrap_or(default)
    }

    pub fn b(&self, key: &str, default: bool) -> bool {
        self.get(key)
            .map(|v| v.trim() == "1" || v.trim() == "True")
            .unwrap_or(default)
    }

    /// A vector's value ("x y z").
    pub fn v3(&self, key: &str, default: Vec3) -> Vec3 {
        let Some(v) = self.get(key) else {
            return default;
        };
        let c: Vec<f32> = v
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        if c.len() == 3 {
            Vec3::new(c[0], c[1], c[2])
        } else {
            default
        }
    }

    /// An object reference's last name part (Package.Group.Name -> Name).
    pub fn obj(&self, key: &str) -> Option<&str> {
        self.get(key)
            .filter(|v| *v != "None")
            .map(|v| v.rsplit('.').next().unwrap_or(v))
    }

    /// An array's length (key.n).
    pub fn n(&self, key: &str) -> usize {
        self.i(&format!("{key}.n"), 0).max(0) as usize
    }
}

/// Every behaviour, by enemy type id.
#[derive(Clone, Debug, Default)]
pub struct AiData {
    pub domains: HashMap<String, Arc<Domain>>,
    pub behaviours: HashMap<String, Arc<Behaviour>>,
    pub enums: HashMap<String, Vec<String>>,
    /// The projectiles the enemies' animations spawn (TnAnimNotify_SpawnProjectile),
    /// by class: their values as a gun's round's (weapon.rs's ProjectileData).
    pub projectiles: HashMap<String, Arc<Behaviour>>,
}

impl AiData {
    pub fn load(assets: &Path) -> AiData {
        match std::fs::read_to_string(assets.join(AI_FILE)) {
            Ok(text) => AiData::parse(&text),
            Err(_) => {
                bevy::log::info!("{AI_FILE} not found: the enemies have no AI");
                AiData::default()
            }
        }
    }

    pub fn parse(text: &str) -> AiData {
        let mut out = AiData::default();
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;
        let mut current: Option<Behaviour> = None;
        let mut pending_types: Vec<(Behaviour, String)> = Vec::new();
        while i < lines.len() {
            let line = lines[i];
            let r: Vec<&str> = line.split('\t').collect();
            match r[0] {
                "domain" if r.len() >= 2 => {
                    let start = i + 1;
                    let mut end = start;
                    while end < lines.len() {
                        let k = lines[end].split('\t').next().unwrap_or("");
                        if matches!(k, "domain" | "type" | "enum") {
                            break;
                        }
                        end += 1;
                    }
                    let d = Domain::parse(r[1], lines[start..end].iter().copied());
                    out.domains.insert(r[1].to_string(), Arc::new(d));
                    i = end;
                    continue;
                }
                "type" if r.len() >= 5 => {
                    if let Some(b) = current.take() {
                        let dc = b.class.clone();
                        pending_types.push((b, dc));
                    }
                    current = Some(Behaviour {
                        id: r[1].to_string(),
                        class: r[2].to_string(),
                        domain: None,
                        goal: r[4].trim().parse().unwrap_or(0),
                        props: HashMap::new(),
                    });
                    if r[3] == "-" {
                        // No domain (the drones).
                        if let Some(b) = current.as_mut() {
                            b.class = r[2].to_string();
                        }
                    }
                }
                "prop" if r.len() >= 3 => {
                    if let Some(b) = current.as_mut() {
                        b.props.insert(r[1].to_string(), r[2].to_string());
                    }
                }
                "projectile" if r.len() >= 4 => {
                    let p = out.projectiles.entry(r[1].to_string()).or_insert_with(|| {
                        Arc::new(Behaviour {
                            id: r[1].to_string(),
                            class: r[1].to_string(),
                            ..Default::default()
                        })
                    });
                    if let Some(p) = Arc::get_mut(p) {
                        p.props.insert(r[2].to_string(), r[3].to_string());
                    }
                }
                "enum" if r.len() >= 2 => {
                    out.enums.insert(
                        r[1].to_string(),
                        r[2..].iter().map(|s| s.to_string()).collect(),
                    );
                }
                _ => {}
            }
            i += 1;
        }
        if let Some(b) = current.take() {
            let dc = b.class.clone();
            pending_types.push((b, dc));
        }
        for (mut b, dc) in pending_types {
            // (The drones' characters build theirs in script.)
            b.domain = out
                .domains
                .get(&dc)
                .cloned()
                .or_else(|| super::drones::script_domain(&b).map(Arc::new));
            out.behaviours.insert(b.id.clone(), Arc::new(b));
        }
        out
    }

    pub fn behaviour(&self, id: &str) -> Option<Arc<Behaviour>> {
        self.behaviours.get(id).cloned()
    }

    /// An enum value's index by name (AI.EWorldState's WS_Health...).
    pub fn enum_value(&self, e: &str, name: &str) -> Option<usize> {
        self.enums.get(e)?.iter().position(|n| n == name)
    }
}

#[cfg(test)]
mod tests {
    use super::super::htn::{self, Task, WS_NUM, Ws};
    use super::*;

    /// The game's own domains: every type has its domain and values, and a soldier
    /// with nothing going on (its world state as it starts) plans.
    #[test]
    fn loads_the_games_domains() {
        let data = AiData::load(&crate::assets_dir());
        if data.behaviours.is_empty() {
            eprintln!("ai.txt not built: skipped");
            return;
        }
        let soldier = data.behaviour("SoldierDec_Kaon").expect("Kaon soldier");
        let d = soldier.domain.clone().expect("soldier domain");
        assert_eq!(d.name(soldier.goal), "STS_Soldier");
        assert_eq!(soldier.f("TacticalSpeed", 0.0), 1200.0);
        assert_eq!(soldier.obj("PrimaryWeapon"), Some("TnWeaponBurstRifleAI"));
        let mut ws = [Ws::default(); WS_NUM];
        ws[htn::WS_AREA_CLEAR].value = 1;
        ws[htn::WS_BALANCED].value = 1;
        let b = htn::build_plan(&d, Task::new(soldier.goal, &[]), &ws);
        let names: Vec<&str> = b.plan.iter().map(|t| d.name(t.symbol)).collect();
        eprintln!("soldier at rest: {names:?} priority {:#x}", b.priority);
        assert!(!b.plan.is_empty());
        // Aware of the enemy, an attack point to take: off to it.
        ws[htn::WS_ALERT_LEVEL].value = 2;
        ws[htn::WS_ALERT_TO_ENEMY_PRESENCE].value = 1;
        ws[htn::WS_ATTACK_POINT_AVAILABLE].value = 1;
        ws[htn::WS_ENEMY_RANGE].value = 4;
        let b = htn::build_plan(&d, Task::new(soldier.goal, &[]), &ws);
        let names: Vec<&str> = b.plan.iter().map(|t| d.name(t.symbol)).collect();
        eprintln!(
            "soldier with an attack point: {names:?} priority {:#x}",
            b.priority
        );
        assert!(names.contains(&"STS_MoveToCover"));
        for id in data.behaviours.keys() {
            let b = &data.behaviours[id];
            if b.class.ends_with("CharacterBlueprint") {
                assert!(b.domain.is_some(), "{id} has no domain");
            }
        }
    }
}
