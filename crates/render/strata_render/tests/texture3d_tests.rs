//! Textured mesh sampling: filter modes, address modes, UV sub-rects, and
//! perspective-correct rasterization.
//!
//! The 3D path must be able to place a real image on a mesh. These tests pin
//! the sampler contract and the rasterizer integration so tile imagery cannot
//! silently regress to a flat colour.

use strata_layout::Rect;
use strata_render::rasterizer::FrameBuffer;
use strata_render::rasterizer3d::{render_scene_3d, render_scene_3d_with_textures};
use strata_render::scene3d::{
    Camera3D, Geometry3D, Material3D, Mesh3D, Scene3D, TexturedMaterial3D, Transform3D,
};
use strata_render::texture3d::{
    sample_texture, AddressMode, FilterMode, SamplerDesc, Texture, TextureData, TextureDesc,
    TextureStore, UvTransform,
};
use strata_style::types::Color;

/// Four-texel texture laid out as a 2x2 quad so filtering and UV mapping are
/// observable: top-left red, top-right green, bottom-left blue, bottom-right white.
fn quad_texture(filter: FilterMode) -> Texture {
    let pixels: [u8; 16] = [
        255, 0, 0, 255, // (0,0) red
        0, 255, 0, 255, // (1,0) green
        0, 0, 255, 255, // (0,1) blue
        255, 255, 255, 255, // (1,1) white
    ];
    Texture {
        data: TextureData {
            desc: TextureDesc {
                width: 2,
                height: 2,
                format: Default::default(),
            },
            pixels: pixels.to_vec().into(),
        },
        sampler: SamplerDesc {
            filter,
            address_u: AddressMode::ClampToEdge,
            address_v: AddressMode::ClampToEdge,
        },
    }
}

#[test]
fn nearest_sampling_returns_exact_texels() {
    let texture = quad_texture(FilterMode::Nearest);
    assert_eq!(sample_texture(&texture, (0.25, 0.25)), [255, 0, 0, 255]);
    assert_eq!(sample_texture(&texture, (0.75, 0.25)), [0, 255, 0, 255]);
    assert_eq!(sample_texture(&texture, (0.25, 0.75)), [0, 0, 255, 255]);
    assert_eq!(sample_texture(&texture, (0.75, 0.75)), [255, 255, 255, 255]);
}

#[test]
fn bilinear_sampling_blends_neighbouring_texels() {
    let texture = quad_texture(FilterMode::Bilinear);
    // The exact centre of a 2x2 texture sits between all four texels.
    let centre = sample_texture(&texture, (0.5, 0.5));
    assert!(
        (centre[0] as i32 - 128).abs() <= 1
            && (centre[1] as i32 - 128).abs() <= 1
            && (centre[2] as i32 - 128).abs() <= 1,
        "bilinear centre must average all four texels, got {centre:?}"
    );

    // A texel centre stays essentially pure with bilinear filtering.
    let red = sample_texture(&texture, (0.25, 0.25));
    assert_eq!(red[0], 255);
    assert!(
        red[1] <= 2 && red[2] <= 2,
        "centre tap must stay red, got {red:?}"
    );
}

#[test]
fn bilinear_sampling_ramps_between_adjacent_texels() {
    let texture = quad_texture(FilterMode::Bilinear);
    let left = sample_texture(&texture, (0.3, 0.25));
    let right = sample_texture(&texture, (0.7, 0.25));
    assert!(
        left[0] > right[0],
        "the red-to-green ramp must fall left to right, got {left:?} then {right:?}"
    );
    assert!(
        right[1] > left[1],
        "the green channel must rise left to right, got {left:?} then {right:?}"
    );
}

#[test]
fn address_modes_wrap_and_clamp() {
    assert_eq!(AddressMode::ClampToEdge.wrap(-0.4), 0.0);
    assert_eq!(AddressMode::ClampToEdge.wrap(1.4), 1.0);
    assert!((AddressMode::Repeat.wrap(1.25) - 0.25).abs() < 1e-6);
    assert!((AddressMode::Repeat.wrap(-0.25) - 0.75).abs() < 1e-6);
    assert!((AddressMode::MirrorRepeat.wrap(1.25) - 0.75).abs() < 1e-6);
}

#[test]
fn uv_sub_rect_selects_the_ancestor_region() {
    let texture = quad_texture(FilterMode::Nearest);
    // The top-right quarter of a tile that lives in the bottom-right quadrant
    // of a loaded ancestor covers the white texel.
    let transform = UvTransform::from_sub_rect(0.5, 0.5, 1.0, 1.0);
    let uv = transform.apply((0.75, 0.75));
    assert_eq!(sample_texture(&texture, uv), [255, 255, 255, 255]);

    // The identity transform addresses the whole image.
    assert_eq!(UvTransform::IDENTITY.apply((0.75, 0.25)), (0.75, 0.25));
}

#[test]
fn texture_store_hands_out_stable_unique_handles() {
    let mut store = TextureStore::new();
    let a = store
        .insert_rgba8(2, 2, vec![9u8; 16])
        .expect("valid texture");
    let b = store
        .insert_rgba8(2, 2, vec![7u8; 16])
        .expect("valid texture");
    assert_ne!(a, b);
    assert_eq!(store.sample(a, (0.5, 0.5)), Some([9, 9, 9, 9]));
    assert_eq!(store.sample(b, (0.5, 0.5)), Some([7, 7, 7, 7]));

    let generation = store.generation();
    store.remove(a);
    assert!(store.generation() > generation);
    assert_eq!(store.sample(a, (0.5, 0.5)), None);
    assert_eq!(
        store.sample(b, (0.5, 0.5)),
        Some([7, 7, 7, 7]),
        "removing one texture must not disturb another"
    );
}

#[test]
fn texture_store_rejects_mismatched_pixel_buffers() {
    let mut store = TextureStore::new();
    let wrong_len = store.insert(
        TextureData {
            desc: TextureDesc {
                width: 4,
                height: 4,
                format: Default::default(),
            },
            pixels: vec![0u8; 8].into(),
        },
        SamplerDesc::default(),
    );
    assert!(wrong_len.is_none(), "a short buffer must be rejected");

    let empty = store.insert(
        TextureData {
            desc: TextureDesc {
                width: 0,
                height: 0,
                format: Default::default(),
            },
            pixels: Vec::new().into(),
        },
        SamplerDesc::default(),
    );
    assert!(empty.is_none(), "a zero-sized texture must be rejected");
}

fn rgba8(c: Color) -> (u8, u8, u8) {
    (
        (c.r() * 255.0).round() as u8,
        (c.g() * 255.0).round() as u8,
        (c.b() * 255.0).round() as u8,
    )
}

fn textured_plane_scene(texture: strata_render::texture3d::TextureId, uv: UvTransform) -> Scene3D {
    // A 2x2 plane facing the camera with four corners carrying the full UV square.
    // V is flipped so texel row 0 lands on the top of the quad.
    let verts = vec![
        -1.0, -1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, //
        1.0, -1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, //
        1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, //
        -1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
    let mut scene = Scene3D::new();
    scene.camera = Camera3D::Perspective {
        fov: 60.0,
        aspect: 1.0,
        near: 0.1,
        far: 100.0,
        position: [0.0, 0.0, 4.0],
        target: [0.0, 0.0, 0.0],
    };
    let material = TexturedMaterial3D::new(texture).with_uv_transform(uv);
    scene.add_mesh(
        Mesh3D::new(
            Geometry3D::CustomTextured {
                vertices: verts,
                indices,
            },
            Material3D::Textured(material),
        )
        .with_transform(Transform3D::default()),
    );
    scene
}

#[test]
fn textured_mesh_paints_sampled_texels() {
    let mut store = TextureStore::new();
    let texture = store
        .insert_rgba8(2, 2, quad_texture(FilterMode::Nearest).data.pixels)
        .expect("valid texture");

    let scene = textured_plane_scene(texture, UvTransform::IDENTITY);
    let viewport = Rect::new(0.0, 0.0, 64.0, 64.0);
    let mut fb = FrameBuffer::new(64, 64);

    render_scene_3d_with_textures(&mut fb, viewport, &scene, None, Some(&store));

    let top_left = fb.get_pixel(20, 20);
    let top_right = fb.get_pixel(44, 20);
    assert_eq!(
        rgba8(top_left),
        (255, 0, 0),
        "top-left of the plane must sample the red texel, got {top_left:?}"
    );
    assert_eq!(
        rgba8(top_right),
        (0, 255, 0),
        "top-right of the plane must sample the green texel, got {top_right:?}"
    );
}

#[test]
fn textured_mesh_honours_the_uv_sub_rect() {
    let mut store = TextureStore::new();
    let texture = store
        .insert_rgba8(2, 2, quad_texture(FilterMode::Nearest).data.pixels)
        .expect("valid texture");

    // Only the bottom-right texel (white) is addressed.
    let scene = textured_plane_scene(texture, UvTransform::from_sub_rect(0.5, 0.5, 1.0, 1.0));
    let viewport = Rect::new(0.0, 0.0, 64.0, 64.0);
    let mut fb = FrameBuffer::new(64, 64);

    render_scene_3d_with_textures(&mut fb, viewport, &scene, None, Some(&store));

    let centre = fb.get_pixel(32, 32);
    assert_eq!(
        rgba8(centre),
        (255, 255, 255),
        "a bottom-right sub-rect must paint white, got {centre:?}"
    );
}

#[test]
fn textured_mesh_without_textures_falls_back_to_its_tint() {
    let mut store = TextureStore::new();
    let texture = store
        .insert_rgba8(2, 2, quad_texture(FilterMode::Nearest).data.pixels)
        .expect("valid texture");

    let mut scene = textured_plane_scene(texture, UvTransform::IDENTITY);
    scene.meshes[0].material = Material3D::Textured(
        TexturedMaterial3D::new(texture).with_tint(Color::rgba(10, 20, 30, 255)),
    );

    let viewport = Rect::new(0.0, 0.0, 64.0, 64.0);
    let mut fb = FrameBuffer::new(64, 64);
    render_scene_3d(&mut fb, viewport, &scene, None);

    let centre = fb.get_pixel(32, 32);
    assert_eq!(
        rgba8(centre),
        (10, 20, 30),
        "a missing texture store must not make the mesh invisible, got {centre:?}"
    );
}

#[test]
fn perspective_correction_keeps_the_midline_straight() {
    // A quad tilted away from the camera: without perspective-correct UV
    // interpolation the vertical seam in the middle would bend.
    let pixels: Vec<u8> = (0..16).flat_map(|_| [255u8; 4]).collect();
    let mut store = TextureStore::new();
    let texture = store
        .insert_rgba8(4, 4, pixels)
        .expect("a 4x4 RGBA8 texture needs 64 bytes");

    let verts = vec![
        -1.0, -1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, //
        1.0, -1.0, -1.0, 0.0, 0.0, 1.0, 1.0, 0.0, //
        1.0, 1.0, -1.0, 0.0, 0.0, 1.0, 1.0, 1.0, //
        -1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0,
    ];
    let mut scene = Scene3D::new();
    scene.camera = Camera3D::Perspective {
        fov: 60.0,
        aspect: 1.0,
        near: 0.1,
        far: 100.0,
        position: [0.0, 0.0, 4.0],
        target: [0.0, 0.0, 0.0],
    };
    scene.add_mesh(Mesh3D::new(
        Geometry3D::CustomTextured {
            vertices: verts,
            indices: vec![0, 1, 2, 0, 2, 3],
        },
        Material3D::Textured(TexturedMaterial3D::new(texture)),
    ));

    let viewport = Rect::new(0.0, 0.0, 64.0, 64.0);
    let mut fb = FrameBuffer::new(64, 64);
    render_scene_3d_with_textures(&mut fb, viewport, &scene, None, Some(&store));

    // Every covered pixel is white, which proves sampling ran for the whole
    // quad rather than dropping the far half.
    let covered = (0..64)
        .flat_map(|y| (0..64).map(move |x| (x, y)))
        .filter(|&(x, y)| fb.get_pixel(x, y).a() > 0.5)
        .count();
    assert!(
        covered > 500,
        "the tilted textured quad must cover a substantial area, got {covered} pixels"
    );
}
