//! The map's parts joined into fewer, bigger meshes once it has come in.
//!
//! A map places its meshes thousands of times (2,200 to 4,200 parts), and nearly
//! every part has its own place on a light-map page. Bevy gives each light-mapped
//! entity a slot of its own in its light-map slabs (four to a slab) and makes a new
//! bind group for every slab, in every render phase, every frame: on the maps that
//! alone took a third of the frame, and each part was a draw of its own too.
//!
//! So once the map is in, its light-map pages are gathered onto one atlas (up to 49
//! pages to an atlas, `build_atlases`), and the parts that draw alike (one material,
//! one atlas, the same vertex layout and shadow flags) are joined into one mesh, in
//! the map's own space, with their light-map coordinates moved onto the atlas: a
//! part's rectangle there is put into its UV1, and the joined mesh's rectangle is
//! the whole atlas. See-through parts (sorted back to front, part by part) and the
//! effect meshes (levelfx.rs, given their own materials) are left as they are, and
//! take their places on the atlases; so are the spinning actors' (mapmotion.rs),
//! which turn on their own.
//!
//! A mirrored part (placed with a negative scale, as about a quarter of the maps'
//! parts are) has from Bevy's glTF loader a copy of its material that culls the
//! other side, its triangles winding the other way round on screen. Joined with that
//! copy, the mirrored parts made meshes (and took light-map slots) of their own: a
//! mirrored part is joined with the unmirrored parts of its material instead, its
//! triangles turned round (`PartMaterial::look`, and `join`'s `flip`). With one
//! atlas, that roughly halves the meshes (Downfall's from 252 to 119, Oblivion's
//! from 199 to 115, Ignition's from 194 to 85, Ancients' from 82 to 61), and so the
//! light-map slabs whose bind groups Bevy makes anew each frame.
//!
//! Bevy's light-map slots are given out as parts first show, and never handed back
//! for good (a slot freed is used again, but its slab stays, and is bound every
//! frame): so the map's scene comes in hidden (main.rs), and is shown once joined.

use bevy::asset::{LoadState, RenderAssetUsages, UntypedAssetId};
use bevy::camera::primitives::Aabb;
use bevy::gltf::GltfMaterialExtras;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::Affine3A;
use bevy::mesh::{Indices, MeshVertexAttribute, MeshVertexAttributeId, VertexAttributeValues};
use bevy::pbr::Lightmap;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, Face, PrimitiveTopology, TextureDimension};
use bevy::scene::{SceneInstance, SceneSpawner};
use std::collections::HashMap;

use crate::lighting::{FlipBooks, LightmapPages};
use crate::live::LiveMaterial;

/// The map's scene (main.rs puts it on the map's SceneRoot, hidden): its parts are
/// joined once it is in, and then it is shown.
#[derive(Component)]
pub struct MapScene;

/// A map scene whose parts have been joined.
#[derive(Component)]
struct Joined;

/// Frames to let pass after the map's scene is in, for its parts' light maps and
/// material flags (lighting.rs) and effect materials (levelfx.rs) to be set.
const SETTLE_FRAMES: u32 = 3;
/// Frames to wait at most for its light-map pages to load (then those still out are
/// left off the atlases).
const PAGE_WAIT_FRAMES: u32 = 900;

/// Light-map pages to an atlas (as 7 x 7 at most: 7196 texels a side, under the
/// 8192 any graphics card takes), and the largest page (texels a side). The maps
/// have 3 to 33.
const PAGES_PER_ATLAS: usize = 49;
const PAGE_TEXELS: u32 = 1024;
/// Texels around each page on an atlas, its edges' texels repeated: the light maps
/// are filtered, and a page's edge mustn't pick up its neighbour's.
const GUTTER: u32 = 2;

pub struct MergePlugin;

/// The joining (for what must happen to the map's parts before it: recreator.rs).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct JoinSystems;

impl Plugin for MergePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, join_map_parts.in_set(JoinSystems));
    }
}

/// A part's material: a standard one, or a live one (its glow pulsing in its
/// shader: live.rs).
#[derive(Clone)]
enum PartMaterial {
    Standard(Handle<StandardMaterial>),
    Live(Handle<LiveMaterial>),
}

impl PartMaterial {
    fn id(&self) -> UntypedAssetId {
        match self {
            PartMaterial::Standard(h) => h.id().untyped(),
            PartMaterial::Live(h) => h.id().untyped(),
        }
    }

    /// Drawn whole (opaque or masked), not see-through.
    fn opaque(&self, standard: &Assets<StandardMaterial>, live: &Assets<LiveMaterial>) -> bool {
        let mode = match self {
            PartMaterial::Standard(h) => standard.get(h).map(|m| m.alpha_mode),
            PartMaterial::Live(h) => live.get(h).map(|m| m.base.alpha_mode),
        };
        mode.is_some_and(|m| matches!(m, AlphaMode::Opaque | AlphaMode::Mask(_)))
    }

    /// How it draws (everything it holds, written out), as an unmirrored part's
    /// when `unmirror`: a mirrored part's copy that culls the front then culls the
    /// back, as its twin does (a two-sided copy is its twin as it is). And whether
    /// that turned its culling round (the part's triangles must be turned round
    /// too). None for one not in.
    fn look(
        &self,
        standard: &Assets<StandardMaterial>,
        live: &Assets<LiveMaterial>,
        unmirror: bool,
    ) -> Option<(String, bool)> {
        let turn = |cull: &mut Option<Face>| {
            let turned = unmirror && *cull == Some(Face::Front);
            if turned {
                *cull = Some(Face::Back);
            }
            turned
        };
        match self {
            PartMaterial::Standard(h) => {
                let mut m = standard.get(h)?.clone();
                let turned = turn(&mut m.cull_mode);
                Some((format!("{m:?}"), turned))
            }
            PartMaterial::Live(h) => {
                let mut m = live.get(h)?.clone();
                let turned = turn(&mut m.base.cull_mode);
                Some((format!("{m:?}"), turned))
            }
        }
    }
}

/// What makes parts drawable as one.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    material: UntypedAssetId,
    lightmap: Option<AssetId<Image>>,
    no_shadow_cast: bool,
    no_shadow_receive: bool,
    layout: Vec<MeshVertexAttributeId>,
}

/// A part to join: its mesh, its place in the map, its rectangle on its light map
/// (its atlas), and whether its triangles are to be turned round (a mirrored part
/// joined with its material's unmirrored parts: `PartMaterial::look`).
struct Part {
    entity: Entity,
    parent: Option<Entity>,
    mesh: AssetId<Mesh>,
    to_map: Affine3A,
    rect: Option<Rect>,
    flip: bool,
}

/// The parts that join into one mesh, and what that mesh is drawn with.
struct Group {
    material: PartMaterial,
    lightmap: Option<Handle<Image>>,
    parts: Vec<Part>,
}

/// Where a light-map page went: its atlas, and how a place on the page maps to one
/// on the atlas (`offset + uv * scale`).
#[derive(Clone)]
struct OnAtlas {
    image: Handle<Image>,
    offset: Vec2,
    scale: Vec2,
}

impl OnAtlas {
    fn rect(&self, r: Rect) -> Rect {
        Rect {
            min: self.offset + r.min * self.scale,
            max: self.offset + r.max * self.scale,
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn join_map_parts(
    mut commands: Commands,
    roots: Query<(Entity, &SceneInstance, &GlobalTransform), (With<MapScene>, Without<Joined>)>,
    spawner: Res<SceneSpawner>,
    mut waited: Local<HashMap<Entity, u32>>,
    children: Query<&Children>,
    parts: Query<(
        &Mesh3d,
        AnyOf<(
            &MeshMaterial3d<StandardMaterial>,
            &MeshMaterial3d<LiveMaterial>,
        )>,
        &GlobalTransform,
        Option<&Lightmap>,
        Option<&GltfMaterialExtras>,
        Option<&Visibility>,
        Option<&ChildOf>,
        Has<NotShadowCaster>,
        Has<NotShadowReceiver>,
    )>,
    lit: Query<&Lightmap>,
    spinning: Query<(), With<crate::mapmotion::Spinning>>,
    mut meshes: ResMut<Assets<Mesh>>,
    (materials, live): (Res<Assets<StandardMaterial>>, Res<Assets<LiveMaterial>>),
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
    (mut pages, books): (ResMut<LightmapPages>, Res<FlipBooks>),
) {
    waited.retain(|root, _| roots.contains(*root));
    for (root, instance, root_transform) in &roots {
        if !spawner.instance_is_ready(**instance) {
            continue;
        }
        let frames = waited.entry(root).or_insert(0);
        *frames += 1;
        if *frames < SETTLE_FRAMES {
            continue;
        }
        // Its light-map pages, in to be put on atlases (by name, for a steady order).
        let mut page_ids: Vec<AssetId<Image>> = Vec::new();
        for entity in children.iter_descendants(root) {
            if let Ok(lightmap) = lit.get(entity)
                && !page_ids.contains(&lightmap.image.id())
            {
                page_ids.push(lightmap.image.id());
            }
        }
        let loading = page_ids.iter().any(|&id| {
            images.get(id).is_none() && !matches!(asset_server.load_state(id), LoadState::Failed(_))
        });
        if loading && *frames < SETTLE_FRAMES + PAGE_WAIT_FRAMES {
            continue;
        }
        waited.remove(&root);
        commands.entity(root).insert(Joined);
        page_ids.sort_by_key(|&id| asset_server.get_path(id).map(|p| p.to_string()));
        let atlases = build_atlases(&page_ids, &mut images);
        let on_atlas = |l: &Lightmap| -> (Handle<Image>, Rect) {
            match atlases.get(&l.image.id()) {
                Some(a) => (a.image.clone(), a.rect(l.uv_rect)),
                None => (l.image.clone(), l.uv_rect),
            }
        };

        let to_root = root_transform.affine().inverse();
        let mirrored = |t: &GlobalTransform| (to_root * t.affine()).matrix3.determinant() < 0.0;
        // How each material draws, as an unmirrored part's or not (`look`).
        let mut looks: HashMap<(UntypedAssetId, bool), Option<(String, bool)>> = HashMap::new();
        let mut look = |material: &PartMaterial, unmirror: bool| {
            looks
                .entry((material.id(), unmirror))
                .or_insert_with(|| material.look(&materials, &live, unmirror))
                .clone()
        };
        // The unmirrored parts' materials, by how they draw: the twins of the
        // mirrored parts' copies (but for flip books, each turning its own pages).
        let book = |m: &PartMaterial| matches!(m, PartMaterial::Standard(h) if books.has(h.id()));
        let mut twins: HashMap<String, PartMaterial> = HashMap::new();
        for entity in children.iter_descendants(root) {
            let Ok((_, material, transform, ..)) = parts.get(entity) else {
                continue;
            };
            let material = match material {
                (Some(m), _) => PartMaterial::Standard(m.0.clone()),
                (None, Some(m)) => PartMaterial::Live(m.0.clone()),
                (None, None) => continue,
            };
            if !mirrored(transform)
                && !book(&material)
                && let Some((key, _)) = look(&material, false)
            {
                twins.entry(key).or_insert(material);
            }
        }
        // (In the order the map has them, which is kept: the order things are drawn
        // in decides between surfaces that lie exactly on each other.)
        let mut groups: Vec<(Key, Group)> = Vec::new();
        let mut group_of: HashMap<Key, usize> = HashMap::new();
        let mut kept = 0usize;
        let mut turned = 0usize;
        for entity in children.iter_descendants(root) {
            let Ok((
                mesh,
                material,
                transform,
                lightmap,
                extras,
                visibility,
                parent,
                no_cast,
                no_receive,
            )) = parts.get(entity)
            else {
                continue;
            };
            let lightmap = lightmap.map(on_atlas);
            let mut material = match material {
                (Some(m), _) => PartMaterial::Standard(m.0.clone()),
                (None, Some(m)) => PartMaterial::Live(m.0.clone()),
                (None, None) => continue,
            };
            let opaque = material.opaque(&materials, &live);
            let effect = extras.is_some_and(|e| e.value.contains("fx:"));
            // (A spinning actor's, which turns on its own: mapmotion.rs.)
            let spins = parent.is_some_and(|p| spinning.contains(p.parent()));
            let hidden = visibility == Some(&Visibility::Hidden);
            // (Its vertex data must be at hand: not only in the render world.)
            let data = meshes.get(&mesh.0).filter(|m| m.try_attributes().is_ok());
            let joinable = data.is_some_and(|data| {
                data.primitive_topology() == PrimitiveTopology::TriangleList
                    && data.indices().is_some()
                    && !data.has_morph_targets()
                    && !data.contains_attribute(Mesh::ATTRIBUTE_JOINT_INDEX)
                    && data.contains_attribute(Mesh::ATTRIBUTE_POSITION)
                    && (lightmap.is_none() || data.contains_attribute(Mesh::ATTRIBUTE_UV_1))
            });
            let (Some(data), true) = (data, opaque && !effect && !hidden && !spins && joinable)
            else {
                // Left as it is, its light map on the atlas.
                if let Some((image, uv_rect)) = lightmap {
                    commands.entity(entity).insert(Lightmap {
                        image,
                        uv_rect,
                        bicubic_sampling: false,
                    });
                }
                kept += 1;
                continue;
            };
            // A mirrored part with its unmirrored twin's material, its triangles
            // turned round if its copy culled the other side.
            let mut flip = false;
            if mirrored(transform)
                && !book(&material)
                && let Some((key, turn)) = look(&material, true)
                && let Some(twin) = twins.get(&key)
                && twin.id() != material.id()
            {
                material = twin.clone();
                flip = turn;
                turned += 1;
            }
            let key = Key {
                material: material.id(),
                lightmap: lightmap.as_ref().map(|(image, _)| image.id()),
                no_shadow_cast: no_cast,
                no_shadow_receive: no_receive,
                layout: data.attributes().map(|(a, _)| a.id).collect(),
            };
            let at = *group_of.entry(key.clone()).or_insert_with(|| {
                groups.push((
                    key,
                    Group {
                        material: material.clone(),
                        lightmap: lightmap.as_ref().map(|(image, _)| image.clone()),
                        parts: Vec::new(),
                    },
                ));
                groups.len() - 1
            });
            groups[at].1.parts.push(Part {
                entity,
                parent: parent.map(ChildOf::parent),
                mesh: mesh.0.id(),
                to_map: to_root * transform.affine(),
                rect: lightmap.map(|(_, rect)| rect),
                flip,
            });
        }

        // Join each group (a group of one gains nothing, but is moved onto its atlas
        // all the same, to keep one way of drawing the map).
        let mut joined: Vec<(Entity, Option<Entity>)> = Vec::new();
        let mut made = 0usize;
        for (key, group) in groups {
            let Some((mesh, min, max)) = join(&group.parts, &meshes) else {
                for part in &group.parts {
                    if let Ok(l) = lit.get(part.entity) {
                        let (image, uv_rect) = on_atlas(l);
                        commands.entity(part.entity).insert(Lightmap {
                            image,
                            uv_rect,
                            bicubic_sampling: false,
                        });
                    }
                }
                kept += group.parts.len();
                continue;
            };
            let mut e = commands.spawn((
                Mesh3d(meshes.add(mesh)),
                Transform::IDENTITY,
                Aabb::from_min_max(min, max),
                ChildOf(root),
            ));
            match group.material {
                PartMaterial::Standard(m) => e.insert(MeshMaterial3d(m)),
                PartMaterial::Live(m) => e.insert(MeshMaterial3d(m)),
            };
            if let Some(image) = group.lightmap {
                e.insert(Lightmap {
                    image,
                    uv_rect: Rect::new(0.0, 0.0, 1.0, 1.0),
                    bicubic_sampling: false,
                });
            }
            if key.no_shadow_cast {
                e.insert(NotShadowCaster);
            }
            if key.no_shadow_receive {
                e.insert(NotShadowReceiver);
            }
            made += 1;
            joined.extend(group.parts.iter().map(|p| (p.entity, p.parent)));
        }
        // The joined parts go, and so do the nodes they leave empty (with them).
        let mut left: HashMap<Entity, usize> = HashMap::new();
        for &(_, parent) in &joined {
            if let Some(parent) = parent.filter(|&p| p != root) {
                let n = left
                    .entry(parent)
                    .or_insert_with(|| children.get(parent).map_or(0, |c| c.len()));
                *n = n.saturating_sub(1);
            }
        }
        for &(part, parent) in &joined {
            if !parent.is_some_and(|p| left.get(&p) == Some(&0)) {
                commands.entity(part).despawn();
            }
        }
        for (node, n) in left {
            if n == 0 {
                commands.entity(node).despawn();
            }
        }
        // (The pages on atlases are needed no more.)
        if !atlases.is_empty() {
            pages.clear();
        }
        commands.entity(root).insert(Visibility::Inherited);
        let atlas_count = {
            let mut ids: Vec<AssetId<Image>> = atlases.values().map(|a| a.image.id()).collect();
            ids.sort();
            ids.dedup();
            ids.len()
        };
        info!(
            "The map's {} opaque parts joined into {made} meshes ({kept} left as they were; \
             {turned} mirrored parts with their unmirrored twins' materials); \
             {} light-map pages on {atlas_count} atlases",
            joined.len(),
            atlases.len(),
        );
    }
}

/// Copies the light-map pages onto atlases (`PAGES_PER_ATLAS` to one, each page in a
/// square of `PAGE_TEXELS` and its gutter), and says where each went. Pages not in,
/// or unlike the first (in format, or bigger), are left off.
fn build_atlases(
    ids: &[AssetId<Image>],
    images: &mut Assets<Image>,
) -> HashMap<AssetId<Image>, OnAtlas> {
    let mut out = HashMap::new();
    let mut fits: Vec<AssetId<Image>> = Vec::new();
    let mut format = None;
    for &id in ids {
        let Some(page) = images.get(id) else {
            continue;
        };
        let d = &page.texture_descriptor;
        let f = d.format;
        let texel = f
            .block_copy_size(None)
            .filter(|_| f.block_dimensions() == (1, 1));
        let fine = d.dimension == TextureDimension::D2
            && d.size.depth_or_array_layers == 1
            && d.mip_level_count == 1
            && d.size.width <= PAGE_TEXELS
            && d.size.height <= PAGE_TEXELS
            && *format.get_or_insert(f) == f
            && texel
                .zip(page.data.as_ref())
                .is_some_and(|(t, data)| data.len() == (d.size.width * d.size.height * t) as usize);
        if fine {
            fits.push(id);
        }
    }
    let Some(format) = format else {
        return out;
    };
    let Some(texel) = format.block_copy_size(None).map(|t| t as usize) else {
        return out;
    };
    let slot = (PAGE_TEXELS + 2 * GUTTER) as usize;
    let g = GUTTER as usize;
    for chunk in fits.chunks(PAGES_PER_ATLAS) {
        let cols = (chunk.len() as f32).sqrt().ceil() as usize;
        let rows = chunk.len().div_ceil(cols);
        let (aw, ah) = (cols * slot, rows * slot);
        let mut data = vec![0u8; aw * ah * texel];
        let mut placed: Vec<(AssetId<Image>, Vec2, Vec2)> = Vec::new();
        let mut sampler = None;
        for (k, &id) in chunk.iter().enumerate() {
            let Some(page) = images.get(id) else {
                continue;
            };
            let Some(src) = page.data.as_ref() else {
                continue;
            };
            sampler.get_or_insert_with(|| page.sampler.clone());
            let w = page.texture_descriptor.size.width as usize;
            let h = page.texture_descriptor.size.height as usize;
            let (sx, sy) = ((k % cols) * slot, (k / cols) * slot);
            // Its rows, each with its first and last texels repeated into the gutter.
            for y in 0..h {
                let row = &src[y * w * texel..(y + 1) * w * texel];
                let at = ((sy + g + y) * aw + sx) * texel;
                for i in 0..g {
                    data[at + i * texel..at + (i + 1) * texel].copy_from_slice(&row[..texel]);
                    let right = at + (g + w + i) * texel;
                    data[right..right + texel].copy_from_slice(&row[(w - 1) * texel..]);
                }
                data[at + g * texel..at + (g + w) * texel].copy_from_slice(row);
            }
            // Then its first and last rows (gutters and all) repeated above and below.
            let span = (2 * g + w) * texel;
            let row_at = |y: usize| (y * aw + sx) * texel;
            for i in 0..g {
                let (from, to) = (row_at(sy + g), row_at(sy + i));
                data.copy_within(from..from + span, to);
                let (from, to) = (row_at(sy + g + h - 1), row_at(sy + g + h + i));
                data.copy_within(from..from + span, to);
            }
            let size = Vec2::new(aw as f32, ah as f32);
            placed.push((
                id,
                Vec2::new((sx + g) as f32, (sy + g) as f32) / size,
                Vec2::new(w as f32, h as f32) / size,
            ));
        }
        let mut atlas = Image::new(
            Extent3d {
                width: aw as u32,
                height: ah as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            format,
            RenderAssetUsages::RENDER_WORLD,
        );
        if let Some(sampler) = sampler {
            atlas.sampler = sampler;
        }
        let image = images.add(atlas);
        for (id, offset, scale) in placed {
            out.insert(
                id,
                OnAtlas {
                    image: image.clone(),
                    offset,
                    scale,
                },
            );
        }
    }
    out
}

/// The parts' meshes as one, in the map's space (with each part's light-map
/// rectangle put into its UV1), and its bounds. None if their vertex data differ
/// in kind.
fn join(parts: &[Part], meshes: &Assets<Mesh>) -> Option<(Mesh, Vec3, Vec3)> {
    let first = meshes.get(parts.first()?.mesh)?;
    let layout: Vec<MeshVertexAttribute> = first.try_attributes().ok()?.map(|(a, _)| *a).collect();
    let mut total_vertices = 0;
    let mut total_indices = 0;
    for part in parts {
        let mesh = meshes.get(part.mesh)?;
        total_vertices += mesh.count_vertices();
        total_indices += mesh.indices()?.len();
    }
    let mut out: Vec<VertexAttributeValues> = Vec::with_capacity(layout.len());
    let mut indices: Vec<u32> = Vec::with_capacity(total_indices);
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for (i, part) in parts.iter().enumerate() {
        let mesh = meshes.get(part.mesh)?;
        let start = u32::try_from(out.first().map_or(0, VertexAttributeValues::len)).ok()?;
        let m3 = Mat3::from(part.to_map.matrix3);
        let mirrored = m3.determinant() < 0.0;
        let normal_m = m3.inverse().transpose();
        for (k, attr) in layout.iter().enumerate() {
            let values = mesh.try_attribute(attr.id).ok()?;
            let moved = if attr.id == Mesh::ATTRIBUTE_POSITION.id {
                let VertexAttributeValues::Float32x3(v) = values else {
                    return None;
                };
                VertexAttributeValues::Float32x3(
                    v.iter()
                        .map(|p| {
                            let p = part.to_map.transform_point3(Vec3::from_array(*p));
                            min = min.min(p);
                            max = max.max(p);
                            p.to_array()
                        })
                        .collect(),
                )
            } else if attr.id == Mesh::ATTRIBUTE_NORMAL.id {
                let VertexAttributeValues::Float32x3(v) = values else {
                    return None;
                };
                VertexAttributeValues::Float32x3(
                    v.iter()
                        .map(|n| {
                            (normal_m * Vec3::from_array(*n))
                                .normalize_or_zero()
                                .to_array()
                        })
                        .collect(),
                )
            } else if attr.id == Mesh::ATTRIBUTE_TANGENT.id {
                let VertexAttributeValues::Float32x4(v) = values else {
                    return None;
                };
                // (Mirrored, the bitangent turns over with the normal and tangent:
                // its sign keeps it the way the texture's v runs, as Bevy's shader
                // does for a mirrored part by its transform's sign.)
                let sign = if mirrored { -1.0 } else { 1.0 };
                VertexAttributeValues::Float32x4(
                    v.iter()
                        .map(|t| {
                            let d = (m3 * Vec3::new(t[0], t[1], t[2])).normalize_or_zero();
                            [d.x, d.y, d.z, t[3] * sign]
                        })
                        .collect(),
                )
            } else if attr.id == Mesh::ATTRIBUTE_UV_1.id
                && let Some(rect) = part.rect
            {
                let VertexAttributeValues::Float32x2(v) = values else {
                    return None;
                };
                let size = rect.max - rect.min;
                VertexAttributeValues::Float32x2(
                    v.iter()
                        .map(|uv| (rect.min + Vec2::from_array(*uv) * size).to_array())
                        .collect(),
                )
            } else {
                values.clone()
            };
            if i == 0 {
                out.push(moved);
            } else if !append(&mut out[k], moved) {
                return None;
            }
        }
        // (A mirrored part's triangles, mirrored, wind the other way round on screen,
        // and Bevy's glTF loader gives it a copy of its material that culls the
        // other side: joined with that copy, they are left so; joined with the
        // unmirrored parts' material (`flip`), they are turned round.)
        let at = indices.len();
        for index in mesh.indices()?.iter() {
            indices.push(start + u32::try_from(index).ok()?);
        }
        if part.flip {
            for tri in indices[at..].chunks_exact_mut(3) {
                tri.swap(1, 2);
            }
        }
    }
    debug_assert_eq!(
        out.first().map_or(0, VertexAttributeValues::len),
        total_vertices
    );
    let mut joined = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    for (attr, values) in layout.into_iter().zip(out) {
        joined.insert_attribute(attr, values);
    }
    joined.insert_indices(Indices::U32(indices));
    Some((joined, min, max))
}

/// Adds `more` to the end of `to`; false if they hold different kinds of value.
fn append(to: &mut VertexAttributeValues, more: VertexAttributeValues) -> bool {
    use VertexAttributeValues as V;
    match (to, more) {
        (V::Float32(a), V::Float32(b)) => a.extend(b),
        (V::Sint32(a), V::Sint32(b)) => a.extend(b),
        (V::Uint32(a), V::Uint32(b)) => a.extend(b),
        (V::Float32x2(a), V::Float32x2(b)) => a.extend(b),
        (V::Sint32x2(a), V::Sint32x2(b)) => a.extend(b),
        (V::Uint32x2(a), V::Uint32x2(b)) => a.extend(b),
        (V::Float32x3(a), V::Float32x3(b)) => a.extend(b),
        (V::Sint32x3(a), V::Sint32x3(b)) => a.extend(b),
        (V::Uint32x3(a), V::Uint32x3(b)) => a.extend(b),
        (V::Float32x4(a), V::Float32x4(b)) => a.extend(b),
        (V::Sint32x4(a), V::Sint32x4(b)) => a.extend(b),
        (V::Uint32x4(a), V::Uint32x4(b)) => a.extend(b),
        (V::Sint16x2(a), V::Sint16x2(b)) => a.extend(b),
        (V::Snorm16x2(a), V::Snorm16x2(b)) => a.extend(b),
        (V::Uint16x2(a), V::Uint16x2(b)) => a.extend(b),
        (V::Unorm16x2(a), V::Unorm16x2(b)) => a.extend(b),
        (V::Sint16x4(a), V::Sint16x4(b)) => a.extend(b),
        (V::Snorm16x4(a), V::Snorm16x4(b)) => a.extend(b),
        (V::Uint16x4(a), V::Uint16x4(b)) => a.extend(b),
        (V::Unorm16x4(a), V::Unorm16x4(b)) => a.extend(b),
        (V::Sint8x2(a), V::Sint8x2(b)) => a.extend(b),
        (V::Snorm8x2(a), V::Snorm8x2(b)) => a.extend(b),
        (V::Uint8x2(a), V::Uint8x2(b)) => a.extend(b),
        (V::Unorm8x2(a), V::Unorm8x2(b)) => a.extend(b),
        (V::Sint8x4(a), V::Sint8x4(b)) => a.extend(b),
        (V::Snorm8x4(a), V::Snorm8x4(b)) => a.extend(b),
        (V::Uint8x4(a), V::Uint8x4(b)) => a.extend(b),
        (V::Unorm8x4(a), V::Unorm8x4(b)) => a.extend(b),
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mirrored part's copy of its material looks like its twin's once its
    /// culling is turned round (and a two-sided copy as it is); another material
    /// doesn't.
    #[test]
    fn a_mirrored_copy_looks_like_its_twin() {
        let mut standard = Assets::<StandardMaterial>::default();
        let live = Assets::<LiveMaterial>::default();
        let base = StandardMaterial {
            base_color: Color::srgb(0.5, 0.4, 0.3),
            cull_mode: Some(Face::Back),
            ..default()
        };
        let copy = StandardMaterial {
            cull_mode: Some(Face::Front),
            ..base.clone()
        };
        let other = StandardMaterial {
            base_color: Color::srgb(0.5, 0.4, 0.2),
            ..copy.clone()
        };
        let two_sided = StandardMaterial {
            cull_mode: None,
            double_sided: true,
            ..base.clone()
        };
        let [base, copy, other, two_sided, two_sided_copy] =
            [base, copy, other, two_sided.clone(), two_sided]
                .map(|m| PartMaterial::Standard(standard.add(m)));
        let look = |m: &PartMaterial, unmirror| m.look(&standard, &live, unmirror).unwrap();
        let (twin, _) = look(&base, false);
        assert_eq!(look(&copy, true), (twin.clone(), true));
        // (Unmirrored, the copy is a material of its own.)
        assert_ne!(look(&copy, false).0, twin);
        assert_ne!(look(&other, true).0, twin);
        let (two, _) = look(&two_sided, false);
        assert_eq!(look(&two_sided_copy, true), (two, false));
    }

    /// Joined, a mirrored part turned round winds its triangles the other way
    /// (its corners the same), and its vertices are mirrored into place.
    #[test]
    fn mirrored_parts_join_turned_round() {
        let mut meshes = Assets::<Mesh>::default();
        let mut triangle = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        triangle.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        );
        triangle.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 3]);
        triangle.insert_indices(Indices::U32(vec![0, 1, 2]));
        let mesh = meshes.add(triangle).id();
        let part = |to_map: Affine3A, flip: bool| Part {
            entity: Entity::PLACEHOLDER,
            parent: None,
            mesh,
            to_map,
            rect: None,
            flip,
        };
        let mirror = Affine3A::from_scale(Vec3::new(-1.0, 1.0, 1.0));
        let (joined, min, max) = join(
            &[
                part(Affine3A::IDENTITY, false),
                part(mirror, true),
                part(mirror, false),
            ],
            &meshes,
        )
        .unwrap();
        let indices: Vec<usize> = joined.indices().unwrap().iter().collect();
        assert_eq!(indices, [0, 1, 2, 3, 5, 4, 6, 7, 8]);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            joined.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        assert_eq!(positions[4], [-1.0, 0.0, 0.0]);
        assert_eq!(
            (min, max),
            (Vec3::new(-1.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.0))
        );
    }
}
