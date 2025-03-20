#![cfg_attr(target_arch = "spirv", no_std)]
use spirv_std::{
    glam::{vec2, vec3, Vec2, Vec3, Vec4},
    spirv,
};

#[allow(dead_code)]
#[spirv(vertex)]
pub fn vert_main(
    in_pos: Vec2,
    in_col: Vec3,
    #[spirv(position)] position: &mut Vec4,
    out: &mut Vec3,
) {
    *position = in_pos.extend(0.0).extend(1.0);
    *out = in_col;
}

#[allow(dead_code)]
#[spirv(fragment)]
pub fn frag_main(frag_color: Vec3, out: &mut Vec4) {
    *out = frag_color.extend(1.0);
}
