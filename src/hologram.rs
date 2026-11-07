//! The pause menu's "Teletraan" view of the world (UI_GFxPause_p.PauseMenuPostProcess).
//!
//! While the game's pause menu is up, the world keeps running but is drawn as a
//! hologram: the material UI_Materials_p.Hologram_Post_MAT replaces the picture with
//! a dark blue field crossed by scan lines, where every jump in depth (the outline
//! of Jazz, the edges of the map) glows blue, and far things are blurred (the
//! chain's depth of field). It sweeps in as a ring growing from Jazz when the menu
//! opens, and sweeps out the same way when it closes (TnGameViewportClient's
//! StartPauseEffect / UpdatePauseEffect / StopPauseEffect, one second each).
//!
//! The material, from its expression graph (its custom HLSL nodes included):
//!
//! ```text
//! z(uv)      = clamp(SceneDepth(uv) / 500, 0, 100)                (500 = 5 m)
//! edge       = saturate(|z(a) - z(b)| + |z(a) - z(c)|)           a, b, c = uv + (-+.001, -+.001)
//! volume     = edge * VolumeColor(0, .3, .6) + BaseColor(0, .01, .02)
//! hologram   = |1 - sin(ScreenY * 1440)| * (volume + .01) + volume     (scan lines)
//! ring(r, f) = 1 / (1 + 2.72^(-(saturate(r / distance to Location) - .9) * f))
//! in         = ring(MaxRadius * RadiusAlphaIn^5, 150),  out = ring(MaxRadius * RadiusAlphaOut^3, 150)
//! scene      = SceneColor * (1 - (RadiusAlphaIn - ceil(RadiusAlphaOut)))^6
//! colour     = lerp(lerp(scene + EdgeColor * (1 - cos(pi in)), hologram, in),
//!                   scene + EdgeColor * (1 - cos(2 pi out)), out)       EdgeColor (.5, 3, 2)
//! ```
//!
//! The chain's depth of field (UberPostProcessEffect: no near blur, 32-pixel
//! kernel, the game's DOF focus 8 m and far falloff 10 m) blurs the hologram's
//! glow with distance; the scan lines are laid on after.
//!
//! assets/shaders/hologram.wgsl is that material; this module runs it over the game
//! camera's picture (before bloom, so the glow blooms) with the camera's depth.

use bevy::core_pipeline::{
    FullscreenShader,
    core_3d::graph::{Core3d, Node3d},
};
use bevy::ecs::query::QueryItem;
use bevy::prelude::*;
use bevy::render::{
    RenderApp, RenderStartup,
    extract_component::{
        ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
        UniformComponentPlugin,
    },
    render_graph::{
        NodeRunError, RenderGraphContext, RenderGraphExt, RenderLabel, ViewNode, ViewNodeRunner,
    },
    render_resource::{
        binding_types::{
            sampler, texture_2d, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer,
        },
        *,
    },
    renderer::{RenderContext, RenderDevice},
    view::{Msaa, ViewDepthTexture, ViewTarget},
};

/// The material's sweep: MaxRadius (PauseMenu_Post_MAT_INST) in metres.
const MAX_RADIUS: f32 = 2_000_000.0 / 100.0;
/// RadiusAlphaIn / Out take this long to go from 0 to 1 (PauseEffectLerpTime).
pub const SWEEP_SECS: f32 = 1.0;

/// The effect's settings, on the game camera (the material's parameters as they
/// stand this frame). `enabled` is 0 when the effect is off and the pass is skipped.
#[derive(Component, Clone, Copy, Default, ExtractComponent, ShaderType)]
pub struct Hologram {
    /// The camera's world-from-clip matrix (to find each pixel's place in the world).
    pub world_from_clip: Mat4,
    /// Where the sweep starts (the material's Location: where Jazz stood).
    pub location: Vec3,
    /// The sweep radii (MaxRadius * RadiusAlphaIn^5, MaxRadius * RadiusAlphaOut^3), m.
    pub radius_in: f32,
    pub radius_out: f32,
    /// How much of the real picture is left: (1 - (In - ceil(Out)))^6.
    pub scene: f32,
    pub enabled: f32,
    /// The camera's near plane (Bevy's depth is near / distance).
    pub near: f32,
}

/// Where the pause effect stands (driven by the pause menu).
#[derive(Resource, Default)]
pub struct PauseEffect {
    /// The menu is up.
    pub on: bool,
    /// RadiusAlphaIn and RadiusAlphaOut.
    pub alpha_in: f32,
    pub alpha_out: f32,
    /// The sweep's centre.
    pub location: Vec3,
    /// The effect is drawn at all (on, or sweeping out).
    pub visible: bool,
}

impl PauseEffect {
    /// StartPauseEffect: the sweep starts again from `location`.
    pub fn start(&mut self, location: Vec3) {
        self.on = true;
        self.visible = true;
        self.alpha_in = 0.0;
        self.alpha_out = 0.0;
        self.location = location;
    }

    /// StopPauseEffect: the real picture sweeps back.
    pub fn stop(&mut self) {
        if self.visible {
            self.on = false;
            self.alpha_in = 1.0;
            self.alpha_out = 0.0;
        }
    }
}

pub struct HologramPlugin;

impl Plugin for HologramPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseEffect>()
            .add_plugins((
                ExtractComponentPlugin::<Hologram>::default(),
                UniformComponentPlugin::<Hologram>::default(),
            ))
            .add_systems(
                PostUpdate,
                update_hologram
                    .after(bevy::transform::TransformSystems::Propagate)
                    .after(bevy::camera::CameraUpdateSystems),
            );

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .add_systems(RenderStartup, init_hologram_pipeline)
            .add_render_graph_node::<ViewNodeRunner<HologramNode>>(Core3d, HologramLabel)
            .add_render_graph_edges(
                Core3d,
                (
                    Node3d::StartMainPassPostProcessing,
                    HologramLabel,
                    Node3d::Bloom,
                ),
            );
    }
}

/// UpdatePauseEffect, and the camera's settings for the shader.
fn update_hologram(
    time: Res<Time<Real>>,
    mut effect: ResMut<PauseEffect>,
    mut cameras: Query<(&mut Hologram, &Camera, &GlobalTransform, &Projection)>,
) {
    let step = time.delta_secs() / SWEEP_SECS;
    if effect.on {
        effect.alpha_in = (effect.alpha_in + step).min(1.0);
    } else if effect.visible {
        effect.alpha_out = (effect.alpha_out + step).min(1.0);
        if effect.alpha_out >= 1.0 {
            effect.visible = false;
        }
    }
    for (mut hologram, camera, transform, projection) in &mut cameras {
        let near = match projection {
            Projection::Perspective(p) => p.near,
            _ => 0.1,
        };
        let out_started = if effect.alpha_out > 0.0 { 1.0 } else { 0.0 };
        *hologram = Hologram {
            world_from_clip: transform.to_matrix() * camera.clip_from_view().inverse(),
            location: effect.location,
            radius_in: MAX_RADIUS * effect.alpha_in.powi(5),
            radius_out: MAX_RADIUS * effect.alpha_out.powi(3),
            scene: (1.0 - (effect.alpha_in - out_started))
                .clamp(0.0, 1.0)
                .powi(6),
            enabled: if effect.visible { 1.0 } else { 0.0 },
            near,
        };
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub(crate) struct HologramLabel;

#[derive(Default)]
struct HologramNode;

impl ViewNode for HologramNode {
    type ViewQuery = (
        &'static ViewTarget,
        &'static ViewDepthTexture,
        &'static Hologram,
        &'static DynamicUniformIndex<Hologram>,
        &'static Msaa,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, depth, hologram, settings_index, msaa): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        if hologram.enabled <= 0.0 {
            return Ok(());
        }
        let pipelines = world.resource::<HologramPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let multisampled = *msaa != Msaa::Off;
        let (layout, id) = pipelines.pick(multisampled, view_target.is_hdr());
        let Some(pipeline) = pipeline_cache.get_render_pipeline(id) else {
            return Ok(());
        };
        let uniforms = world.resource::<ComponentUniforms<Hologram>>();
        let Some(settings) = uniforms.uniforms().binding() else {
            return Ok(());
        };

        let post_process = view_target.post_process_write();
        let bind_group = render_context.render_device().create_bind_group(
            "hologram_bind_group",
            &pipeline_cache.get_bind_group_layout(layout),
            &BindGroupEntries::sequential((
                post_process.source,
                &pipelines.sampler,
                depth.view(),
                settings.clone(),
            )),
        );
        let mut pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("hologram_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: post_process.destination,
                depth_slice: None,
                resolve_target: None,
                ops: Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_render_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[settings_index.index()]);
        pass.draw(0..3, 0..1);
        Ok(())
    }
}

/// The pipelines, for a multisampled depth buffer or not, and HDR or not.
#[derive(Resource)]
struct HologramPipeline {
    layout: BindGroupLayoutDescriptor,
    layout_multisampled: BindGroupLayoutDescriptor,
    sampler: Sampler,
    /// [multisampled][hdr]
    ids: [[CachedRenderPipelineId; 2]; 2],
}

impl HologramPipeline {
    fn pick(
        &self,
        multisampled: bool,
        hdr: bool,
    ) -> (&BindGroupLayoutDescriptor, CachedRenderPipelineId) {
        let layout = if multisampled {
            &self.layout_multisampled
        } else {
            &self.layout
        };
        (
            layout,
            self.ids[usize::from(multisampled)][usize::from(hdr)],
        )
    }
}

fn init_hologram_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
) {
    let layout_for = |multisampled: bool| {
        let depth = if multisampled {
            texture_depth_2d_multisampled()
        } else {
            texture_depth_2d()
        };
        BindGroupLayoutDescriptor::new(
            "hologram_bind_group_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    depth,
                    uniform_buffer::<Hologram>(true),
                ),
            ),
        )
    };
    let layout = layout_for(false);
    let layout_multisampled = layout_for(true);
    let sampler = render_device.create_sampler(&SamplerDescriptor::default());
    let shader = asset_server.load("shaders/hologram.wgsl");
    let mut ids = [[CachedRenderPipelineId::INVALID; 2]; 2];
    for (m, row) in ids.iter_mut().enumerate() {
        for (h, id) in row.iter_mut().enumerate() {
            let multisampled = m == 1;
            let format = if h == 1 {
                ViewTarget::TEXTURE_FORMAT_HDR
            } else {
                TextureFormat::bevy_default()
            };
            *id = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("hologram_pipeline".into()),
                layout: vec![if multisampled {
                    layout_multisampled.clone()
                } else {
                    layout.clone()
                }],
                vertex: fullscreen_shader.to_vertex_state(),
                fragment: Some(FragmentState {
                    shader: shader.clone(),
                    shader_defs: if multisampled {
                        vec!["MULTISAMPLED".into()]
                    } else {
                        vec![]
                    },
                    targets: vec![Some(ColorTargetState {
                        format,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })],
                    ..default()
                }),
                ..default()
            });
        }
    }
    commands.insert_resource(HologramPipeline {
        layout,
        layout_multisampled,
        sampler,
        ids,
    });
}
