use std::{mem::size_of, time::Instant};

use ahash::AHashMap;
use cgmath::{Vector3, Zero};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BufferAddress, BufferDescriptor,
};
use winit::dpi::PhysicalSize;

use crate::{
    camera::{Camera, Projection},
    chunk::{Block, BlockType, Chunk},
    cube,
    instance::Instance,
    light::Light,
    texture::Texture,
    uniforms::Uniforms,
    vertex::Vertex,
    world::World,
};

pub struct WorldState {
    pub render_pipeline: wgpu::RenderPipeline,
    pub uniforms: Uniforms,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_bind_group: wgpu::BindGroup,
    pub texture_bind_groups: AHashMap<BlockType, wgpu::BindGroup>,
    pub camera: Camera,
    pub projection: Projection,
    pub instance_lists: Vec<(BlockType, Vector3<i32>, Vec<Instance>)>,
    pub vertex_buffer: wgpu::Buffer,
    pub vertex_buffer_grass: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub instance_buffers: AHashMap<(BlockType, Vector3<i32>), wgpu::Buffer>,
    pub depth_texture: Texture,
    pub light_bind_group: wgpu::BindGroup,
    pub world: World,
}

impl WorldState {
    fn create_textures(
        render_device: &wgpu::Device,
        render_queue: &wgpu::Queue,
    ) -> (wgpu::BindGroupLayout, AHashMap<BlockType, wgpu::BindGroup>) {
        let sampler = render_device.create_sampler(&wgpu::SamplerDescriptor::default());

        let bind_group_layout =
            render_device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("texture_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Sampler {
                            comparison: false,
                            filtering: true,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let bind_groups: AHashMap<BlockType, wgpu::BindGroup> = [
            (BlockType::Cobblestone, "assets/block/cobblestone.png"),
            (BlockType::Dirt, "assets/block/dirt.png"),
            (BlockType::Grass, "assets/block_temp/grass.png"),
            (BlockType::Stone, "assets/block/stone.png"),
        ]
        .iter()
        .map(|(block_type, texture_path)| {
            let bytes = std::fs::read(texture_path).unwrap();
            let texture =
                Texture::from_bytes(render_device, render_queue, &bytes, "block texture").unwrap();

            (
                *block_type,
                render_device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("block texture bind group"),
                    layout: &bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&texture.view),
                        },
                    ],
                }),
            )
        })
        .collect();

        (bind_group_layout, bind_groups)
    }

    fn create_camera(swap_chain_descriptor: &wgpu::SwapChainDescriptor) -> (Camera, Projection) {
        let camera = Camera::new(
            (0.0, 80.0, 0.0).into(),
            cgmath::Deg(0.0).into(),
            cgmath::Deg(-20.0).into(),
        );

        let projection = Projection::new(
            swap_chain_descriptor.width,
            swap_chain_descriptor.height,
            cgmath::Deg(45.0),
            0.1,
            500.0,
        );

        (camera, projection)
    }

    fn create_uniforms(
        camera: &Camera,
        projection: &Projection,
        render_device: &wgpu::Device,
    ) -> (
        Uniforms,
        wgpu::Buffer,
        wgpu::BindGroupLayout,
        wgpu::BindGroup,
    ) {
        let mut uniforms = Uniforms::new();
        uniforms.update_view_projection(camera, projection);

        let uniform_buffer = render_device.create_buffer_init(&BufferInitDescriptor {
            label: Some("uniform_buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        });

        let uniform_bind_group_layout =
            render_device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("uniform_bind_group_layout"),
            });

        let uniform_bind_group = render_device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: Some("uniform_bind_group"),
        });

        (
            uniforms,
            uniform_buffer,
            uniform_bind_group_layout,
            uniform_bind_group,
        )
    }

    fn create_light(
        render_device: &wgpu::Device,
    ) -> (Light, wgpu::Buffer, wgpu::BindGroupLayout, wgpu::BindGroup) {
        let light = Light::new(
            Vector3::new(256.0, 500.0, 200.0),
            Vector3::new(1.0, 1.0, 1.0),
        );

        let light_buffer = render_device.create_buffer_init(&BufferInitDescriptor {
            label: Some("light_buffer"),
            contents: bytemuck::cast_slice(&[light]),
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        });

        let light_bind_group_layout =
            render_device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("light_bind_group_layout"),
            });

        let light_bind_group = render_device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &light_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light_buffer.as_entire_binding(),
            }],
            label: Some("light_bind_group"),
        });

        (
            light,
            light_buffer,
            light_bind_group_layout,
            light_bind_group,
        )
    }

    fn create_render_pipeline(
        render_device: &wgpu::Device,
        swap_chain_descriptor: &wgpu::SwapChainDescriptor,
        bind_group_layouts: &[&wgpu::BindGroupLayout],
    ) -> wgpu::RenderPipeline {
        let shader = render_device.create_shader_module(
            &(wgpu::ShaderModuleDescriptor {
                label: Some("shader"),
                flags: wgpu::ShaderFlags::all(),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/world.wgsl").into()),
            }),
        );

        let render_pipeline_layout =
            render_device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render_pipeline_layout"),
                bind_group_layouts,
                push_constant_ranges: &[],
            });

        let wireframe = false;

        render_device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "main",
                buffers: &[Vertex::desc(), Instance::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "main",
                targets: &[wgpu::ColorTargetState {
                    format: swap_chain_descriptor.format,
                    blend: Some(wgpu::BlendState {
                        alpha: wgpu::BlendComponent::REPLACE,
                        color: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrite::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: if wireframe {
                    None
                } else {
                    Some(wgpu::Face::Back)
                },
                polygon_mode: if wireframe {
                    wgpu::PolygonMode::Line
                } else {
                    wgpu::PolygonMode::Fill
                },
                clamp_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        })
    }

    fn create_instances(
        render_device: &wgpu::Device,
        world: &World,
    ) -> (
        Vec<(BlockType, Vector3<i32>, Vec<Instance>)>,
        AHashMap<(BlockType, Vector3<i32>), wgpu::Buffer>,
    ) {
        let instance_lists = world.to_instances();

        let instance_buffers = instance_lists
            .iter()
            .map(|(block_type, offset, _)| {
                let buffer = render_device.create_buffer(&BufferDescriptor {
                    label: Some("instance_buffer"),
                    size: (size_of::<Instance>() * 16 * 16 * 16) as BufferAddress,
                    usage: wgpu::BufferUsage::VERTEX | wgpu::BufferUsage::COPY_DST,
                    mapped_at_creation: false,
                });

                ((*block_type, *offset), buffer)
            })
            .collect();

        (instance_lists, instance_buffers)
    }

    pub fn update_chunk(&mut self, render_queue: &wgpu::Queue) {
        let instant = Instant::now();

        self.instance_lists = self.world.to_instances();

        for (block_type, offset, instance_list) in &self.instance_lists {
            if let Some(instance_buffer) = self.instance_buffers.get_mut(&(*block_type, *offset)) {
                render_queue.write_buffer(instance_buffer, 0, bytemuck::cast_slice(&instance_list));
            } else {
                todo!();
            }
        }

        let elapsed = instant.elapsed();
        println!("Chunk update took {:?}", elapsed);
    }

    pub fn new(
        render_device: &wgpu::Device,
        render_queue: &wgpu::Queue,
        swap_chain_descriptor: &wgpu::SwapChainDescriptor,
    ) -> WorldState {
        // let chunk = Chunk::generate(0, 0, 0);
        let world = World::generate();

        let (world_texture_layout, texture_bind_groups) =
            Self::create_textures(&render_device, &render_queue);

        let (camera, projection) = Self::create_camera(&swap_chain_descriptor);

        let (uniforms, uniform_buffer, world_uniform_layout, uniform_bind_group) =
            Self::create_uniforms(&camera, &projection, &render_device);

        let (_, _, world_light_layout, light_bind_group) = Self::create_light(&render_device);

        let render_pipeline = Self::create_render_pipeline(
            &render_device,
            &swap_chain_descriptor,
            &[
                &world_texture_layout,
                &world_uniform_layout,
                &world_light_layout,
            ],
        );

        let vertex_buffer = render_device.create_buffer_init(&BufferInitDescriptor {
            label: Some("vertex_buffer"),
            contents: bytemuck::cast_slice(cube::VERTICES),
            usage: wgpu::BufferUsage::VERTEX,
        });
        let grass_vertex_buffer = render_device.create_buffer_init(&BufferInitDescriptor {
            label: Some("grass vertex buffer"),
            contents: bytemuck::cast_slice(cube::VERTICES_GRASS),
            usage: wgpu::BufferUsage::VERTEX,
        });

        let index_buffer = render_device.create_buffer_init(&BufferInitDescriptor {
            label: Some("index_buffer"),
            contents: bytemuck::cast_slice(cube::INDICES),
            usage: wgpu::BufferUsage::INDEX,
        });

        let (instance_lists, instance_buffers) = Self::create_instances(&render_device, &world);

        let depth_texture =
            Texture::create_depth_texture(&render_device, &swap_chain_descriptor, "depth_texture");

        let mut world_state = Self {
            render_pipeline,
            uniforms,
            uniform_buffer,
            uniform_bind_group,
            texture_bind_groups,
            camera,
            projection,
            instance_lists,
            vertex_buffer,
            vertex_buffer_grass: grass_vertex_buffer,
            index_buffer,
            instance_buffers,
            depth_texture,
            light_bind_group,
            world,
        };

        world_state.update_chunk(&render_queue);

        world_state
    }

    pub fn resize(
        &mut self,
        render_device: &wgpu::Device,
        swap_chain_descriptor: &wgpu::SwapChainDescriptor,
        new_size: PhysicalSize<u32>,
    ) {
        self.projection.resize(new_size.width, new_size.height);
        self.depth_texture =
            Texture::create_depth_texture(render_device, swap_chain_descriptor, "depth_texture");
    }
}
