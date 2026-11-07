//! The game's light shafts: the sun's rays, as Fall of Cybertron's UE3 draws them
//! for a light with `bRenderLightShafts`.
//!
//! Three of the Escalation maps have such a light (tools/build_level.py writes it
//! as a `shafts` line in the map's lights file, lighting.rs's `ShaftLight`):
//!
//! | map       | light                                  | BloomScale | tint            | occlusion range | screen blend |
//! |-----------|----------------------------------------|------------|-----------------|-----------------|--------------|
//! | Downfall  | DirectionalLight_13872 (lights nothing) | 1          | (255, 199, 184) | 1 km            | 0.2          |
//! | Ancients  | its sun (the DominantDirectionalLight)  | 10         | (255, 235, 189) | 200 m           | 1            |
//! | Ignition  | its sun                                | 0.5        | white           | 200 m           | 1            |
//!
//! Downfall's shaft light shines the way its sun does; its own brightness lights
//! nothing (its lighting channels are all off), so only its shafts show.
//!
//! How the game draws them (its compiled shaders, Engine.xxx's global shader cache:
//! `TDownsampleLightShaftsPixelShader<0>`, `FBlurLightShaftsPixelShader`,
//! `FApplyLightShaftsPixelShader`), from the picture as it stands after the
//! translucent things, before the depth of field, bloom and tone curve:
//!
//! * **Downsample** to the quarter-size filter buffer. With t the texel's place on
//!   the screen (0 to 1 each way), the edge mask e = (1 - 8 t.x (1 - t.x) t.y
//!   (1 - t.y))^4 (1 at the screen's edges). Alpha (the occlusion): the mean of
//!   saturate(depth / OcclusionDepthRange), at least e: what is near the camera
//!   blocks the light, the sky lets it through. Colour (the bloom): the picture's
//!   light over BloomThreshold (its luminance l = 0.3 r + 0.59 g + 0.11 b),
//!   colour x BloomScale x (l - threshold) / l x 2, from the far half of the
//!   occlusion range only (x the mean of saturate((depth - range / 2) / range)),
//!   x (1 - e), x (1 - saturate(5 d))^2 with d the distance from the sun's place on
//!   the screen (in screen heights), x 1/4: the sky right round the sun.
//! * **Blur** the filter buffer toward the sun: 48 samples from the texel to the
//!   sun's place (at most sqrt(d / 2) of the way when it is further than half a
//!   screen height), weighted 2 - k / 24 (the k-th; the colour's min(w, 4 w^2)),
//!   their sum / 48.
//! * **Apply** to the picture: with B the blurred (colour, occlusion) and D the
//!   OcclusionMaskDarkness (0.3), the picture is scaled by
//!   lerp(D, 1.5 - D / 2, B.a^2) - darker where the way to the sun is blocked,
//!   brighter where it is open - eased back to 1 by saturate(d / 2) and by the
//!   distance fade^3 (the sun going off the screen), and the bloom B.rgb x
//!   BloomTint x 4 x saturate(2^(-3 l) x BloomScreenBlendThreshold) is added (l the
//!   picture's luminance: less over what is already bright).
//!
//! The sun's place is where its direction meets the far distance on the screen;
//! with the sun behind the camera, or a whole screen-half past the screen's edge,
//! there are none. They come with the bloom, as the Graphics level has it
//! (graphics.rs). assets/shaders/lightshafts.wgsl is the three passes.

use bevy::camera::visibility::VisibilitySystems;
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

use crate::dof::GameDofLabel;
use crate::lighting::{MapLighting, ShaftLight};

/// The filter buffer is this many times smaller each way (FilterDownsampleFactor),
/// as the depth of field's (dof.rs).
pub const DOWNSAMPLE: u32 = 4;

/// The game camera's light shafts this frame: on the camera only while the map in
/// play has a light that draws them, the Graphics level has them and the sun is
/// on the screen or near it (`aim_light_shafts`).
#[derive(Component, Clone, Copy, Debug, PartialEq, ExtractComponent, ShaderType)]
pub struct GameLightShafts {
    /// The sun's place on the screen (TextureSpaceBlurOrigin): x in screen heights
    /// from the left (u x width / height), y from the top (v).
    pub origin: Vec2,
    /// The view's width / height.
    pub aspect: f32,
    /// 1 / OcclusionDepthRange (per metre).
    pub inv_range: f32,
    /// BloomTint (linear) and BloomThreshold.
    pub tint: Vec4,
    /// BloomScale.
    pub bloom_scale: f32,
    /// OcclusionMaskDarkness.
    pub darkness: f32,
    /// BloomScreenBlendThreshold.
    pub screen_blend: f32,
    /// 0 with the sun on the screen, to 1 as it goes a screen-half past its edge.
    pub distance_fade: f32,
    /// The camera's near plane (Bevy's depth is near / distance).
    pub near: f32,
    /// The view's part of the picture it draws into, in the picture's 0 to 1 each
    /// way: left, top, width and height (all of it but for split screen's views,
    /// split.rs). The screen the shafts are worked out on is the view's.
    pub view: Vec4,
}

/// All of the picture (`GameLightShafts::view`).
pub const WHOLE_VIEW: Vec4 = Vec4::new(0.0, 0.0, 1.0, 1.0);

/// Where the sun is on a view, and the shafts' settings there: None with the sun
/// behind the camera or too far off the screen. `clip_from_world` is the camera's
/// projection x view; `to_sun` points at the sun; `view` the view's part of the
/// picture (WHOLE_VIEW for all of it).
pub fn shafts_for_view(
    light: &ShaftLight,
    clip_from_world: Mat4,
    to_sun: Vec3,
    (aspect, view): (f32, Vec4),
    near: f32,
) -> Option<GameLightShafts> {
    let clip = clip_from_world * to_sun.extend(0.0);
    if clip.w <= 1e-6 {
        return None;
    }
    let ndc = clip.truncate().truncate() / clip.w;
    let distance_fade = (ndc.x.abs().max(ndc.y.abs()) - 1.0).clamp(0.0, 1.0);
    if distance_fade >= 1.0 {
        return None;
    }
    let uv = Vec2::new(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let tint = light.tint.to_linear();
    Some(GameLightShafts {
        origin: Vec2::new(uv.x * aspect, uv.y),
        aspect,
        inv_range: 1.0 / light.range.max(1e-3),
        tint: Vec4::new(tint.red, tint.green, tint.blue, light.threshold),
        bloom_scale: light.bloom_scale,
        darkness: light.darkness,
        screen_blend: light.screen_blend,
        distance_fade,
        near,
        view,
    })
}

pub struct LightShaftsPlugin;

impl Plugin for LightShaftsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<GameLightShafts>::default(),
            UniformComponentPlugin::<GameLightShafts>::default(),
        ))
        .add_systems(
            PostUpdate,
            aim_light_shafts
                .after(bevy::transform::TransformSystems::Propagate)
                .after(bevy::camera::CameraUpdateSystems)
                .before(VisibilitySystems::CheckVisibility),
        );

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .add_systems(RenderStartup, init_shaft_pipelines)
            .add_systems(
                Render,
                prepare_shaft_textures.in_set(RenderSystems::PrepareResources),
            )
            .add_render_graph_node::<ViewNodeRunner<LightShaftsNode>>(Core3d, LightShaftsLabel)
            // On the picture as the main pass leaves it, before the depth of field
            // (dof.rs), as the game draws them before its post-processing.
            .add_render_graph_edges(
                Core3d,
                (
                    Node3d::StartMainPassPostProcessing,
                    LightShaftsLabel,
                    GameDofLabel,
                ),
            );
    }
}

/// Puts the light shafts on the game camera, aimed at the sun, or takes them off.
#[allow(clippy::type_complexity)]
fn aim_light_shafts(
    mut commands: Commands,
    map: Option<Res<MapLighting>>,
    quality: Res<crate::graphics::GraphicsQuality>,
    cameras: Query<
        (
            Entity,
            &Camera,
            &GlobalTransform,
            &Projection,
            Option<&GameLightShafts>,
        ),
        With<crate::OrbitCamera>,
    >,
) {
    let light = map
        .as_deref()
        .and_then(|m| m.shafts.as_ref())
        .filter(|_| quality.level().bloom);
    for (entity, camera, transform, projection, had) in &cameras {
        let shafts = light.and_then(|light| {
            let Projection::Perspective(p) = projection else {
                return None;
            };
            let size = camera.logical_viewport_size()?;
            let clip_from_world = camera.clip_from_view() * transform.to_matrix().inverse();
            // (Split screen's views draw into part of the picture.)
            let view = match (&camera.viewport, camera.physical_target_size()) {
                (Some(v), Some(whole)) if whole.x > 0 && whole.y > 0 => {
                    let whole = whole.as_vec2();
                    let at = v.physical_position.as_vec2() / whole;
                    let part = v.physical_size.as_vec2() / whole;
                    Vec4::new(at.x, at.y, part.x, part.y)
                }
                _ => WHOLE_VIEW,
            };
            shafts_for_view(
                light,
                clip_from_world,
                -light.direction,
                (size.x / size.y.max(1.0), view),
                p.near,
            )
        });
        match (shafts, had) {
            (Some(s), Some(h)) if s == *h => {}
            (Some(s), _) => {
                commands.entity(entity).insert(s);
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<GameLightShafts>();
            }
            (None, None) => {}
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct LightShaftsLabel;

/// The view's two filter buffers: the downsampled picture, then its blur.
#[derive(Component)]
struct LightShaftTextures {
    a: CachedTexture,
    b: CachedTexture,
}

const FILTER_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

fn prepare_shaft_textures(
    mut commands: Commands,
    mut cache: ResMut<TextureCache>,
    device: Res<RenderDevice>,
    views: Query<(Entity, &ViewTarget), With<GameLightShafts>>,
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
        commands.entity(entity).insert(LightShaftTextures {
            a: cache.get(&device, descriptor("light_shafts_a")),
            b: cache.get(&device, descriptor("light_shafts_b")),
        });
    }
}

#[derive(Default)]
struct LightShaftsNode;

impl ViewNode for LightShaftsNode {
    type ViewQuery = (
        &'static ViewTarget,
        &'static ViewDepthTexture,
        &'static LightShaftTextures,
        &'static DynamicUniformIndex<GameLightShafts>,
        &'static Msaa,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, depth, textures, settings_index, msaa): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let pipelines = world.resource::<ShaftPipelines>();
        let cache = world.resource::<PipelineCache>();
        let ms = usize::from(*msaa != Msaa::Off);
        let hdr = usize::from(view_target.is_hdr());
        let (Some(downsample), Some(blur), Some(apply)) = (
            cache.get_render_pipeline(pipelines.downsample[ms]),
            cache.get_render_pipeline(pipelines.blur),
            cache.get_render_pipeline(pipelines.apply[hdr]),
        ) else {
            return Ok(());
        };
        let uniforms = world.resource::<ComponentUniforms<GameLightShafts>>();
        let Some(settings) = uniforms.uniforms().binding() else {
            return Ok(());
        };
        let device = render_context.render_device().clone();
        let post_process = view_target.post_process_write();
        let a = &textures.a.default_view;
        let b = &textures.b.default_view;
        let offset = [settings_index.index()];

        let downsample_group = device.create_bind_group(
            "light_shafts_downsample",
            &cache.get_bind_group_layout(&pipelines.downsample_layout[ms]),
            &BindGroupEntries::sequential((post_process.source, depth.view(), settings.clone())),
        );
        let blur_group = device.create_bind_group(
            "light_shafts_blur",
            &cache.get_bind_group_layout(&pipelines.blur_layout),
            &BindGroupEntries::sequential((a, &pipelines.sampler, settings.clone())),
        );
        let apply_group = device.create_bind_group(
            "light_shafts_apply",
            &cache.get_bind_group_layout(&pipelines.apply_layout),
            &BindGroupEntries::sequential((
                post_process.source,
                b,
                &pipelines.sampler,
                settings.clone(),
            )),
        );

        let passes = [
            ("light_shafts_downsample", downsample, &downsample_group, a),
            ("light_shafts_blur", blur, &blur_group, b),
            (
                "light_shafts_apply",
                apply,
                &apply_group,
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

/// The three passes' pipelines: the downsample for a multisampled depth buffer or
/// not ([ms]), the blur, the apply for an HDR view or not ([hdr]).
#[derive(Resource)]
struct ShaftPipelines {
    downsample_layout: [BindGroupLayoutDescriptor; 2],
    blur_layout: BindGroupLayoutDescriptor,
    apply_layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    downsample: [CachedRenderPipelineId; 2],
    blur: CachedRenderPipelineId,
    apply: [CachedRenderPipelineId; 2],
}

fn init_shaft_pipelines(
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
    let settings = || uniform_buffer::<GameLightShafts>(true);
    let downsample_layout = [false, true].map(|ms| {
        BindGroupLayoutDescriptor::new(
            "light_shafts_downsample_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (picture(), depth(ms), settings()),
            ),
        )
    });
    let blur_layout = BindGroupLayoutDescriptor::new(
        "light_shafts_blur_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                picture(),
                sampler(SamplerBindingType::Filtering),
                settings(),
            ),
        ),
    );
    let apply_layout = BindGroupLayoutDescriptor::new(
        "light_shafts_apply_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                picture(),
                picture(),
                sampler(SamplerBindingType::Filtering),
                settings(),
            ),
        ),
    );
    let sampler = render_device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    let shader = asset_server.load("shaders/lightshafts.wgsl");
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
    let downsample = [0, 1].map(|ms| {
        let mut defs: Vec<ShaderDefVal> = vec!["DOWNSAMPLE".into()];
        if ms == 1 {
            defs.push("MULTISAMPLED".into());
        }
        queue(
            "light_shafts_downsample",
            &downsample_layout[ms],
            defs,
            FILTER_FORMAT,
        )
    });
    let blur = queue(
        "light_shafts_blur",
        &blur_layout,
        vec!["BLUR".into()],
        FILTER_FORMAT,
    );
    let apply = [
        TextureFormat::bevy_default(),
        ViewTarget::TEXTURE_FORMAT_HDR,
    ]
    .map(|format| {
        queue(
            "light_shafts_apply",
            &apply_layout,
            vec!["APPLY".into()],
            format,
        )
    });
    commands.insert_resource(ShaftPipelines {
        downsample_layout,
        blur_layout,
        apply_layout,
        sampler,
        downsample,
        blur,
        apply,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // The passes' numbers, worked out here as lightshafts.wgsl works them out, to
    // check them against the game's shaders.

    /// The blur's k-th sample's weight (alpha) and its colour's (BlurLightShafts).
    fn blur_weights(k: u32) -> (f32, f32) {
        let w = 2.0 - k as f32 / 24.0;
        (w, w.min(4.0 * w * w))
    }

    /// How far toward the sun the blur reaches from a texel `d` screen heights
    /// from it.
    fn blur_reach(d: f32) -> f32 {
        d.min((d / 2.0).sqrt())
    }

    /// The picture's scale where the blurred occlusion is `occlusion`, `d` screen
    /// heights from the sun (ApplyLightShafts' alpha).
    fn scene_scale(darkness: f32, occlusion: f32, d: f32, distance_fade: f32) -> f32 {
        let o2 = occlusion * occlusion;
        let open = o2 * (1.5 - 0.5 * darkness) + (1.0 - o2) * darkness;
        let fade = (d * 0.5).clamp(0.0, 1.0);
        let s = fade + (1.0 - fade) * open;
        s + distance_fade.powi(3) * (1.0 - s)
    }

    fn ancients() -> ShaftLight {
        ShaftLight {
            direction: Vec3::new(-0.0285, -0.7850, -0.6189).normalize(),
            tint: Color::srgb_u8(255, 235, 189),
            bloom_scale: 10.0,
            threshold: 0.0,
            screen_blend: 1.0,
            range: 200.0,
            darkness: 0.3,
        }
    }

    #[test]
    fn the_blur_weights_are_the_games() {
        // 48 samples, the first weighted 2, falling by 1/24 each.
        assert_eq!(blur_weights(0), (2.0, 2.0));
        let (w, c) = blur_weights(47);
        assert!((w - 2.0 / 48.0).abs() < 1e-6);
        assert!((c - 4.0 * w * w).abs() < 1e-6);
        // Their sum over 48: the occlusion stays about where it is.
        let sum: f32 = (0..48).map(|k| blur_weights(k).0).sum::<f32>() / 48.0;
        assert!((sum - 49.0 / 48.0).abs() < 1e-5);
        // Up to half a screen height away the blur reaches the sun; further, less.
        assert_eq!(blur_reach(0.3), 0.3);
        assert!((blur_reach(0.5) - 0.5).abs() < 1e-6);
        assert!((blur_reach(1.28) - 0.8).abs() < 1e-6);
    }

    #[test]
    fn blocked_light_darkens_and_open_sky_brightens_near_the_sun() {
        // At the sun: blocked, 0.3 of the picture; open, 1.35 of it.
        assert!((scene_scale(0.3, 0.0, 0.0, 0.0) - 0.3).abs() < 1e-6);
        assert!((scene_scale(0.3, 1.0, 0.0, 0.0) - 1.35).abs() < 1e-6);
        // Eased back to the picture two screen heights away, or as the sun leaves.
        assert_eq!(scene_scale(0.3, 0.0, 2.0, 0.0), 1.0);
        assert!((scene_scale(0.3, 0.0, 0.0, 1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_sun_is_found_on_the_screen() {
        let light = ancients();
        let to_sun = -light.direction;
        // A camera at the origin looking straight at the sun: it is mid-screen.
        let view = Transform::default().looking_to(to_sun, Vec3::Y);
        let proj = PerspectiveProjection {
            fov: 1.0,
            aspect_ratio: 16.0 / 9.0,
            ..default()
        };
        let clip_from_world =
            bevy::camera::CameraProjection::get_clip_from_view(&proj) * view.to_matrix().inverse();
        let s = shafts_for_view(
            &light,
            clip_from_world,
            to_sun,
            (16.0 / 9.0, WHOLE_VIEW),
            0.1,
        )
        .unwrap();
        assert!((s.origin - Vec2::new(0.5 * 16.0 / 9.0, 0.5)).length() < 1e-4);
        assert_eq!(s.distance_fade, 0.0);
        assert!((s.inv_range - 1.0 / 200.0).abs() < 1e-9);
        // Looking away from it: none.
        let away = Transform::default().looking_to(-to_sun, Vec3::Y);
        let clip_from_world =
            bevy::camera::CameraProjection::get_clip_from_view(&proj) * away.to_matrix().inverse();
        assert!(
            shafts_for_view(
                &light,
                clip_from_world,
                to_sun,
                (16.0 / 9.0, WHOLE_VIEW),
                0.1
            )
            .is_none()
        );
    }
}
