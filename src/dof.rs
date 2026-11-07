//! The game's depth of field, as Fall of Cybertron's UE3 draws it for a level.
//!
//! A level's post-process volume can turn it on (`bEnableDOF`), and three of the
//! Escalation maps do, all for the far distance only (no near blur,
//! `DOF_MaxNearBlurAmount` 0):
//!
//! | map       | focus  | far falloff | most blur | kernel |
//! |-----------|--------|-------------|-----------|--------|
//! | Ignition  | 0      | 200 m       | 0.8       | 32 px  |
//! | Downfall  | 0      | 2 km        | 0.5       | 16 px  |
//! | Oblivion  | 500 m  | 200 m       | 0.4       | 16 px  |
//!
//! So Ignition's sky, its moons and the far spires are softened; Downfall's sky and
//! Oblivion's space outside the windows a little. tools/build_level.py writes the
//! volume's settings as a `dof` line in the map's lights file (lighting.rs).
//!
//! How the game draws it (its compiled shaders, Engine.xxx's global shader cache,
//! and TFOC.exe's blur set-up):
//!
//! * **How unfocused a depth is** (`CalcUnfocusedPercent`, the same in each shader):
//!   `min(saturate(|depth - FocusDistance| / falloff)^FalloffExponent, most)`, with
//!   the near falloff and `DOF_MaxNearBlurAmount` nearer than the focus, the far
//!   ones beyond it (Fall of Cybertron's own split of UE3's single focus radius).
//! * **Gather** (`TDOFGatherPixelShader<16>`): the picture is cut to a quarter of
//!   its size (the filter buffer, FilterDownsampleFactor 4), each texel the mean of
//!   16 samples with the mean depth's unfocused amount `u`: (u x colour, u).
//! * **Blur** (`GaussianBlurFilterBuffer`, TFOC.exe 0x99C1C0 and 0xD74410): a
//!   Gaussian across and then down the filter buffer whose variance is its radius,
//!   min(DOF_BlurKernelSize / 4, 16) x the view's width / 1280 texels (the kernel is
//!   in pixels at 1280 wide), held to 15, out to that many texels each way.
//! * **Blend** (`FUberHalfResPixelShader<0>` and the uber post-process): with
//!   f = 1 - u at the pixel and B the blurred (u x colour, u),
//!   picture x f + (1 - f) x (B.rgb + picture x f) / (B.a + f). In focus that is the
//!   picture; where everything around is as far out of focus it is the picture
//!   blended toward the blur by u^2 - Ignition's sky is 36% sharp and 64% blurred.
//!   Things in focus in front don't bleed into the blur behind them (B holds only
//!   what is out of focus).
//!
//! It runs before bloom and tone mapping, as the game blends it before adding its
//! bloom; assets/shaders/dof.wgsl is the four passes.

use bevy::core_pipeline::{
    FullscreenShader,
    core_3d::graph::{Core3d, Node3d},
};
use bevy::ecs::query::QueryItem;
use bevy::prelude::*;
use bevy::render::{
    Render, RenderApp, RenderStartup, RenderSystems,
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
    texture::{CachedTexture, TextureCache},
    view::{Msaa, ViewDepthTexture, ViewTarget},
};
use bevy::shader::ShaderDefVal;

use crate::hologram::HologramLabel;

/// The filter buffer is this many times smaller each way (FilterDownsampleFactor).
pub const DOWNSAMPLE: u32 = 4;

/// The game camera's depth of field, from the map's post-process volume (all
/// lengths in metres). On the camera only while the map in play has it.
#[derive(Component, Clone, Copy, Debug, PartialEq, ExtractComponent, ShaderType)]
pub struct GameDof {
    /// DOF_FocusDistance: the distance in focus.
    pub focus: f32,
    /// 1 / DOF_FocusNearFalloff and 1 / DOF_FocusFarFalloff.
    pub inv_near_falloff: f32,
    pub inv_far_falloff: f32,
    /// DOF_FalloffExponent.
    pub exponent: f32,
    /// DOF_MaxNearBlurAmount and DOF_MaxFarBlurAmount.
    pub max_near: f32,
    pub max_far: f32,
    /// DOF_BlurKernelSize, in pixels at 1280 wide.
    pub kernel: f32,
    /// The camera's near plane (Bevy's depth is near / distance); kept up to date
    /// by `update_view`.
    pub near: f32,
    /// The view's part of the picture it draws into, in the picture's 0 to 1 each
    /// way: left, top, width and height (all of it but for split screen's views,
    /// split.rs: the blur stays within the view, and its kernel is the view's
    /// width's); kept up to date by `update_view`.
    pub view: Vec4,
}

impl GameDof {
    /// From the settings as the lights file gives them: focus distance, near and
    /// far falloffs (m), exponent, most near and far blur, kernel (pixels).
    pub fn new(v: &[f32]) -> Option<GameDof> {
        let v: [f32; 7] = v.get(..7)?.try_into().ok()?;
        let [
            focus,
            near_falloff,
            far_falloff,
            exponent,
            max_near,
            max_far,
            kernel,
        ] = v;
        Some(GameDof {
            focus,
            inv_near_falloff: 1.0 / near_falloff.max(1e-3),
            inv_far_falloff: 1.0 / far_falloff.max(1e-3),
            exponent,
            max_near,
            max_far,
            kernel,
            near: 0.1,
            view: Vec4::new(0.0, 0.0, 1.0, 1.0),
        })
    }
}

pub struct GameDofPlugin;

impl Plugin for GameDofPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<GameDof>::default(),
            UniformComponentPlugin::<GameDof>::default(),
        ))
        .add_systems(PostUpdate, update_view);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .add_systems(RenderStartup, init_dof_pipelines)
            .add_systems(
                Render,
                prepare_dof_textures.in_set(RenderSystems::PrepareResources),
            )
            .add_render_graph_node::<ViewNodeRunner<GameDofNode>>(Core3d, GameDofLabel)
            // Before the pause menu's hologram (hologram.rs), which draws over the
            // picture, and so before bloom.
            .add_render_graph_edges(
                Core3d,
                (
                    Node3d::StartMainPassPostProcessing,
                    GameDofLabel,
                    HologramLabel,
                ),
            );
    }
}

/// Keeps each camera's near plane and its part of the picture in its settings.
fn update_view(mut cameras: Query<(&mut GameDof, &Projection, &Camera)>) {
    for (mut dof, projection, camera) in &mut cameras {
        if let Projection::Perspective(p) = projection
            && dof.near != p.near
        {
            dof.near = p.near;
        }
        let view = match (&camera.viewport, camera.physical_target_size()) {
            (Some(v), Some(whole)) if whole.x > 0 && whole.y > 0 => {
                let whole = whole.as_vec2();
                let at = v.physical_position.as_vec2() / whole;
                let part = v.physical_size.as_vec2() / whole;
                Vec4::new(at.x, at.y, part.x, part.y)
            }
            _ => Vec4::new(0.0, 0.0, 1.0, 1.0),
        };
        if dof.view != view {
            dof.view = view;
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub(crate) struct GameDofLabel;

/// The view's two filter buffers (the gather and the blur go back and forth
/// between them).
#[derive(Component)]
struct GameDofTextures {
    a: CachedTexture,
    b: CachedTexture,
}

const FILTER_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

fn prepare_dof_textures(
    mut commands: Commands,
    mut cache: ResMut<TextureCache>,
    device: Res<RenderDevice>,
    views: Query<(Entity, &ViewTarget), With<GameDof>>,
) {
    for (entity, target) in &views {
        let size = target.main_texture().size();
        let descriptor = |label: &'static str| TextureDescriptor {
            label: Some(label),
            size: Extent3d {
                width: size.width.div_ceil(DOWNSAMPLE).max(1),
                height: size.height.div_ceil(DOWNSAMPLE).max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: FILTER_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        commands.entity(entity).insert(GameDofTextures {
            a: cache.get(&device, descriptor("game_dof_a")),
            b: cache.get(&device, descriptor("game_dof_b")),
        });
    }
}

#[derive(Default)]
struct GameDofNode;

impl ViewNode for GameDofNode {
    type ViewQuery = (
        &'static ViewTarget,
        &'static ViewDepthTexture,
        &'static GameDofTextures,
        &'static DynamicUniformIndex<GameDof>,
        &'static Msaa,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, depth, textures, settings_index, msaa): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let pipelines = world.resource::<GameDofPipelines>();
        let cache = world.resource::<PipelineCache>();
        let ms = usize::from(*msaa != Msaa::Off);
        let hdr = usize::from(view_target.is_hdr());
        let (Some(gather), Some(blur_x), Some(blur_y), Some(blend)) = (
            cache.get_render_pipeline(pipelines.gather[ms]),
            cache.get_render_pipeline(pipelines.blur[0]),
            cache.get_render_pipeline(pipelines.blur[1]),
            cache.get_render_pipeline(pipelines.blend[ms][hdr]),
        ) else {
            return Ok(());
        };
        let uniforms = world.resource::<ComponentUniforms<GameDof>>();
        let Some(settings) = uniforms.uniforms().binding() else {
            return Ok(());
        };
        let device = render_context.render_device().clone();
        let post_process = view_target.post_process_write();
        let a = &textures.a.default_view;
        let b = &textures.b.default_view;
        let offset = [settings_index.index()];

        let gather_group = device.create_bind_group(
            "game_dof_gather",
            &cache.get_bind_group_layout(&pipelines.gather_layout[ms]),
            &BindGroupEntries::sequential((post_process.source, depth.view(), settings.clone())),
        );
        let blur_x_group = device.create_bind_group(
            "game_dof_blur_x",
            &cache.get_bind_group_layout(&pipelines.blur_layout),
            &BindGroupEntries::sequential((a, settings.clone())),
        );
        let blur_y_group = device.create_bind_group(
            "game_dof_blur_y",
            &cache.get_bind_group_layout(&pipelines.blur_layout),
            &BindGroupEntries::sequential((b, settings.clone())),
        );
        let blend_group = device.create_bind_group(
            "game_dof_blend",
            &cache.get_bind_group_layout(&pipelines.blend_layout[ms]),
            &BindGroupEntries::sequential((
                post_process.source,
                depth.view(),
                a,
                &pipelines.sampler,
                settings.clone(),
            )),
        );

        let passes = [
            ("game_dof_gather", gather, &gather_group, a),
            ("game_dof_blur_x", blur_x, &blur_x_group, b),
            ("game_dof_blur_y", blur_y, &blur_y_group, a),
            (
                "game_dof_blend",
                blend,
                &blend_group,
                post_process.destination,
            ),
        ];
        for (label, pipeline, group, target) in passes {
            let mut pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations::default(),
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_render_pipeline(pipeline);
            pass.set_bind_group(0, group, &offset);
            pass.draw(0..3, 0..1);
        }
        Ok(())
    }
}

/// The four passes' pipelines: the gather and the blend for a multisampled depth
/// buffer or not ([ms]), the blend for an HDR view or not ([ms][hdr]), the blur
/// across and down.
#[derive(Resource)]
struct GameDofPipelines {
    gather_layout: [BindGroupLayoutDescriptor; 2],
    blur_layout: BindGroupLayoutDescriptor,
    blend_layout: [BindGroupLayoutDescriptor; 2],
    sampler: Sampler,
    gather: [CachedRenderPipelineId; 2],
    blur: [CachedRenderPipelineId; 2],
    blend: [[CachedRenderPipelineId; 2]; 2],
}

fn init_dof_pipelines(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
) {
    let picture = || texture_2d(TextureSampleType::Float { filterable: true });
    let depth = |ms: bool| {
        if ms {
            texture_depth_2d_multisampled()
        } else {
            texture_depth_2d()
        }
    };
    let settings = || uniform_buffer::<GameDof>(true);
    let gather_layout = [false, true].map(|ms| {
        BindGroupLayoutDescriptor::new(
            "game_dof_gather_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (picture(), depth(ms), settings()),
            ),
        )
    });
    let blur_layout = BindGroupLayoutDescriptor::new(
        "game_dof_blur_layout",
        &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (picture(), settings())),
    );
    let blend_layout = [false, true].map(|ms| {
        BindGroupLayoutDescriptor::new(
            "game_dof_blend_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    picture(),
                    depth(ms),
                    picture(),
                    sampler(SamplerBindingType::Filtering),
                    settings(),
                ),
            ),
        )
    });
    let sampler = render_device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    let shader = asset_server.load("shaders/dof.wgsl");
    let queue = |label: &'static str,
                 layout: &BindGroupLayoutDescriptor,
                 defs: Vec<ShaderDefVal>,
                 format: TextureFormat| {
        pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some(label.into()),
            layout: vec![layout.clone()],
            vertex: fullscreen_shader.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: shader.clone(),
                shader_defs: defs,
                targets: vec![Some(ColorTargetState {
                    format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            ..default()
        })
    };
    let ms_defs = |pass: &str, ms: usize| {
        let mut defs: Vec<ShaderDefVal> = vec![pass.into()];
        if ms == 1 {
            defs.push("MULTISAMPLED".into());
        }
        defs
    };
    let gather = [0, 1].map(|ms| {
        queue(
            "game_dof_gather",
            &gather_layout[ms],
            ms_defs("GATHER", ms),
            FILTER_FORMAT,
        )
    });
    let blur = ["BLUR_X", "BLUR_Y"].map(|dir| {
        queue(
            "game_dof_blur",
            &blur_layout,
            vec!["BLUR".into(), dir.into()],
            FILTER_FORMAT,
        )
    });
    let blend = [0, 1].map(|ms| {
        [
            TextureFormat::bevy_default(),
            ViewTarget::TEXTURE_FORMAT_HDR,
        ]
        .map(|format| {
            queue(
                "game_dof_blend",
                &blend_layout[ms],
                ms_defs("BLEND", ms),
                format,
            )
        })
    });
    commands.insert_resource(GameDofPipelines {
        gather_layout,
        blur_layout,
        blend_layout,
        sampler,
        gather,
        blur,
        blend,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // The passes' numbers, worked out here as dof.wgsl works them out, to check
    // them against the game's.

    /// The blur's kernel sizes are in pixels at this width.
    const KERNEL_REFERENCE_WIDTH: f32 = 1280.0;
    /// The blur's radius is held to this many filter texels (MAX_FILTER_SAMPLES - 1).
    const MAX_RADIUS: f32 = 15.0;
    /// The kernel is first held to this many filter texels at the reference width.
    const MAX_KERNEL_TEXELS: f32 = 16.0;
    /// The unfocused amount's power is taken of at least this (the shaders' log2 of
    /// smaller is -13.2877).
    const POW_FLOOR: f32 = 0.0001;

    impl GameDof {
        /// How out of focus something `depth` metres away is (dof.wgsl's `unfocused`,
        /// CalcUnfocusedPercent).
        pub fn unfocused(&self, depth: f32) -> f32 {
            let rel = depth - self.focus;
            let (inv, most) = if rel >= 0.0 {
                (self.inv_far_falloff, self.max_far)
            } else {
                (self.inv_near_falloff, self.max_near)
            };
            let x = (rel.abs() * inv).clamp(0.0, 1.0);
            let p = if x >= POW_FLOOR {
                x.powf(self.exponent)
            } else {
                POW_FLOOR.powf(self.exponent)
            };
            p.min(most)
        }

        /// The blur's radius (and variance) in filter texels for a picture `width`
        /// pixels wide (dof.wgsl's blur pass).
        pub fn blur_radius(&self, width: f32) -> f32 {
            ((self.kernel / DOWNSAMPLE as f32).min(MAX_KERNEL_TEXELS) * width
                / KERNEL_REFERENCE_WIDTH)
                .clamp(1e-5, MAX_RADIUS)
        }
    }

    /// Ignition's volume (ESC_Gladiator_ART_m's PostProcessVolume): no near blur,
    /// far falloff 20000 UU, most 0.8, the default exponent 4, a 32-pixel kernel.
    fn ignition() -> GameDof {
        GameDof::new(&[0.0, 20.0, 200.0, 4.0, 0.0, 0.8, 32.0]).unwrap()
    }

    #[test]
    fn unfocused_follows_the_game() {
        let dof = ignition();
        assert!(dof.unfocused(0.0) < 1e-12);
        assert!((dof.unfocused(100.0) - 0.0625).abs() < 1e-6);
        assert!((dof.unfocused(150.0) - 0.75f32.powi(4)).abs() < 1e-6);
        // Held to DOF_MaxFarBlurAmount: the sky and the moons.
        assert_eq!(dof.unfocused(600.0), 0.8);
        assert_eq!(dof.unfocused(1e5), 0.8);
        // Oblivion's focus at 500 m: nearer is sharp (no near blur).
        let ark = GameDof::new(&[500.0, 200.0, 200.0, 4.0, 0.0, 0.4, 16.0]).unwrap();
        assert_eq!(ark.unfocused(30.0), 0.0);
        assert!((ark.unfocused(600.0) - 0.0625).abs() < 1e-6);
        assert_eq!(ark.unfocused(900.0), 0.4);
    }

    #[test]
    fn blur_radius_scales_with_the_view() {
        let dof = ignition();
        // 32 px / 4 = 8 texels at 1280 wide, 12 at 1920, held to 15.
        assert!((dof.blur_radius(1280.0) - 8.0).abs() < 1e-5);
        assert!((dof.blur_radius(1920.0) - 12.0).abs() < 1e-5);
        assert_eq!(dof.blur_radius(3840.0), 15.0);
        let iacon = GameDof::new(&[0.0, 20.0, 2000.0, 4.0, 0.0, 0.5, 16.0]).unwrap();
        assert!((iacon.blur_radius(1920.0) - 6.0).abs() < 1e-5);
    }

    #[test]
    fn settings_need_all_seven_values() {
        assert!(GameDof::new(&[0.0, 20.0, 200.0, 4.0, 0.0, 0.8]).is_none());
        assert_eq!(ignition().inv_far_falloff, 1.0 / 200.0);
    }
}
