use crate::texture::TextureManager;

pub struct RenderContext {
    pub surface: wgpu::Surface,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub format: wgpu::TextureFormat,
    pub texture_manager: Option<TextureManager>,
}
