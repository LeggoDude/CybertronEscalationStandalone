// The pickups' glowing outline shells (models/weapons, models/props: PickupOutline,
// src/outline.rs): Bevy's own mesh vertex shader, but every corner of the shell is
// moved back along its line of sight to the back of the pickup's bounding box (its
// far side as the camera sees it). Where it is seen it stays where it was on the
// screen, the same width; but it now lies behind every part of the pickup, so the
// pickup hides it wherever it covers it and it shows only round the pickup's
// silhouette, as the game draws its outline, not along the edges of its parts in
// front of one another. A pickup with a plane behind it (src/outline.rs's
// OutlineBackstop: a pod's back wall, which a weapon's back is in) goes back no
// further than just in front of it.

#import bevy_pbr::{
    mesh_functions,
    skinning,
    forward_io::{Vertex, VertexOutput},
    mesh_view_bindings::view,
}

// The box round the shell in its mesh's space: its middle (xyz) and half its size
// along each of the mesh's axes (xyz).
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> outline_centre: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<uniform> outline_extent: vec4<f32>;
// The plane it goes back no further than, in world space: in front where
// dot(xyz, p) + w > 0; none if xyz is zero.
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<uniform> outline_backstop: vec4<f32>;

// How far in front of that plane it stops (metres).
const BACKSTOP_GAP: f32 = 0.02;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let mesh_world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
#ifdef SKINNED
    var world_from_local = skinning::skin_model(
        vertex.joint_indices,
        vertex.joint_weights,
        vertex.instance_index
    );
#else
    var world_from_local = mesh_world_from_local;
#endif

#ifdef VERTEX_NORMALS
#ifdef SKINNED
    out.world_normal = skinning::skin_normals(world_from_local, vertex.normal);
#else
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index
    );
#endif
#endif

#ifdef VERTEX_POSITIONS
    out.world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0)
    );
    // The box's back, as deep as any of the pickup is (view space looks down -z):
    // its middle's depth less how deep each of its axes reaches.
    let view_from_local = view.view_from_world * mesh_world_from_local;
    let centre = view_from_local * vec4<f32>(outline_centre.xyz, 1.0);
    let reach = abs(view_from_local[0].z) * outline_extent.x
        + abs(view_from_local[1].z) * outline_extent.y
        + abs(view_from_local[2].z) * outline_extent.z;
    let back = centre.z - reach;
    var at = view.view_from_world * vec4<f32>(out.world_position.xyz, 1.0);
    // (Moved along the line from the eye: the same place on the screen. Only
    // nearer corners move, and none behind the eye.)
    if at.z > back && at.z < -1e-4 && back < 0.0 {
        var k = back / at.z;
        // (Not past the plane behind it, seen from in front of it: along the line
        // from the eye a place is that much further from the eye, and the plane's
        // value there `eye + k * (here - eye)`.)
        let n = outline_backstop.xyz;
        if dot(n, n) > 0.0 {
            let eye = dot(n, view.world_position) + outline_backstop.w;
            let here = dot(n, out.world_position.xyz) + outline_backstop.w;
            if eye > BACKSTOP_GAP && here < eye {
                k = clamp((eye - BACKSTOP_GAP) / (eye - here), 1.0, k);
            }
        }
        at = vec4<f32>(at.xyz * k, 1.0);
    }
    out.position = view.clip_from_view * at;
#endif

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif

#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        world_from_local,
        vertex.tangent,
        vertex.instance_index
    );
#endif

#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif

#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index, mesh_world_from_local[3]);
#endif

    return out;
}
