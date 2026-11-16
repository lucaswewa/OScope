//! A height map as a mesh (P42): a vertex at every `stride`-th pixel's
//! centre, two triangles to each square of four, written as binary PLY
//! (Turk, 1994), binary STL (3D Systems, 1989), or OBJ (Wavefront) with a
//! material naming its texture.
//!
//! In µm and right-handed: x to the right and y up the image, from its
//! bottom-left pixel, and z the height, so the surface is seen from above as
//! the image shows it, its triangles wound counter-clockwise, facing up.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use oscope_compute::Image;
use oscope_imaging::color::encode_png;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{VolumeError, refused};

/// A mesh's file format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MeshFormat {
    /// Binary PLY, with each vertex's colour.
    Ply,
    /// Binary STL: triangles alone.
    Stl,
    /// OBJ, with texture coordinates, its material, and the texture as PNG.
    Obj,
}

impl MeshFormat {
    /// Its file's extension.
    pub fn extension(self) -> &'static str {
        match self {
            MeshFormat::Ply => "ply",
            MeshFormat::Stl => "stl",
            MeshFormat::Obj => "obj",
        }
    }
}

fn one() -> u32 {
    1
}

/// How a height map becomes a mesh.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeshOptions {
    /// A pixel's side, in µm.
    pub pixel_um: f64,
    /// A vertex at every this many pixels, along each axis: 1 or more.
    #[serde(default = "one")]
    pub stride: u32,
    /// The confidence a square's four corners need for it to be kept.
    #[serde(default)]
    pub min_confidence: f32,
}

/// A mesh: its vertices, their colours and their places in the texture,
/// from 0 to 1 with v up, and its triangles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    /// Each vertex's x, y and z, in µm.
    pub vertices: Vec<[f32; 3]>,
    /// Each vertex's colour, from the texture, or grey.
    pub colours: Vec<[u8; 3]>,
    /// Each vertex's place in the texture.
    pub uv: Vec<[f32; 2]>,
    /// The triangles, by their vertices, counter-clockwise from above.
    pub triangles: Vec<[u32; 3]>,
}

/// A byte of `sample`, from 0 to 1.
fn byte(sample: f32) -> u8 {
    (sample.clamp(0.0, 1.0) * 255.0).round() as u8
}

impl Mesh {
    /// The mesh of `heights`, in µm. Where `confidence` is given, a square
    /// any of whose corners is under the options' floor is left out; where
    /// `texture` is, of the heights' size, grey or colour, it colours the
    /// vertices.
    pub fn new(
        heights: &Image,
        confidence: Option<&Image>,
        texture: Option<&Image>,
        options: &MeshOptions,
    ) -> Result<Self, VolumeError> {
        let (w, h) = (heights.width() as usize, heights.height() as usize);
        let s = options.stride as usize;
        let size = |image: &Image| (image.width() as usize, image.height() as usize);
        let fits = |image: Option<&Image>| image.is_none_or(|i| size(i) == (w, h));
        let sized = options.pixel_um > 0.0 && options.pixel_um.is_finite();
        if s == 0 || !sized || heights.channels() != 1 || w == 0 || h == 0 {
            return Err(refused(format!(
                "a mesh of grey heights {w} × {h}, a stride of {s} and pixels of {} µm: none empty, the stride 1 or more, the pixels above 0",
                options.pixel_um
            )));
        }
        if !fits(confidence) || !fits(texture) {
            return Err(refused(
                "the confidence and the texture must be the heights' size",
            ));
        }
        let (columns, rows) = ((w - 1) / s + 1, (h - 1) / s + 1);
        let p = options.pixel_um as f32;
        let mut mesh = Mesh::default();
        for row in 0..rows {
            for column in 0..columns {
                let (x, y) = (column * s, row * s);
                let i = y * w + x;
                mesh.vertices
                    .push([x as f32 * p, (h - 1 - y) as f32 * p, heights.data()[i]]);
                mesh.uv.push([
                    (x as f32 + 0.5) / w as f32,
                    1.0 - (y as f32 + 0.5) / h as f32,
                ]);
                mesh.colours.push(match texture {
                    Some(t) => {
                        let c = t.channels();
                        let pixel = &t.data()[i * c..(i + 1) * c];
                        [0, 1, 2].map(|k| byte(pixel[k.min(c - 1)]))
                    }
                    None => [200; 3],
                });
            }
        }
        let kept = |x: usize, y: usize| {
            confidence.is_none_or(|c| c.data()[y * s * w + x * s] >= options.min_confidence)
        };
        for row in 0..rows.saturating_sub(1) {
            for column in 0..columns.saturating_sub(1) {
                let corners = [
                    (column, row),
                    (column + 1, row),
                    (column, row + 1),
                    (column + 1, row + 1),
                ];
                if !corners.iter().all(|&(x, y)| kept(x, y)) {
                    continue;
                }
                let vertex = |x: usize, y: usize| (y * columns + x) as u32;
                let (top_left, top_right) = (vertex(column, row), vertex(column + 1, row));
                let (bottom_left, bottom_right) =
                    (vertex(column, row + 1), vertex(column + 1, row + 1));
                mesh.triangles.push([bottom_left, bottom_right, top_right]);
                mesh.triangles.push([bottom_left, top_right, top_left]);
            }
        }
        Ok(mesh)
    }

    /// A triangle's unit normal, or none for a degenerate one.
    pub fn normal(&self, triangle: [u32; 3]) -> [f32; 3] {
        let [a, b, c] = triangle.map(|v| self.vertices[v as usize]);
        let (u, v) = (
            [0, 1, 2].map(|k| b[k] - a[k]),
            [0, 1, 2].map(|k| c[k] - a[k]),
        );
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let length = n.iter().map(|x| x * x).sum::<f32>().sqrt();
        match length > 0.0 {
            true => n.map(|x| x / length),
            false => [0.0; 3],
        }
    }
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> VolumeError + '_ {
    move |source| VolumeError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Writes `mesh` in `folder` as `<name>.<extension>` in `format`: for OBJ,
/// with `<name>.mtl` and the `texture` as `<name>.png`, if given. A file
/// already there is refused, never written over. The files written.
pub fn write_mesh(
    mesh: &Mesh,
    format: MeshFormat,
    texture: Option<&Image>,
    folder: &Path,
    name: &str,
) -> Result<Vec<PathBuf>, VolumeError> {
    let path = folder.join(format!("{name}.{}", format.extension()));
    let (mtl, png) = (
        folder.join(format!("{name}.mtl")),
        folder.join(format!("{name}.png")),
    );
    let mut files = vec![path.clone()];
    if format == MeshFormat::Obj && texture.is_some() {
        files.extend([mtl.clone(), png.clone()]);
    }
    if let Some(there) = files.iter().find(|f| f.exists()) {
        return Err(refused(format!("{} is there already", there.display())));
    }
    std::fs::create_dir_all(folder).map_err(io(folder))?;
    let file = File::create(&path).map_err(io(&path))?;
    let mut out = BufWriter::new(file);
    match format {
        MeshFormat::Ply => ply(mesh, &mut out),
        MeshFormat::Stl => stl(mesh, &mut out),
        MeshFormat::Obj => obj(mesh, texture.map(|_| name), &mut out),
    }
    .and_then(|()| out.flush())
    .map_err(io(&path))?;
    if let (MeshFormat::Obj, Some(texture)) = (format, texture) {
        let material = format!("newmtl surface\nKa 1 1 1\nKd 1 1 1\nmap_Kd {name}.png\n");
        std::fs::write(&mtl, material).map_err(io(&mtl))?;
        let c = texture.channels();
        let rgb: Vec<u8> = texture
            .data()
            .chunks(c)
            .flat_map(|p| [0, 1, 2].map(|k| byte(p[k.min(c - 1)])))
            .collect();
        let mut encoded = Vec::new();
        encode_png(&mut encoded, &rgb, texture.width(), texture.height())
            .map_err(|e| refused(format!("the texture: {e}")))?;
        std::fs::write(&png, encoded).map_err(io(&png))?;
    }
    Ok(files)
}

/// Binary PLY: each vertex's position and colour, each face its three.
fn ply(mesh: &Mesh, out: &mut impl Write) -> std::io::Result<()> {
    write!(
        out,
        "ply\nformat binary_little_endian 1.0\ncomment OScope depth from focus, in micrometres\n\
         element vertex {}\nproperty float x\nproperty float y\nproperty float z\n\
         property uchar red\nproperty uchar green\nproperty uchar blue\n\
         element face {}\nproperty list uchar int vertex_indices\nend_header\n",
        mesh.vertices.len(),
        mesh.triangles.len()
    )?;
    for (vertex, colour) in mesh.vertices.iter().zip(&mesh.colours) {
        for v in vertex {
            out.write_all(&v.to_le_bytes())?;
        }
        out.write_all(colour)?;
    }
    for triangle in &mesh.triangles {
        out.write_all(&[3])?;
        for v in triangle {
            out.write_all(&(*v as i32).to_le_bytes())?;
        }
    }
    Ok(())
}

/// Binary STL: a header, the count, then each triangle's normal and
/// corners.
fn stl(mesh: &Mesh, out: &mut impl Write) -> std::io::Result<()> {
    let mut header = [b' '; 80];
    let title = b"OScope depth from focus, in micrometres";
    header[..title.len()].copy_from_slice(title);
    out.write_all(&header)?;
    out.write_all(&(mesh.triangles.len() as u32).to_le_bytes())?;
    for &triangle in &mesh.triangles {
        let corners = triangle.map(|v| mesh.vertices[v as usize]);
        for v in std::iter::once(mesh.normal(triangle))
            .chain(corners)
            .flatten()
        {
            out.write_all(&v.to_le_bytes())?;
        }
        out.write_all(&0_u16.to_le_bytes())?;
    }
    Ok(())
}

/// OBJ: vertices, their texture's places, and faces numbered from 1; with
/// its material if `material` names it.
fn obj(mesh: &Mesh, material: Option<&str>, out: &mut impl Write) -> std::io::Result<()> {
    writeln!(out, "# OScope depth from focus, in micrometres")?;
    if let Some(name) = material {
        writeln!(out, "mtllib {name}.mtl\nusemtl surface")?;
    }
    for [x, y, z] in &mesh.vertices {
        writeln!(out, "v {x} {y} {z}")?;
    }
    for [u, v] in &mesh.uv {
        writeln!(out, "vt {u} {v}")?;
    }
    for triangle in &mesh.triangles {
        let [a, b, c] = triangle.map(|v| v + 1);
        writeln!(out, "f {a}/{a} {b}/{b} {c}/{c}")?;
    }
    Ok(())
}
