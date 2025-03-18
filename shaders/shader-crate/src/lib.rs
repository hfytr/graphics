#![cfg_attr(target_arch = "spirv", no_std)]
use spirv_std::{
    glam::{vec2, vec3, Vec2, Vec3, Vec4},
    spirv,
};

const POSITIONS: [Vec2; 3] = [vec2(0.0, -0.5), vec2(0.5, 0.5), vec2(-0.5, 0.5)];

const COLORS: [Vec3; 3] = [
    vec3(1.0, 0.0, 0.0),
    vec3(0.0, 1.0, 0.0),
    vec3(0.0, 0.0, 1.0),
];

#[allow(dead_code)]
#[spirv(vertex)]
pub fn vert_main(
    #[spirv(vertex_index)] vert_ind: usize,
    #[spirv(position)] position: &mut Vec4,
    out: &mut Vec3,
) {
    *position = POSITIONS[vert_ind].extend(0.0).extend(1.0);
    *out = COLORS[vert_ind];
}

#[allow(dead_code)]
#[spirv(fragment)]
pub fn frag_main(frag_color: Vec3, out: &mut Vec4) {
    *out = frag_color.extend(1.0);
}
