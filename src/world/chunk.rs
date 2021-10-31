use std::collections::VecDeque;

use crate::{
    aabb::Aabb,
    geometry::Geometry,
    geometry_buffers::GeometryBuffers,
    render_context::RenderContext,
    vertex::BlockVertex,
    view::View,
    world::{
        block::{Block, BlockType},
        face_flags::*,
        quad::Quad,
    },
};
use cgmath::{Point3, Vector3};
use fxhash::{FxHashMap, FxHashSet};
use noise::utils::{NoiseMapBuilder, PlaneMapBuilder};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::{
    de::{SeqAccess, Visitor},
    ser::SerializeSeq,
    Deserialize, Serialize, Serializer,
};
use wgpu::{BufferUsages, RenderPass};

pub const CHUNK_SIZE: usize = 32;
pub const CHUNK_ISIZE: isize = CHUNK_SIZE as isize;

type CoordinateXZ = (usize, usize);
type BlockFace = (BlockType, FaceFlags);

pub struct Chunk {
    pub blocks: [[[Option<Block>; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE],
    pub buffers: Option<GeometryBuffers<u16>>,
    pub full: bool,
}

impl Default for Chunk {
    fn default() -> Self {
        Self {
            blocks: [[[None; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE],
            buffers: None,
            full: false,
        }
    }
}

struct ChunkVisitor;

impl<'de> Visitor<'de> for ChunkVisitor {
    type Value = Chunk;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a chunk")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut chunk = Chunk::default();
        for layer in chunk.blocks.iter_mut() {
            for row in layer {
                for block in row {
                    *block = seq.next_element()?.unwrap();
                }
            }
        }

        Ok(chunk)
    }
}

impl Serialize for Chunk {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(CHUNK_SIZE.pow(3)))?;
        for layer in self.blocks.iter() {
            for row in layer {
                for block in row {
                    seq.serialize_element(block)?;
                }
            }
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Chunk {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(ChunkVisitor)
    }
}

impl Chunk {
    pub fn render<'a>(
        &'a self,
        render_pass: &mut RenderPass<'a>,
        position: &Point3<isize>,
        view: &View,
    ) -> usize {
        if !self.is_visible(position * CHUNK_ISIZE, view) {
            // Frustrum culling
            0
        } else if let Some(buffers) = &self.buffers {
            buffers.apply_buffers(render_pass);
            buffers.draw_indexed(render_pass)
        } else {
            // Not loaded
            println!("Trying to render non-loaded chunk {:?}", position);
            0
        }
    }

    pub fn update_fullness(&mut self) {
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    if self.blocks[y][z][x].is_none() {
                        self.full = false;
                        return;
                    }
                }
            }
        }

        self.full = true;
    }

    pub fn generate(&mut self, chunk_x: isize, chunk_y: isize, chunk_z: isize) {
        let fbm = noise::Fbm::new();

        const TERRAIN_NOISE_SCALE: f64 = 0.1 / 16.0 * CHUNK_SIZE as f64;
        const TERRAIN_NOISE_OFFSET: f64 = 0.0 / 16.0 * CHUNK_SIZE as f64;
        let terrain_noise = PlaneMapBuilder::new(&fbm)
            .set_size(CHUNK_SIZE, CHUNK_SIZE)
            .set_x_bounds(
                chunk_x as f64 * TERRAIN_NOISE_SCALE + TERRAIN_NOISE_OFFSET,
                chunk_x as f64 * TERRAIN_NOISE_SCALE + TERRAIN_NOISE_SCALE + TERRAIN_NOISE_OFFSET,
            )
            .set_y_bounds(
                chunk_z as f64 * TERRAIN_NOISE_SCALE + TERRAIN_NOISE_OFFSET,
                chunk_z as f64 * TERRAIN_NOISE_SCALE + TERRAIN_NOISE_SCALE + TERRAIN_NOISE_OFFSET,
            )
            .build();

        const STONE_NOISE_SCALE: f64 = 0.07 / 16.0 * CHUNK_SIZE as f64;
        const STONE_NOISE_OFFSET: f64 = 11239.0 / 16.0 * CHUNK_SIZE as f64;
        let stone_noise = PlaneMapBuilder::new(&fbm)
            .set_size(CHUNK_SIZE, CHUNK_SIZE)
            .set_x_bounds(
                chunk_x as f64 * STONE_NOISE_SCALE + STONE_NOISE_OFFSET,
                chunk_x as f64 * STONE_NOISE_SCALE + STONE_NOISE_SCALE + STONE_NOISE_OFFSET,
            )
            .set_y_bounds(
                chunk_z as f64 * STONE_NOISE_SCALE + STONE_NOISE_OFFSET,
                chunk_z as f64 * STONE_NOISE_SCALE + STONE_NOISE_SCALE + STONE_NOISE_OFFSET,
            )
            .build();

        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let v = terrain_noise.get_value(x, z) * 20.0 + 128.0;
                let v = v.round() as isize;

                let s = stone_noise.get_value(x, z) * 20.0 + 4.5;
                let s = (s.round() as isize).min(10).max(3);

                let stone_max = (v - s - chunk_y * CHUNK_ISIZE).min(CHUNK_ISIZE);
                for y in 0..stone_max {
                    self.blocks[y as usize][z][x] = Some(Block {
                        block_type: BlockType::Stone,
                    });
                }

                let dirt_max = (v - chunk_y * CHUNK_ISIZE).min(CHUNK_ISIZE);
                for y in stone_max.max(0)..dirt_max {
                    self.blocks[y as usize][z][x] = Some(Block {
                        block_type: BlockType::Dirt,
                    });
                }

                if (0..CHUNK_ISIZE).contains(&dirt_max) {
                    self.blocks[dirt_max as usize][z][x] = Some(Block {
                        block_type: BlockType::Grass,
                    });
                }

                if chunk_y == 0 {
                    self.blocks[0][z][x] = Some(Block {
                        block_type: BlockType::Bedrock,
                    });
                }
                if chunk_y < 128 / CHUNK_ISIZE {
                    for layer in self.blocks.iter_mut() {
                        if layer[z][x].is_none() {
                            layer[z][x] = Some(Block {
                                block_type: BlockType::Water,
                            });
                        }
                    }
                }
            }
        }
    }

    pub fn block_coords_to_local(
        chunk_coords: Point3<isize>,
        block_coords: Point3<isize>,
    ) -> Option<Vector3<usize>> {
        let chunk_position = chunk_coords * CHUNK_ISIZE;
        let position = block_coords - chunk_position;
        if (0..CHUNK_ISIZE).contains(&position.x)
            && (0..CHUNK_ISIZE).contains(&position.y)
            && (0..CHUNK_ISIZE).contains(&position.z)
        {
            Some(position.cast().unwrap())
        } else {
            None
        }
    }

    #[rustfmt::skip]
    fn check_visible_faces(&self, x: usize, y: usize, z: usize) -> FaceFlags {
        let mut visible_faces = FACE_NONE;
        let transparent = self.blocks[y][z][x].unwrap().block_type.is_transparent();

        if x == 0 || self.blocks[y][z][x - 1].is_none()
            || transparent != self.blocks[y][z][x - 1].unwrap().block_type.is_transparent()
        {
            visible_faces |= FACE_LEFT;
        }
        if x == CHUNK_SIZE - 1 || self.blocks[y][z][x + 1].is_none()
            || transparent != self.blocks[y][z][x + 1].unwrap().block_type.is_transparent()
        {
            visible_faces |= FACE_RIGHT;
        }

        if y == 0 || self.blocks[y - 1][z][x].is_none()
            || transparent != self.blocks[y - 1][z][x].unwrap().block_type.is_transparent()
        {
            visible_faces |= FACE_BOTTOM;
        }
        if y == CHUNK_SIZE - 1 || self.blocks[y + 1][z][x].is_none()
            || transparent != self.blocks[y + 1][z][x].unwrap().block_type.is_transparent()
        {
            visible_faces |= FACE_TOP;
        }

        if z == 0 || self.blocks[y][z - 1][x].is_none()
            || transparent != self.blocks[y][z - 1][x].unwrap().block_type.is_transparent()
        {
            visible_faces |= FACE_BACK;
        }
        if z == CHUNK_SIZE - 1 || self.blocks[y][z + 1][x].is_none()
            || transparent != self.blocks[y][z + 1][x].unwrap().block_type.is_transparent()
        {
            visible_faces |= FACE_FRONT;
        }

        visible_faces
    }

    fn cull_layer(&self, y: usize) -> (FxHashMap<CoordinateXZ, BlockFace>, VecDeque<CoordinateXZ>) {
        let mut culled = FxHashMap::default();
        let mut queue = VecDeque::new();

        let y_blocks = &self.blocks[y];
        for (z, z_blocks) in y_blocks.iter().enumerate() {
            for (x, block) in z_blocks.iter().enumerate() {
                if let Some(block) = block {
                    // Don't add the block if it's not visible
                    let visible_faces = self.check_visible_faces(x, y, z);
                    if visible_faces == FACE_NONE {
                        continue;
                    }

                    culled.insert((x, z), (block.block_type, visible_faces));
                    queue.push_back((x, z));
                }
            }
        }

        (culled, queue)
    }

    fn layer_to_quads(
        &self,
        y: usize,
        offset: Point3<isize>,
        culled: FxHashMap<CoordinateXZ, BlockFace>,
        queue: &mut VecDeque<CoordinateXZ>,
        highlighted: Option<(Vector3<usize>, Vector3<i32>)>,
    ) -> Vec<Quad> {
        let mut quads: Vec<Quad> = Vec::new();
        let mut visited = FxHashSet::default();
        let hl = highlighted.map(|h| h.0);
        while let Some((x, z)) = queue.pop_front() {
            let position = offset + Vector3::new(x, y, z).cast().unwrap();

            if visited.contains(&(x, z)) {
                continue;
            }
            visited.insert((x, z));

            if let Some(&(block_type, visible_faces)) = &culled.get(&(x, z)) {
                let mut quad_faces = visible_faces;

                if hl == Some(Vector3::new(x, y, z)) {
                    let mut quad = Quad::new(position, 1, 1);
                    quad.highlighted_normal = highlighted.unwrap().1;
                    quad.visible_faces = quad_faces;
                    quad.block_type = Some(block_type);
                    quads.push(quad);
                    continue;
                }

                if block_type == BlockType::Water {
                    let mut quad = Quad::new(position, 1, 1);
                    quad.visible_faces = quad_faces;
                    quad.block_type = Some(block_type);
                    quads.push(quad);
                    continue;
                }

                // Extend along the X axis
                let mut xmax = x + 1;
                for x_ in x..CHUNK_SIZE {
                    xmax = x_ + 1;

                    if visited.contains(&(xmax, z)) || hl == Some(Vector3::new(xmax, y, z)) {
                        break;
                    }

                    if let Some(&(block_type_, visible_faces_)) = culled.get(&(xmax, z)) {
                        quad_faces |= visible_faces_;
                        if block_type != block_type_ {
                            break;
                        }
                    } else {
                        break;
                    }

                    visited.insert((xmax, z));
                }

                // Extend along the Z axis
                let mut zmax = z + 1;
                'z: for z_ in z..CHUNK_SIZE {
                    zmax = z_ + 1;

                    for x_ in x..xmax {
                        if visited.contains(&(x_, zmax)) || hl == Some(Vector3::new(x_, y, zmax)) {
                            break 'z;
                        }

                        if let Some(&(block_type_, visible_faces_)) = culled.get(&(x_, zmax)) {
                            quad_faces |= visible_faces_;
                            if block_type != block_type_ {
                                break 'z;
                            }
                        } else {
                            break 'z;
                        }
                    }

                    for x_ in x..xmax {
                        visited.insert((x_, zmax));
                    }
                }

                let mut quad = Quad::new(position, (xmax - x) as isize, (zmax - z) as isize);
                quad.visible_faces = quad_faces;
                quad.block_type = Some(block_type);
                quads.push(quad);
            }
        }

        quads
    }

    fn quads_to_geometry(quads: Vec<Quad>) -> Geometry<BlockVertex, u16> {
        let mut geometry: Geometry<BlockVertex, u16> = Default::default();
        for quad in quads {
            geometry.append(&mut quad.to_geometry(geometry.vertices.len() as u16));
        }
        geometry
    }

    pub fn update_geometry(
        &mut self,
        render_context: &RenderContext,
        chunk_coords: Point3<isize>,
        highlighted: Option<(Point3<isize>, Vector3<i32>)>,
    ) {
        let highlighted = highlighted.and_then(|(position, normal)| {
            Self::block_coords_to_local(chunk_coords, position).map(|x| (x, normal))
        });

        let offset = chunk_coords * CHUNK_ISIZE;
        let quads: Vec<Quad> = (0..CHUNK_SIZE)
            .into_par_iter()
            .flat_map(|y| {
                let (culled, mut queue) = self.cull_layer(y);
                self.layer_to_quads(y, offset, culled, &mut queue, highlighted)
            })
            .collect();

        self.buffers = Some(GeometryBuffers::from_geometry(
            render_context,
            &Self::quads_to_geometry(quads),
            BufferUsages::empty(),
        ));

        self.update_fullness();
    }

    pub fn save(&self, position: Point3<isize>, store: &sled::Db) -> anyhow::Result<()> {
        let data = rmp_serde::encode::to_vec_named(self)?;
        let key = format!("{}_{}_{}", position.x, position.y, position.z);
        store.insert(key, data)?;
        Ok(())
    }

    pub fn load(&mut self, position: Point3<isize>, store: &sled::Db) -> anyhow::Result<bool> {
        let key = format!("{}_{}_{}", position.x, position.y, position.z);

        if let Some(data) = store.get(key)? {
            *self = rmp_serde::decode::from_slice(&data)?;
            Ok(false)
        } else {
            self.generate(position.x, position.y, position.z);
            Ok(true)
        }
    }

    pub fn is_visible(&self, position: Point3<isize>, view: &View) -> bool {
        let aabb = Aabb {
            min: position.cast().unwrap(),
            max: (position + Vector3::new(CHUNK_ISIZE, CHUNK_ISIZE, CHUNK_ISIZE))
                .cast()
                .unwrap(),
        };

        aabb.intersects(&view.frustrum_aabb)
    }
}
