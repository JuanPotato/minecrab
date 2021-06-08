use std::mem::size_of;

use wgpu::VertexAttribute;

pub trait Vertex {
    fn descriptor() -> wgpu::VertexBufferLayout<'static>;
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PlainVertex {
    pub position: [f32; 3],
    pub texture_coordinates: [f32; 2],
    pub normal: [f32; 3],
}

const PLAIN_VERTEX_ATTRIBUTES: &[VertexAttribute] = &wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Float32x2,
    2 => Float32x3,
];

impl Vertex for PlainVertex {
    fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: PLAIN_VERTEX_ATTRIBUTES,
        }
    }
}

/// Vertex used to represent HUD vertices.
///
/// A vertex with a 2D position and no normal, for representing UI elements.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct HudVertex {
    pub position: [f32; 2],
    pub texture_coordinates: [f32; 2],
    pub texture_index: i32,
}

const HUD_VERTEX_ATTRIBUTES: &[VertexAttribute] = &wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x2,
    2 => Sint32,
];

impl Vertex for HudVertex {
    fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: HUD_VERTEX_ATTRIBUTES,
        }
    }
}

/// Vertex used to represent block vertices.
///
/// Aside from the usual vertex position, texture coordinates and normal, this "vertex" also
/// contains whether the block is highlighted (i.e. the player is pointing at the block) and its
/// texture index (to address the texture arrays)
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlockVertex {
    pub position: [f32; 3],
    pub texture_coordinates: [f32; 2],
    pub normal: [f32; 3],
    pub highlighted: i32,
    pub texture_id: i32,
}

const BLOCK_VERTEX_ATTRIBUTES: &[VertexAttribute] = &wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Float32x2,
    2 => Float32x3,
    3 => Sint32,
    4 => Sint32,
];

impl Vertex for BlockVertex {
    fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: BLOCK_VERTEX_ATTRIBUTES,
        }
    }
}
