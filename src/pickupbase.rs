//! The Escalation purchase points' bases (the pedestals the weapons, ammo crates and
//! equipment float over, and the health stations'), drawn as the game draws them:
//! with WEP_Shared_p's Pickup_Unlit_MAT (through MP_PickupBase_Ammo_MATINST and the
//! bases' own ESC_PickupBase_*_MATINST), an unlit material that lights itself. The
//! map's lights and light maps play no part; a light of its own circles the base
//! instead, once every 4.2 seconds, 45 degrees up:
//!
//!   light      = normalize(cos(1.5 t), sin(1.5 t), 1)   (Unreal's axes, t: the time)
//!   gloss      = Masks_NormY green + 0.12 (Roughness Offset)
//!   specular   = SpecularColor x gloss  ((0.95, 0.93, 0.88); the heavy weapons'
//!                bases (0.56, 0.57, 0.58), MP_PickupBase_Ammo_MATINST's own)
//!   diffuse    = saturate(((N.L + 1) / 2 x 1.127)^2) x colour
//!                x (1 - luminance(specular)) x 0.5
//!   highlight  = |N.H|^max(100 x gloss, 1) x specular x (power + 8) / (8 pi)
//!   reflection = the level's cube map x gloss x specular x |N.H|
//!   glow       = its lights (Masks_NormY red) in its Emissive_Color x 10
//!
//! (from the game's own material graph, compiled by tools/ue3/fxmat.py). The demo had
//! drawn them lit by the map, a dull grey; the game's are their own rust brown, a
//! highlight sweeping round them. Their lights are the map's glow as before
//! (tools/build_map.py: by the kind of point), their textures the map's own: the
//! colour, the normal map and, in the metal channel, the gloss mask (0.2 + 0.5 x
//! green, build_map.py's; the health stations' base, built by build_assets.py,
//! 0.25 + 0.55 x green).

use bevy::gltf::GltfMaterialName;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use std::collections::HashMap;

use crate::character::PaintCache;

const PICKUP_BASE_SHADER: &str = "shaders/pickup_base.wgsl";

/// The bases' materials (ESC_PickupBase_*: the primary and heavy weapons', the
/// assault and utility equipment's and the health stations').
const BASE_MATERIAL_PREFIX: &str = "ESC_PickupBase_";

/// Pickup_Unlit_MAT's highlight colour (its SpecularColor), and the duller one of
/// the heavy weapons' bases (ESC_PickupBase_HeavyWeapon_STAT, as
/// MP_PickupBase_Ammo_MATINST sets it).
const SPECULAR: Vec3 = Vec3::new(0.95, 0.93, 0.88);
const HEAVY_SPECULAR: Vec3 = Vec3::new(0.56, 0.57, 0.58);

/// How the gloss mask is kept in a base's metal channel: offset + scale x green.
/// The maps' bases have build_map.py's 0.2 + 0.5 x green; the health stations' base
/// (models/props/health_base.glb, no map's) build_assets.py's texture sets'
/// 0.25 + 0.55 x green.
const MAP_GLOSS: Vec2 = Vec2::new(0.2, 0.5);
const HEALTH_GLOSS: Vec2 = Vec2::new(0.25, 0.55);

/// A base's shading (pickup_base.wgsl).
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct PickupBase {
    /// The highlight's colour, and in w 1 when there is a cube map to reflect.
    #[uniform(100)]
    pub specular: Vec4,
    /// The level's cube map, in Unreal's axes (the characters' own: PaintCache).
    #[texture(101, dimension = "cube")]
    #[sampler(102)]
    pub reflection: Option<Handle<Image>>,
    /// How its gloss mask is kept in the metal channel (x offset, y scale).
    #[uniform(103)]
    pub gloss: Vec4,
}

impl MaterialExtension for PickupBase {
    fn fragment_shader() -> ShaderRef {
        PICKUP_BASE_SHADER.into()
    }
}

pub type PickupBaseMaterial = ExtendedMaterial<StandardMaterial, PickupBase>;

pub struct PickupBasePlugin;

impl Plugin for PickupBasePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<PickupBaseMaterial>::default())
            .add_systems(Update, (dress_pickup_bases, reflect_in_bases).chain());
    }
}

/// The highlight colour of the base material `name`.
fn specular_of(name: &str) -> Vec3 {
    if name.contains("HeavyWeapon") {
        HEAVY_SPECULAR
    } else {
        SPECULAR
    }
}

/// How the base material `name` keeps its gloss mask.
fn gloss_of(name: &str) -> Vec2 {
    if name.contains("Health") {
        HEALTH_GLOSS
    } else {
        MAP_GLOSS
    }
}

/// Each base coming in (a map's, or a health station's) gets its material in the
/// game's shading: its own textures and glow, the shader lighting it.
#[allow(clippy::type_complexity)]
fn dress_pickup_bases(
    mut commands: Commands,
    parts: Query<
        (Entity, &GltfMaterialName, &MeshMaterial3d<StandardMaterial>),
        Added<GltfMaterialName>,
    >,
    standard: Res<Assets<StandardMaterial>>,
    mut bases: ResMut<Assets<PickupBaseMaterial>>,
    cache: Res<PaintCache>,
    mut made: Local<HashMap<AssetId<StandardMaterial>, Handle<PickupBaseMaterial>>>,
) {
    // (Those of a map gone are let go.)
    made.retain(|id, _| standard.contains(*id));
    for (entity, name, material) in &parts {
        if !name.0.starts_with(BASE_MATERIAL_PREFIX) {
            continue;
        }
        let handle = match made.get(&material.id()) {
            Some(h) => h.clone(),
            None => {
                let Some(base) = standard.get(material.id()) else {
                    continue;
                };
                let mut base = base.clone();
                // (The shader reads the gloss mask from the metal channel.)
                base.metallic = 1.0;
                let cube = cache.reflection_cube();
                let h = bases.add(PickupBaseMaterial {
                    base,
                    extension: PickupBase {
                        specular: specular_of(&name.0).extend(if cube.is_some() {
                            1.0
                        } else {
                            0.0
                        }),
                        reflection: cube,
                        gloss: gloss_of(&name.0).extend(0.0).extend(0.0),
                    },
                });
                made.insert(material.id(), h.clone());
                h
            }
        };
        commands
            .entity(entity)
            .try_remove::<MeshMaterial3d<StandardMaterial>>()
            .try_insert(MeshMaterial3d(handle));
    }
}

/// The bases reflect the cube map the characters do, which changes with the map.
fn reflect_in_bases(cache: Res<PaintCache>, mut bases: ResMut<Assets<PickupBaseMaterial>>) {
    let cube = cache.reflection_cube();
    let stale: Vec<AssetId<PickupBaseMaterial>> = bases
        .iter()
        .filter(|(_, m)| m.extension.reflection != cube)
        .map(|(id, _)| id)
        .collect();
    for id in stale {
        if let Some(m) = bases.get_mut(id) {
            m.extension.reflection = cube.clone();
            m.extension.specular.w = if cube.is_some() { 1.0 } else { 0.0 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The heavy weapons' bases have the duller highlight, the others the game's
    /// default.
    #[test]
    fn the_heavy_bases_shine_less() {
        assert_eq!(
            specular_of("ESC_PickupBase_HeavyWeapon_STAT"),
            HEAVY_SPECULAR
        );
        assert_eq!(
            specular_of("ESC_PickupBase_PrimaryWeapon_MATINST"),
            SPECULAR
        );
        assert_eq!(specular_of("ESC_PickupBase_Health_MATINST"), SPECULAR);
    }

    /// The health stations' base reads its gloss as build_assets.py keeps it, the
    /// maps' bases as build_map.py does.
    #[test]
    fn each_base_reads_its_gloss_as_it_was_kept() {
        assert_eq!(gloss_of("ESC_PickupBase_Health_MATINST"), HEALTH_GLOSS);
        assert_eq!(gloss_of("ESC_PickupBase_PrimaryWeapon_MATINST"), MAP_GLOSS);
        assert_eq!(gloss_of("ESC_PickupBase_HeavyWeapon_STAT"), MAP_GLOSS);
        // (The metal each keeps for full gloss, and none.)
        for g in [HEALTH_GLOSS, MAP_GLOSS] {
            assert!(g.x > 0.0 && g.x + g.y <= 1.0);
        }
    }
}
