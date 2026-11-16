//! Meshes of a height map (P42), read back in each format: their vertices,
//! faces and normals; squares under the confidence floor left out; strides;
//! and files never written over.

use oscope_compute::{Image, Shape};
use oscope_volume::{Mesh, MeshFormat, MeshOptions, write_mesh};

/// Heights of 3 × 2 pixels of 2 µm: 0, 1, 2 along the top row, 3, 4, 5
/// along the bottom, so the surface rises to the right and down the image.
fn heights() -> Image {
    Image::from_vec(Shape::new(3, 2, 1), vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]).unwrap()
}

fn options() -> MeshOptions {
    MeshOptions {
        pixel_um: 2.0,
        stride: 1,
        min_confidence: 0.0,
    }
}

fn f32_at(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

#[test]
fn a_height_map_s_mesh_is_its_pixels_two_triangles_a_square_facing_up() {
    let texture = Image::from_vec(
        Shape::new(3, 2, 3),
        (0..18).map(|v| v as f32 / 17.0).collect(),
    )
    .unwrap();
    let mesh = Mesh::new(&heights(), None, Some(&texture), &options()).unwrap();
    // y up from the bottom row: the top row at 2 µm.
    assert_eq!(mesh.vertices[0], [0.0, 2.0, 0.0]);
    assert_eq!(mesh.vertices[5], [4.0, 0.0, 5.0]);
    assert_eq!(mesh.colours[5], [225, 240, 255]);
    assert_eq!(mesh.uv[0], [0.5 / 3.0, 1.0 - 0.25]);
    assert_eq!(mesh.triangles.len(), 4);
    // The plane z = x / 2 + 3 − 1.5 y: its normal, (−0.5, 1.5, 1) unit,
    // faces up.
    let length = (0.25_f32 + 2.25 + 1.0).sqrt();
    for &triangle in &mesh.triangles {
        let n = mesh.normal(triangle);
        let want = [-0.5 / length, 1.5 / length, 1.0 / length];
        assert!(
            n.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6),
            "{n:?}"
        );
    }
    // A corner under the floor leaves out both squares about it; a stride
    // of 2 keeps every other pixel.
    let mut sure = vec![1.0; 6];
    sure[1] = 0.2;
    let confidence = Image::from_vec(Shape::new(3, 2, 1), sure).unwrap();
    let floor = MeshOptions {
        min_confidence: 0.5,
        ..options()
    };
    assert!(
        Mesh::new(&heights(), Some(&confidence), None, &floor)
            .unwrap()
            .triangles
            .is_empty()
    );
    sure = vec![1.0; 6];
    sure[0] = 0.2;
    let confidence = Image::from_vec(Shape::new(3, 2, 1), sure).unwrap();
    assert_eq!(
        Mesh::new(&heights(), Some(&confidence), None, &floor)
            .unwrap()
            .triangles
            .len(),
        2
    );
    let wide = Image::zeros(Shape::new(5, 3, 1));
    let strided = MeshOptions {
        stride: 2,
        ..options()
    };
    let mesh = Mesh::new(&wide, None, None, &strided).unwrap();
    assert_eq!((mesh.vertices.len(), mesh.triangles.len()), (6, 4));
    assert_eq!(mesh.vertices[2], [8.0, 4.0, 0.0]);
    // Refused: no stride, pixels of no size, colour heights, a texture of
    // another size.
    for bad in [
        MeshOptions {
            stride: 0,
            ..options()
        },
        MeshOptions {
            pixel_um: 0.0,
            ..options()
        },
    ] {
        assert!(Mesh::new(&heights(), None, None, &bad).is_err());
    }
    assert!(Mesh::new(&Image::zeros(Shape::new(3, 2, 3)), None, None, &options()).is_err());
    assert!(Mesh::new(&heights(), None, Some(&wide), &options()).is_err());
}

#[test]
fn each_format_reads_back_and_none_is_written_over() {
    let folder = tempfile::tempdir().unwrap();
    let texture = Image::from_vec(Shape::new(3, 2, 1), vec![0.5; 6]).unwrap();
    let mesh = Mesh::new(&heights(), None, Some(&texture), &options()).unwrap();

    // PLY: its header, then each vertex's 3 floats and 3 bytes, each face a
    // count and 3 ints.
    let files = write_mesh(&mesh, MeshFormat::Ply, None, folder.path(), "surface").unwrap();
    let ply = std::fs::read(&files[0]).unwrap();
    let end = ply.windows(11).position(|w| w == b"end_header\n").unwrap() + 11;
    let header = String::from_utf8(ply[..end].to_vec()).unwrap();
    assert!(
        header.contains("element vertex 6\n") && header.contains("element face 4\n"),
        "{header}"
    );
    assert!(header.starts_with("ply\nformat binary_little_endian 1.0\n"));
    let body = &ply[end..];
    assert_eq!(body.len(), 6 * 15 + 4 * 13);
    assert_eq!(
        [f32_at(body, 15), f32_at(body, 19), f32_at(body, 23)],
        [2.0, 2.0, 1.0]
    );
    assert_eq!(&body[27..30], &[128, 128, 128]);
    let face = &body[6 * 15..6 * 15 + 13];
    assert_eq!(face[0], 3);
    let indices: Vec<i32> = (0..3)
        .map(|k| i32::from_le_bytes(face[1 + 4 * k..5 + 4 * k].try_into().unwrap()))
        .collect();
    assert_eq!(indices, [3, 4, 1]);

    // STL: 80 bytes, the count, then 50 bytes a triangle, normals up.
    let files = write_mesh(&mesh, MeshFormat::Stl, None, folder.path(), "surface").unwrap();
    let stl = std::fs::read(&files[0]).unwrap();
    assert_eq!(stl.len(), 84 + 4 * 50);
    assert_eq!(u32::from_le_bytes(stl[80..84].try_into().unwrap()), 4);
    assert!(f32_at(&stl, 84 + 8) > 0.0);
    assert_eq!(
        [f32_at(&stl, 96), f32_at(&stl, 100), f32_at(&stl, 104)],
        [0.0, 0.0, 3.0]
    );

    // OBJ: its material, the texture beside it, faces numbered from 1.
    let files = write_mesh(
        &mesh,
        MeshFormat::Obj,
        Some(&texture),
        folder.path(),
        "surface",
    )
    .unwrap();
    let names: Vec<String> = files
        .iter()
        .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["surface.obj", "surface.mtl", "surface.png"]);
    let obj = std::fs::read_to_string(&files[0]).unwrap();
    let count = |prefix: &str| obj.lines().filter(|l| l.starts_with(prefix)).count();
    assert_eq!((count("v "), count("vt "), count("f ")), (6, 6, 4));
    assert!(obj.contains("mtllib surface.mtl\nusemtl surface\n"));
    assert!(obj.lines().any(|l| l == "f 4/4 5/5 2/2"), "{obj}");
    assert!(
        std::fs::read_to_string(&files[1])
            .unwrap()
            .contains("map_Kd surface.png")
    );
    assert_eq!(&std::fs::read(&files[2]).unwrap()[1..4], b"PNG");

    // Never written over.
    for format in [MeshFormat::Ply, MeshFormat::Stl, MeshFormat::Obj] {
        assert!(write_mesh(&mesh, format, None, folder.path(), "surface").is_err());
    }
    let ply_again = std::fs::read(folder.path().join("surface.ply")).unwrap();
    assert_eq!(ply_again, ply);
}
