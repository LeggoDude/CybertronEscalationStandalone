//! Materials whose look changes over time without the material changing: a part's
//! glow pulsing or flaring, worked out in the shader from the clock, and a gun
//! effect's sprite fading, its colour its own in one buffer of the sprites'
//! colours, read by its mesh tag (assets/shaders/live.wgsl).
//!
//! A changed material is prepared again for the graphics card, and Bevy keeps the
//! standard materials in shared groups (bindless slabs) whose bind group is made
//! anew whenever one of their materials changes, is added or goes. The map's
//! pulsing glows (mapmotion.rs), the consoles' and pods' outlines (doors.rs,
//! recreator.rs) and the gun effects' sprites (fx.rs, each in a material of its own)
//! changed theirs every frame: some 20 a frame in a wave and well over 100 while a
//! gun fires, and the groups were made anew every frame, about a millisecond of
//! the render thread's frame. Those parts and sprites are drawn in a LiveMaterial
//! instead: the standard material's own shading, its glow scaled and its colour
//! multiplied in its shader, which never changes once made.
//!
//! A part is given its live material as its model comes in, in the frame it does
//! (`liven_parts`), before it is ever drawn: an entity whose material changes type
//! while Bevy has it among what it draws can be drawn a frame with the old pipeline
//! and the new bind group, which wgpu rejects (outline.rs says so too).

use bevy::asset::RenderAssetUsages;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::gltf::GltfMaterialName;
use bevy::mesh::MeshTag;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{AsBindGroup, BufferSize, BufferUsages};
use bevy::render::renderer::RenderQueue;
use bevy::render::storage::{GpuShaderStorageBuffer, ShaderStorageBuffer};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy::shader::ShaderRef;
use std::collections::HashMap;

const LIVE_SHADER: &str = "shaders/live.wgsl";

/// What changes in a live material over time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wave {
    /// Its glow, clamp(sin(2 pi rate t) + 1, 0, 1.5) times as bright: mapmotion.rs's
    /// pulse (the game's ENV_Emissive_MF).
    Pulse { rate: f32 },
    /// Its glow, 1 + amount (1 - |sin(2 pi rate t)|)^2 times as bright: the
    /// consoles' and pods' outlines.
    Flare { rate: f32, amount: f32 },
    /// Its colour times its sprite's own (LiveTints, by its mesh tag).
    Tinted,
}

impl Wave {
    /// As the shader reads it (Live::wave): its kind, rate and amount.
    pub fn uniform(self) -> Vec4 {
        match self {
            Wave::Pulse { rate } => Vec4::new(1.0, rate, 0.0, 0.0),
            Wave::Flare { rate, amount } => Vec4::new(2.0, rate, amount, 0.0),
            Wave::Tinted => Vec4::new(3.0, 0.0, 0.0, 0.0),
        }
    }

    /// How many times as bright its glow is `t` seconds in, as the shader works it
    /// out from the frame's time (for the tests).
    #[cfg(test)]
    pub fn glow_at(self, t: f32) -> f32 {
        match self {
            Wave::Pulse { rate } => crate::mapmotion::pulse(rate, t),
            Wave::Flare { rate, amount } => {
                let k = 1.0 - (std::f32::consts::TAU * rate * t).sin().abs();
                1.0 + amount * k * k
            }
            Wave::Tinted => 1.0,
        }
    }
}

/// A standard material's extension: what changes in it over time.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct Live {
    /// Its wave (Wave::uniform).
    #[uniform(100)]
    pub wave: Vec4,
    /// The sprites' colours (LiveTints): always LIVE_TINTS_BUFFER.
    #[storage(101, read_only, visibility(fragment))]
    pub tints: Handle<ShaderStorageBuffer>,
}

impl Live {
    pub fn new(wave: Wave) -> Live {
        Live {
            wave: wave.uniform(),
            tints: LIVE_TINTS_BUFFER,
        }
    }
}

impl MaterialExtension for Live {
    fn fragment_shader() -> ShaderRef {
        LIVE_SHADER.into()
    }
}

/// A standard material whose glow or colour changes in its shader (see the top).
pub type LiveMaterial = ExtendedMaterial<StandardMaterial, Live>;

/// A live material: `base` with `wave`.
pub fn live_material(base: StandardMaterial, wave: Wave) -> LiveMaterial {
    LiveMaterial {
        base,
        extension: Live::new(wave),
    }
}

// ---------------------------------------------------------------------------
// The sprites' colours
// ---------------------------------------------------------------------------

/// The sprites' colours' buffer (LiveTints): one, fixed, so every live material can
/// bind it.
const LIVE_TINTS_BUFFER: Handle<ShaderStorageBuffer> =
    bevy::asset::uuid_handle!("c4e19a72-6b3d-4f05-8a2e-91d7c6b05e38");

/// The most sprites the colours have room for (LIVE_TINTS_BUFFER, 16 bytes each:
/// 256 KB). A busy fight has a few hundred; one past these would be drawn in its
/// material's own colour (white).
const LIVE_TINTS_MAX: usize = 16384;

/// The colour of a sprite whose colour hasn't been set: its material's own.
const NO_TINT: [f32; 4] = [1.0; 4];

/// The tinted sprites' colours (linear RGBA, multiplying their materials' colour),
/// each by its slot (its mesh tag: LiveTint), which the live material's shader
/// reads. fx.rs sets them each frame as its sprites fade, and the render world
/// writes them to their buffer as they change.
#[derive(Resource, Default)]
pub struct LiveTints {
    colors: Vec<[f32; 4]>,
    /// Slots given back, to give out again.
    free: Vec<u32>,
}

impl LiveTints {
    /// A slot for a new sprite, its colour none.
    fn take(&mut self) -> u32 {
        let slot = self.free.pop().unwrap_or_else(|| {
            self.colors.push(NO_TINT);
            (self.colors.len() - 1) as u32
        });
        self.colors[slot as usize] = NO_TINT;
        if slot as usize == LIVE_TINTS_MAX {
            warn!("More than {LIVE_TINTS_MAX} effect sprites: those past them aren't faded");
        }
        slot
    }

    /// Sets `slot`'s colour; true if it changed.
    pub fn set(&mut self, slot: u32, color: [f32; 4]) -> bool {
        let Some(c) = self.colors.get_mut(slot as usize) else {
            return false;
        };
        if *c == color {
            return false;
        }
        *c = color;
        true
    }
}

/// A sprite's slot among the sprites' colours (LiveTints): its mesh tag, given as
/// it is spawned (with a MeshTag, `tinted`) and given back as it goes.
#[derive(Component)]
#[component(on_add = take_tint, on_remove = give_back_tint)]
pub struct LiveTint;

/// What a tinted sprite is spawned with: its slot (given it as it is spawned).
pub fn tinted() -> (LiveTint, MeshTag) {
    (LiveTint, MeshTag(u32::MAX))
}

fn take_tint(mut world: DeferredWorld, context: HookContext) {
    // (Marked changed: the slot's colour, none, is written over its last sprite's.)
    let Some(slot) = world.get_resource_mut::<LiveTints>().map(|mut t| t.take()) else {
        return;
    };
    if let Some(mut tag) = world.get_mut::<MeshTag>(context.entity) {
        tag.0 = slot;
    }
}

fn give_back_tint(mut world: DeferredWorld, context: HookContext) {
    let Some(slot) = world.get::<MeshTag>(context.entity).map(|t| t.0) else {
        return;
    };
    if let Some(mut tints) = world.get_resource_mut::<LiveTints>() {
        tints.bypass_change_detection().free.push(slot);
    }
}

/// The render world's copy of the sprites' colours, to write (LiveTints).
#[derive(Resource, Default)]
struct LiveTintsRender {
    colors: Vec<[f32; 4]>,
    /// Not written yet.
    fresh: bool,
}

/// Makes the sprites' colours' buffer.
fn make_live_tints(storage: Option<ResMut<Assets<ShaderStorageBuffer>>>) {
    let Some(mut storage) = storage else {
        return;
    };
    let mut buffer =
        ShaderStorageBuffer::with_size(LIVE_TINTS_MAX * 16, RenderAssetUsages::default());
    buffer.buffer_description.label = Some("live_tints");
    // (Written as they change: write_live_tints.)
    buffer.buffer_description.usage |= BufferUsages::COPY_DST;
    let _ = storage.insert(LIVE_TINTS_BUFFER.id(), buffer);
}

/// Copies the sprites' colours to the render world when they have changed.
fn extract_live_tints(tints: Extract<Res<LiveTints>>, mut render: ResMut<LiveTintsRender>) {
    if tints.is_changed() {
        render.colors.clone_from(&tints.colors);
        render.fresh = true;
    }
}

/// Writes the sprites' colours to their buffer (as many as it holds), once it is
/// made.
fn write_live_tints(
    mut tints: ResMut<LiveTintsRender>,
    buffers: Res<RenderAssets<GpuShaderStorageBuffer>>,
    queue: Res<RenderQueue>,
) {
    if !tints.fresh {
        return;
    }
    let Some(gpu) = buffers.get(LIVE_TINTS_BUFFER.id()) else {
        return;
    };
    tints.fresh = false;
    let n = tints.colors.len().min(gpu.buffer.size() as usize / 16);
    let Some(size) = BufferSize::new(n as u64 * 16) else {
        return;
    };
    let Some(mut view) = queue.write_buffer_with(&gpu.buffer, 0, size) else {
        return;
    };
    // (Written only: reading the view is slow, and logs a warning each time.)
    let bytes: &mut [u8] = view.as_mut();
    for (to, value) in bytes.chunks_exact_mut(4).zip(tints.colors.iter().flatten()) {
        to.copy_from_slice(&value.to_le_bytes());
    }
}

// ---------------------------------------------------------------------------
// The parts that glow in time
// ---------------------------------------------------------------------------

/// A part drawn under a node of this name's beginning glows `glow` in its `wave`
/// (the consoles' outlines, doors.rs; the pods', recreator.rs).
#[derive(Clone, Debug)]
pub struct Outline {
    pub prefix: &'static str,
    pub glow: LinearRgba,
    pub wave: Wave,
}

/// The outlines the modules have named (their plugins add them).
#[derive(Resource, Default)]
pub struct LiveOutlines(pub Vec<Outline>);

/// Each part coming in that glows in time is given its live material, in the frame
/// it comes in (before it is drawn): one under an outline's node (LiveOutlines), or
/// one of the map's pulsing materials with a glow to pulse (MapMotion's, by its
/// name). Parts with the same material share their live material.
#[allow(clippy::type_complexity)]
fn liven_parts(
    mut commands: Commands,
    motion: Option<Res<crate::mapmotion::MapMotion>>,
    outlines: Res<LiveOutlines>,
    parts: Query<
        (
            Entity,
            Option<&GltfMaterialName>,
            Option<&Name>,
            &MeshMaterial3d<StandardMaterial>,
        ),
        Or<(Added<GltfMaterialName>, Added<Name>)>,
    >,
    standard: Res<Assets<StandardMaterial>>,
    mut live: ResMut<Assets<LiveMaterial>>,
    mut made: Local<HashMap<(AssetId<StandardMaterial>, [u32; 4]), Handle<LiveMaterial>>>,
) {
    // (Those of a map gone are let go.)
    made.retain(|(id, _), _| standard.contains(*id));
    for (entity, material_name, name, material) in &parts {
        let outline =
            name.and_then(|n| outlines.0.iter().find(|o| n.as_str().starts_with(o.prefix)));
        let (wave, glow) = match outline {
            Some(o) => (o.wave, Some(o.glow)),
            None => {
                let rate = material_name
                    .zip(motion.as_deref())
                    .and_then(|(n, m)| m.pulse_rate(&n.0));
                let Some(rate) = rate else {
                    continue;
                };
                (Wave::Pulse { rate }, None)
            }
        };
        let Some(base) = standard.get(material.id()) else {
            continue;
        };
        // (A pulsing material without a glow has nothing to pulse.)
        let glows = glow.unwrap_or(base.emissive);
        if glows.red == 0.0 && glows.green == 0.0 && glows.blue == 0.0 {
            continue;
        }
        let key = (material.id(), wave.uniform().to_array().map(f32::to_bits));
        let handle = made
            .entry(key)
            .or_insert_with(|| {
                let mut base = base.clone();
                base.emissive = glows;
                live.add(live_material(base, wave))
            })
            .clone();
        commands
            .entity(entity)
            .try_remove::<MeshMaterial3d<StandardMaterial>>()
            .try_insert(MeshMaterial3d(handle));
    }
}

pub struct LivePlugin;

impl Plugin for LivePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<LiveMaterial>::default())
            .init_resource::<LiveTints>()
            .init_resource::<LiveOutlines>()
            .add_systems(Startup, make_live_tints)
            // (In the frame the parts come in, after the scenes are spawned, and
            // before Bevy looks at what their materials need.)
            .add_systems(
                PostUpdate,
                liven_parts.before(bevy::asset::AssetEventSystems),
            );
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<LiveTintsRender>()
            .add_systems(ExtractSchedule, extract_live_tints)
            .add_systems(
                Render,
                write_live_tints.in_set(RenderSystems::PrepareResources),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The waves' glows as the shader works them out: a pulse from nothing to half
    /// again its glow (mapmotion.rs's), a flare from its glow to 1 + amount times it.
    #[test]
    fn the_glows_pulse_and_flare_in_time() {
        let pulse = Wave::Pulse { rate: 1.0 };
        assert!((pulse.glow_at(0.0) - 1.0).abs() < 1e-5);
        assert!((pulse.glow_at(0.25) - 1.5).abs() < 1e-5);
        assert!(pulse.glow_at(0.75) < 1e-5);
        let flare = Wave::Flare {
            rate: 0.2,
            amount: 5.0,
        };
        assert!((flare.glow_at(0.0) - 6.0).abs() < 1e-5);
        assert!((flare.glow_at(1.25) - 1.0).abs() < 1e-5);
        assert_eq!(Wave::Tinted.glow_at(3.0), 1.0);
        assert_eq!(
            [
                pulse.uniform().x,
                flare.uniform().x,
                Wave::Tinted.uniform().x
            ],
            [1.0, 2.0, 3.0]
        );
    }

    /// Each tinted sprite gets a slot of its own among the colours (its mesh tag)
    /// as it is spawned, given back as it goes and given out again with its colour
    /// none (the colours marked changed, so the render world writes that over the
    /// last sprite's).
    #[test]
    fn sprites_get_slots_for_their_colours() {
        let mut world = World::new();
        world.init_resource::<LiveTints>();
        let a = world.spawn(tinted()).id();
        let b = world.spawn(tinted()).id();
        let tag = |w: &World, e: Entity| w.get::<MeshTag>(e).unwrap().0;
        assert_eq!((tag(&world, a), tag(&world, b)), (0, 1));
        {
            let mut tints = world.resource_mut::<LiveTints>();
            assert!(tints.set(1, [0.5, 0.25, 0.0, 0.5]));
            assert!(!tints.set(1, [0.5, 0.25, 0.0, 0.5]));
            assert!(!tints.set(9, [1.0; 4]));
        }
        world.entity_mut(b).despawn();
        assert_eq!(world.resource::<LiveTints>().free, [1]);
        world.clear_trackers();
        let c = world.spawn(tinted()).id();
        assert_eq!(tag(&world, c), 1);
        assert_eq!(world.resource::<LiveTints>().colors[1], NO_TINT);
        assert!(world.is_resource_changed::<LiveTints>());
    }
}
