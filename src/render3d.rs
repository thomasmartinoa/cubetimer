//! A small software renderer for the 3D cube view.
//!
//! It draws the stickers of an `Nxn` cube in perspective into an RGBA buffer,
//! with smooth (supersampled) edges and a little shading so the three visible
//! faces read as a solid. A layer can be drawn part-way through a turn, which
//! is how move animations work.
//!
//! Every sticker sits on a black plate (the cube's plastic). Hidden surfaces
//! are removed with a depth buffer, so the order quads are drawn in doesn't
//! matter. While a layer turns, the two faces of the cut between it and the
//! rest are drawn as black squares, so the cube never looks hollow.

use crate::nxn::{Move, Nxn};

/// WCA colors, U R F D L B (the same as the flat preview).
pub const STICKER_RGB: [[u8; 3]; 6] = [
    [245, 245, 245],
    [214, 40, 40],
    [0, 168, 84],
    [255, 213, 0],
    [255, 128, 0],
    [0, 82, 204],
];
const PLASTIC: [u8; 3] = [18, 18, 18];

/// Which way the cube is seen. The default shows U, F and R, like a cube held
/// in front of you and turned a little to the left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Turn about the vertical axis, in degrees (negative brings R forward).
    pub yaw: f64,
    /// Tilt, in degrees (positive shows more of U).
    pub pitch: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            yaw: -35.0,
            pitch: 28.0,
        }
    }
}

impl Camera {
    /// Rotate by the given degrees; the tilt stops short of looking straight
    /// down or up, where the view would flip.
    pub fn turned(self, yaw: f64, pitch: f64) -> Camera {
        Camera {
            yaw: (self.yaw + yaw).rem_euclid(360.0),
            pitch: (self.pitch + pitch).clamp(-85.0, 85.0),
        }
    }
}

/// A layer part-way through a turn: `m`'s layers rotated by `fraction` of the
/// move (0.0 = not started, 1.0 = done).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Turning {
    pub m: Move,
    pub fraction: f64,
}

/// An RGBA image, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.width + x) * 4;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }
}

type V3 = [f64; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Rotate `p` by `deg` degrees about `axis` (0 x, 1 y, 2 z), counter-clockwise
/// seen from the positive end of the axis.
fn rotate(p: V3, axis: usize, deg: f64) -> V3 {
    let (s, c) = deg.to_radians().sin_cos();
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let mut out = p;
    out[u] = p[u] * c - p[v] * s;
    out[v] = p[u] * s + p[v] * c;
    out
}

/// World to view: the cube turned by the camera, seen from +z.
fn to_view(p: V3, cam: Camera) -> V3 {
    rotate(rotate(p, 1, cam.yaw), 0, cam.pitch)
}

/// Distance of the eye from the cube's center, in cube half-widths. Far
/// enough that the perspective is gentle.
const EYE: f64 = 7.0;

/// One flat quad to fill, in view space.
/// A flat convex polygon to fill (a sticker, a plate or a cut face).
struct Quad {
    corners: Vec<V3>,
    color: [u8; 3],
    /// Drawn first among quads at the same depth (plates go under stickers).
    bias: f64,
}

/// The quad's four corners for a square of half-width `half` centered at
/// `center` on a face with outward `normal`.
fn square(center: V3, normal: V3, half: f64) -> [V3; 4] {
    // two directions in the face's plane
    let axis = (0..3)
        .max_by(|&a, &b| normal[a].abs().total_cmp(&normal[b].abs()))
        .unwrap();
    let mut u = [0.0; 3];
    u[(axis + 1) % 3] = half;
    let mut v = [0.0; 3];
    v[(axis + 2) % 3] = half;
    [
        add(add(center, u), v),
        add(add(center, scale(u, -1.0)), v),
        add(add(center, scale(u, -1.0)), scale(v, -1.0)),
        add(add(center, u), scale(v, -1.0)),
    ]
}

/// A rounded square like `square`, with corner arcs of radius `round * half`,
/// as a convex polygon (in order around it).
fn rounded_square(center: V3, normal: V3, half: f64, round: f64) -> Vec<V3> {
    let axis = (0..3)
        .max_by(|&a, &b| normal[a].abs().total_cmp(&normal[b].abs()))
        .unwrap();
    let (iu, iv) = ((axis + 1) % 3, (axis + 2) % 3);
    let r = half * round;
    let inner = half - r;
    const ARC: usize = 4; // points per corner
    let mut out = Vec::with_capacity(4 * ARC);
    // corner k sweeps from k to k+1 quarter turns, which is also the quadrant
    // its signs come from: (+,+), (-,+), (-,-), (+,-)
    for (k, (su, sv)) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
        .into_iter()
        .enumerate()
    {
        for i in 0..ARC {
            let a = (k as f64 + i as f64 / (ARC - 1) as f64) * std::f64::consts::FRAC_PI_2;
            let (sin, cos) = a.sin_cos();
            let mut p = center;
            p[iu] += su * inner + r * cos;
            p[iv] += sv * inner + r * sin;
            out.push(p);
        }
    }
    out
}

/// Draw `cube` seen from `camera` into a `width` x `height` image. `turning`
/// rotates one move's layers part of the way.
pub fn render(
    cube: &Nxn,
    camera: Camera,
    turning: Option<Turning>,
    width: usize,
    height: usize,
) -> Image {
    render_with(cube, camera, turning, width, height, 3)
}

/// `render` with `ss` x `ss` samples per pixel: 3 is smooth, 2 is more than
/// twice as fast (for animation), 1 is fastest with jagged edges.
pub fn render_with(
    cube: &Nxn,
    camera: Camera,
    turning: Option<Turning>,
    width: usize,
    height: usize,
    ss: usize,
) -> Image {
    render_opts(
        cube,
        camera,
        turning,
        width,
        height,
        Options {
            ss,
            ..Options::default()
        },
    )
}

/// How the cube is drawn, apart from where it is looking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Samples per pixel along each axis, 1 to 4.
    pub ss: usize,
    /// How much of its square a sticker fills; the rest is black plastic. A
    /// smaller fill leaves wider gaps, which text views need because one
    /// character is a coarse pixel.
    pub sticker_fill: f64,
    /// 1.0 leaves room for the cube to turn to any angle without leaving the
    /// image; more fills the image and may clip a corner while it turns.
    pub zoom: f64,
    /// How strongly the light changes a face's brightness: a face lit
    /// straight on is full brightness, one turned away is `1 - shade_range`.
    /// 0.28 reads as a solid in a picture; text wants more contrast.
    pub shade_range: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            ss: 3,
            sticker_fill: 0.86,
            zoom: 1.0,
            shade_range: 0.28,
        }
    }
}

/// `render_with` with every option.
pub fn render_opts(
    cube: &Nxn,
    camera: Camera,
    turning: Option<Turning>,
    width: usize,
    height: usize,
    opts: Options,
) -> Image {
    let (ss, sticker_fill, shade_range) =
        (opts.ss, opts.sticker_fill, opts.shade_range.clamp(0.0, 0.9));
    let ss = ss.clamp(1, 4);
    let (w, h) = (width * ss, height * ss);
    let mut buf = vec![[0u8; 4]; w * h];
    if width == 0 || height == 0 {
        return Image {
            width,
            height,
            rgba: Vec::new(),
        };
    }

    let n = cube.size() as f64;
    let cell = 2.0 / n;
    let half_sticker = cell / 2.0 * sticker_fill.clamp(0.3, 0.98);
    let half_plate = cell / 2.0;
    let light = {
        let l: V3 = [-0.35, 0.8, 0.5];
        let len = dot(l, l).sqrt();
        scale(l, 1.0 / len)
    };

    // a turn in progress: which stickers move, about which axis, how far
    let spin = turning.map(|t| {
        // a clockwise turn seen from the face is negative about its outward axis
        let deg = -90.0 * t.m.turns as f64 * t.fraction * t.m.sign() as f64;
        (t.m, deg)
    });
    // a point of a sticker in a turning layer turns with it
    let turn_of = |center: V3| -> Option<(usize, f64)> {
        match spin {
            Some((m, deg)) if cube.in_layers(m, center) => Some((m.axis(), deg)),
            _ => None,
        }
    };

    let mut quads: Vec<Quad> = Vec::new();
    for (center, normal, color) in cube.stickers() {
        let normal_f = normal.map(|c| c as f64);
        // Build the plate and the sticker where they are, then turn all their
        // corners with the layer: a sticker turns as one rigid square.
        let turn = turn_of(center);
        let moved = |p: V3| match turn {
            Some((axis, deg)) => rotate(p, axis, deg),
            None => p,
        };
        let (c, nrm) = (moved(center), moved(normal_f));
        let (c_view, n_view) = (to_view(c, camera), to_view(nrm, camera));
        // seen from the eye at (0, 0, EYE): is the face turned towards it?
        let to_eye = add([0.0, 0.0, EYE], scale(c_view, -1.0));
        if dot(n_view, to_eye) <= 0.0 {
            continue;
        }
        let shade = (1.0 - shade_range) + shade_range * dot(nrm, light).max(0.0);
        let rgb = STICKER_RGB[color as usize].map(|x| (x as f64 * shade).round().min(255.0) as u8);
        let plate: Vec<V3> = square(center, normal_f, half_plate)
            .iter()
            .map(|&p| to_view(moved(p), camera))
            .collect();
        let sticker: Vec<V3> = rounded_square(
            add(center, scale(normal_f, 0.002)),
            normal_f,
            half_sticker,
            0.22,
        )
        .into_iter()
        .map(|p| to_view(moved(p), camera))
        .collect();
        quads.push(Quad {
            corners: plate,
            color: PLASTIC,
            bias: 0.001,
        });
        quads.push(Quad {
            corners: sticker,
            color: rgb,
            bias: 0.0,
        });
    }
    // the cut between the turning layers and the rest, as black squares
    if let Some((m, deg)) = spin {
        let layers = m.layers as f64;
        let depth = 1.0 - layers * cell; // where the cut is, from the center
        let mut axis_dir = [0.0; 3];
        axis_dir[m.axis()] = m.sign() as f64;
        let cut = scale(axis_dir, depth);
        for (rot, flip) in [(deg, 1.0), (0.0, -1.0)] {
            // the square is built unturned, then turned with its layer
            let corners: Vec<V3> = square(cut, scale(axis_dir, -flip), 1.0)
                .iter()
                .map(|&p| to_view(rotate(p, m.axis(), rot), camera))
                .collect();
            quads.push(Quad {
                corners,
                color: PLASTIC,
                bias: 0.002,
            });
        }
    }

    // perspective, sized so the whole cube fits whatever way it is turned
    let radius = 3f64.sqrt();
    let fit = radius * EYE / (EYE - radius);
    let pixels_per_unit = opts.zoom.clamp(0.5, 3.0) * 0.94 * (w.min(h) as f64) / 2.0 / fit;
    let project = |p: V3| -> (f64, f64) {
        let k = EYE / (EYE - p[2]);
        (
            w as f64 / 2.0 + p[0] * k * pixels_per_unit,
            h as f64 / 2.0 - p[1] * k * pixels_per_unit,
        )
    };
    let mut depth = vec![f64::NEG_INFINITY; w * h];
    let view = Projection {
        cx: w as f64 / 2.0,
        cy: h as f64 / 2.0,
        ppu: pixels_per_unit,
    };
    for q in &quads {
        let pts: Vec<(f64, f64)> = q.corners.iter().map(|&p| project(p)).collect();
        fill(
            &mut buf, &mut depth, w, h, &pts, &q.corners, view, q.bias, q.color,
        );
    }

    // average each SS x SS block, weighting colors by coverage so edges blend
    // into transparency instead of darkening
    let mut rgba = vec![0u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let (mut sum, mut alpha) = ([0u32; 3], 0u32);
            for dy in 0..ss {
                for dx in 0..ss {
                    let p = buf[(y * ss + dy) * w + x * ss + dx];
                    let a = p[3] as u32;
                    for k in 0..3 {
                        sum[k] += p[k] as u32 * a;
                    }
                    alpha += a;
                }
            }
            let i = (y * width + x) * 4;
            for k in 0..3 {
                rgba[i + k] = sum[k].checked_div(alpha).unwrap_or(0) as u8;
            }
            rgba[i + 3] = (alpha / (ss * ss) as u32) as u8;
        }
    }
    Image {
        width,
        height,
        rgba,
    }
}

/// How view space maps to the (supersampled) image.
#[derive(Clone, Copy)]
struct Projection {
    cx: f64,
    cy: f64,
    ppu: f64,
}

/// Fill a convex quad (either winding) with an opaque color, keeping only the
/// samples nearer than what is already there. Depth comes from intersecting
/// each sample's ray from the eye with the quad's plane, so it is exact for
/// any orientation. `bias` pushes the quad back a little so a sticker always
/// wins over the plate it sits on.
#[allow(clippy::too_many_arguments)]
fn fill(
    buf: &mut [[u8; 4]],
    depth: &mut [f64],
    w: usize,
    h: usize,
    pts: &[(f64, f64)],
    corners: &[V3],
    view: Projection,
    bias: f64,
    color: [u8; 3],
) {
    let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
    let x0 = xs.iter().copied().fold(f64::MAX, f64::min).floor().max(0.0) as usize;
    let x1 = (xs.iter().copied().fold(f64::MIN, f64::max).ceil().max(0.0) as usize).min(w);
    let y0 = ys.iter().copied().fold(f64::MAX, f64::min).floor().max(0.0) as usize;
    let y1 = (ys.iter().copied().fold(f64::MIN, f64::max).ceil().max(0.0) as usize).min(h);
    // the plane n·p = d through the polygon, by Newell's method (stable even
    // when neighbouring points are nearly in a line, as on a rounded corner)
    let mut n = [0.0; 3];
    for (i, a) in corners.iter().enumerate() {
        let b = corners[(i + 1) % corners.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let d = dot(n, corners[0]);
    let eye = [0.0, 0.0, EYE];
    let ne = dot(n, eye);
    for y in y0..y1 {
        // A convex polygon meets each row of samples in one span: find where
        // the row's center line crosses the edges.
        let sy = y as f64 + 0.5;
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            if (a.1 <= sy && sy <= b.1) || (b.1 <= sy && sy <= a.1) {
                if (b.1 - a.1).abs() < 1e-12 {
                    lo = lo.min(a.0.min(b.0));
                    hi = hi.max(a.0.max(b.0));
                } else {
                    let x = a.0 + (sy - a.1) / (b.1 - a.1) * (b.0 - a.0);
                    lo = lo.min(x);
                    hi = hi.max(x);
                }
            }
        }
        if lo > hi {
            continue;
        }
        // samples whose centers fall inside [lo, hi]
        let first = ((lo - 0.5).ceil().max(x0 as f64)) as usize;
        let last = ((hi - 0.5).floor().min(x1 as f64 - 1.0)) as i64;
        if (last as f64) < first as f64 {
            continue;
        }
        let dy = -(sy - view.cy) / view.ppu;
        for x in first..=last as usize {
            // the ray from the eye through this sample: eye + t * dir
            let dir = [(x as f64 + 0.5 - view.cx) / view.ppu, dy, -EYE];
            let denom = dot(n, dir);
            if denom.abs() < 1e-12 {
                continue; // seen edge-on
            }
            let t = (d - ne) / denom;
            let z = EYE - t * EYE - bias;
            let i = y * w + x;
            if z > depth[i] {
                depth[i] = z;
                buf[i] = [color[0], color[1], color[2], 255];
            }
        }
    }
}

/// Where the center of sticker (`face`, `row`, `col`) of a solved cube lands
/// in an image rendered with `camera`, if that face is visible. For tests and
/// for checking a picture against the model.
#[cfg(test)]
pub fn sticker_pixel(
    size: usize,
    camera: Camera,
    face: usize,
    row: usize,
    col: usize,
    width: usize,
    height: usize,
) -> Option<(usize, usize)> {
    // the same layout as `Nxn::facelets`: origin is the face's top-left corner
    // as seen from outside, then column and row directions
    let (origin, col_dir, row_dir, normal): (V3, V3, V3, V3) = match face {
        0 => (
            [-1.0, 1.0, -1.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0],
        ),
        1 => (
            [1.0, 1.0, 1.0],
            [0.0, 0.0, -1.0],
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
        ),
        2 => (
            [-1.0, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
        ),
        3 => (
            [-1.0, -1.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            [0.0, -1.0, 0.0],
        ),
        4 => (
            [-1.0, 1.0, -1.0],
            [0.0, 0.0, 1.0],
            [0.0, -1.0, 0.0],
            [-1.0, 0.0, 0.0],
        ),
        _ => (
            [1.0, 1.0, -1.0],
            [-1.0, 0.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, -1.0],
        ),
    };
    let cell = 2.0 / size as f64;
    let center = add(
        add(origin, scale(col_dir, (col as f64 + 0.5) * cell)),
        scale(row_dir, (row as f64 + 0.5) * cell),
    );
    let (c, nrm) = (to_view(center, camera), to_view(normal, camera));
    if dot(nrm, add([0.0, 0.0, EYE], scale(c, -1.0))) <= 0.0 {
        return None;
    }
    let radius = 3f64.sqrt();
    let fit = radius * EYE / (EYE - radius);
    let ppu = 0.94 * (width.min(height) as f64) / 2.0 / fit;
    let k = EYE / (EYE - c[2]);
    Some((
        (width as f64 / 2.0 + c[0] * k * ppu) as usize,
        (height as f64 / 2.0 - c[1] * k * ppu) as usize,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nxn;

    const W: usize = 240;
    const H: usize = 240;

    fn rgb(img: &Image, (x, y): (usize, usize)) -> [u8; 3] {
        let p = img.pixel(x, y);
        assert_eq!(p[3], 255, "opaque at {x},{y}");
        [p[0], p[1], p[2]]
    }

    /// Which face color a shaded pixel is: the closest by hue.
    fn face_of(c: [u8; 3]) -> usize {
        (0..6)
            .min_by_key(|&f| {
                let s = STICKER_RGB[f];
                // compare directions (shading only scales brightness)
                let norm = |v: [u8; 3]| {
                    let l = ((v[0] as f64).powi(2) + (v[1] as f64).powi(2) + (v[2] as f64).powi(2))
                        .sqrt()
                        .max(1.0);
                    [v[0] as f64 / l, v[1] as f64 / l, v[2] as f64 / l]
                };
                let (a, b) = (norm(c), norm(s));
                (((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)) * 1e6)
                    as i64
            })
            .unwrap()
    }

    fn cube_after(size: usize, moves: &str) -> Nxn {
        let mut c = Nxn::solved(size);
        c.apply_all(&nxn::parse(moves).unwrap());
        c
    }

    #[test]
    fn the_default_view_shows_u_f_and_r() {
        let cam = Camera::default();
        for size in 2..=7 {
            let img = render(&Nxn::solved(size), cam, None, W, H);
            for face in [0, 1, 2] {
                let at = sticker_pixel(size, cam, face, size / 2, size / 2, W, H).expect("visible");
                assert_eq!(face_of(rgb(&img, at)), face, "size {size} face {face}");
            }
            for face in [3, 4, 5] {
                assert!(
                    sticker_pixel(size, cam, face, 0, 0, W, H).is_none(),
                    "{face} hidden"
                );
            }
        }
    }

    #[test]
    fn every_face_can_be_seen_from_some_angle() {
        let views = [
            Camera::default(),
            Camera {
                yaw: 145.0,
                pitch: -28.0,
            }, // from below and behind: D, B, L
        ];
        for face in 0..6 {
            let seen = views.iter().any(|&cam| {
                sticker_pixel(3, cam, face, 1, 1, W, H).is_some_and(|at| {
                    face_of(rgb(&render(&Nxn::solved(3), cam, None, W, H), at)) == face
                })
            });
            assert!(seen, "face {face}");
        }
    }

    // The renderer must show the same stickers as the model: after R, the top
    // face's right column is green and the front face's right column yellow.
    #[test]
    fn a_turn_shows_up_in_the_right_place() {
        let cam = Camera::default();
        let img = render(&cube_after(3, "R"), cam, None, W, H);
        for row in 0..3 {
            let u_right = sticker_pixel(3, cam, 0, row, 2, W, H).unwrap();
            let u_left = sticker_pixel(3, cam, 0, row, 0, W, H).unwrap();
            let f_right = sticker_pixel(3, cam, 2, row, 2, W, H).unwrap();
            assert_eq!(face_of(rgb(&img, u_right)), 2, "U right column is green");
            assert_eq!(face_of(rgb(&img, u_left)), 0, "U left column still white");
            assert_eq!(face_of(rgb(&img, f_right)), 3, "F right column is yellow");
        }
    }

    // Every visible sticker of a scrambled cube matches the model's facelets.
    #[test]
    fn a_scrambled_cube_matches_the_model_sticker_for_sticker() {
        for (size, scramble) in [
            (3, "R U F' L2 D B' R2"),
            (4, "Rw U2 Fw' R D2"),
            (5, "3Rw' Uw F2 L Bw"),
        ] {
            let cube = cube_after(size, scramble);
            let facelets = cube.facelets();
            let cam = Camera::default();
            let img = render(&cube, cam, None, 320, 320);
            for face in [0, 1, 2] {
                for row in 0..size {
                    for col in 0..size {
                        let at = sticker_pixel(size, cam, face, row, col, 320, 320).unwrap();
                        let want = facelets[face * size * size + row * size + col] as usize;
                        assert_eq!(
                            face_of(rgb(&img, at)),
                            want,
                            "{size} face {face} ({row},{col})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_background_is_transparent_and_stickers_have_black_gaps() {
        let img = render(&Nxn::solved(3), Camera::default(), None, W, H);
        assert_eq!(img.pixel(0, 0)[3], 0, "corner of the image is empty");
        assert_eq!(img.pixel(W - 1, H - 1)[3], 0);
        // somewhere between two stickers there is dark plastic
        let dark = img
            .rgba
            .chunks(4)
            .filter(|p| p[3] == 255 && p[0] < 40 && p[1] < 40 && p[2] < 40)
            .count();
        assert!(dark > 200, "{dark}");
    }

    #[test]
    fn faces_are_shaded_differently() {
        let cam = Camera::default();
        let img = render(&Nxn::solved(3), cam, None, W, H);
        // white U and the other faces: U is lit from above, so brighter than
        // its own color would be on a side face
        let u = rgb(&img, sticker_pixel(3, cam, 0, 1, 1, W, H).unwrap());
        let r = rgb(&img, sticker_pixel(3, cam, 1, 1, 1, W, H).unwrap());
        let f = rgb(&img, sticker_pixel(3, cam, 2, 1, 1, W, H).unwrap());
        let brightness = |c: [u8; 3]| c[0] as u32 + c[1] as u32 + c[2] as u32;
        assert!(brightness(u) > 600);
        assert_ne!(r, STICKER_RGB[1], "R is shaded");
        assert_ne!(f, STICKER_RGB[2], "F is shaded");
    }

    #[test]
    fn the_cube_fits_whatever_way_it_is_turned() {
        for yaw in (0..360).step_by(30) {
            for pitch in [-80, -40, 0, 40, 80] {
                let cam = Camera {
                    yaw: yaw as f64,
                    pitch: pitch as f64,
                };
                let img = render(&Nxn::solved(3), cam, None, 100, 80);
                // nothing touches the image edges
                for x in 0..100 {
                    assert_eq!(img.pixel(x, 0)[3], 0, "{yaw} {pitch}");
                    assert_eq!(img.pixel(x, 79)[3], 0, "{yaw} {pitch}");
                }
                for y in 0..80 {
                    assert_eq!(img.pixel(0, y)[3], 0);
                    assert_eq!(img.pixel(99, y)[3], 0);
                }
            }
        }
    }

    #[test]
    fn a_half_finished_turn_is_between_before_and_after() {
        let cube = Nxn::solved(3);
        let cam = Camera::default();
        let m = nxn::parse("U").unwrap()[0];
        let start = render(&cube, cam, Some(Turning { m, fraction: 0.0 }), W, H);
        let still = render(&cube, cam, None, W, H);
        assert_eq!(start, still, "no rotation at 0");
        let mid = render(&cube, cam, Some(Turning { m, fraction: 0.5 }), W, H);
        assert_ne!(mid, still);
        // the bottom of the cube (not turning) is unchanged at the F center row
        let below = sticker_pixel(3, cam, 2, 2, 1, W, H).unwrap();
        assert_eq!(mid.pixel(below.0, below.1), still.pixel(below.0, below.1));
        // and the cube is still solid: no transparent hole in its middle
        assert_eq!(mid.pixel(W / 2, H / 2)[3], 255);
    }

    #[test]
    fn a_whole_turn_looks_like_the_move_applied() {
        let cam = Camera::default();
        for mv in ["R", "U'", "F2", "Rw"] {
            let m = nxn::parse(mv).unwrap()[0];
            let animated = render(
                &Nxn::solved(4),
                cam,
                Some(Turning { m, fraction: 1.0 }),
                W,
                H,
            );
            let applied = render(&cube_after(4, mv), cam, None, W, H);
            // the same picture, give or take anti-aliasing along the edges
            let differ = animated
                .rgba
                .chunks(4)
                .zip(applied.rgba.chunks(4))
                .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 40))
                .count();
            assert!(differ < W * H / 200, "{mv}: {differ} pixels differ");
        }
    }

    #[test]
    fn camera_turns_wrap_and_tilt_is_limited() {
        let c = Camera::default().turned(400.0, 200.0);
        assert!((0.0..360.0).contains(&c.yaw));
        assert_eq!(c.pitch, 85.0);
        assert_eq!(Camera::default().turned(0.0, -500.0).pitch, -85.0);
    }

    #[test]
    fn empty_images_do_not_panic() {
        let img = render(&Nxn::solved(3), Camera::default(), None, 0, 10);
        assert!(img.rgba.is_empty());
        render(&Nxn::solved(3), Camera::default(), None, 1, 1);
    }

    #[test]
    fn lower_quality_draws_the_same_cube() {
        let cube = cube_after(3, "R U");
        let cam = Camera::default();
        for ss in [1, 2, 4] {
            let img = render_with(&cube, cam, None, W, H, ss);
            for face in [0, 1, 2] {
                let at = sticker_pixel(3, cam, face, 1, 1, W, H).unwrap();
                let want = cube.facelets()[face * 9 + 4] as usize;
                assert_eq!(face_of(rgb(&img, at)), want, "ss {ss}");
            }
        }
    }

    /// Transparent pixels the border can't reach through other transparent
    /// pixels: holes in the cube.
    fn holes(img: &Image) -> usize {
        let (w, h) = (img.width, img.height);
        let clear = |x: usize, y: usize| img.pixel(x, y)[3] < 128;
        let mut seen = vec![false; w * h];
        let mut stack: Vec<(usize, usize)> = Vec::new();
        for x in 0..w {
            stack.push((x, 0));
            stack.push((x, h - 1));
        }
        for y in 0..h {
            stack.push((0, y));
            stack.push((w - 1, y));
        }
        while let Some((x, y)) = stack.pop() {
            if seen[y * w + x] || !clear(x, y) {
                continue;
            }
            seen[y * w + x] = true;
            if x > 0 {
                stack.push((x - 1, y));
            }
            if x + 1 < w {
                stack.push((x + 1, y));
            }
            if y > 0 {
                stack.push((x, y - 1));
            }
            if y + 1 < h {
                stack.push((x, y + 1));
            }
        }
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|&(x, y)| clear(x, y) && !seen[y * w + x])
            .count()
    }

    // The bug the first version had: mid-turn, the turning layer looked hollow
    // and its stickers floated apart.
    #[test]
    fn a_turning_layer_is_solid_at_every_angle() {
        for (size, moves) in [
            (3, ["R", "L2", "U'", "D", "F", "B2"]),
            (4, ["Rw", "U", "Lw'", "F2", "D", "B"]),
        ] {
            for mv in moves {
                let m = nxn::parse(mv).unwrap()[0];
                for cam in [
                    Camera::default(),
                    Camera {
                        yaw: 150.0,
                        pitch: -30.0,
                    },
                ] {
                    for k in 1..8 {
                        let turning = Some(Turning {
                            m,
                            fraction: k as f64 / 8.0,
                        });
                        let img = render(&Nxn::solved(size), cam, turning, 160, 160);
                        assert_eq!(holes(&img), 0, "{size}x{size} {mv} at {k}/8, {cam:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_cut_inside_a_turning_layer_is_black_plastic() {
        // halfway through R, looking from the front right, the gap between the
        // R layer and the rest shows black plastic, not a sticker color
        let m = nxn::parse("R").unwrap()[0];
        let img = render(
            &Nxn::solved(3),
            Camera::default(),
            Some(Turning { m, fraction: 0.5 }),
            300,
            300,
        );
        let dark = img
            .rgba
            .chunks(4)
            .filter(|p| p[3] == 255 && p[0] < 40 && p[1] < 40 && p[2] < 40)
            .count();
        let still = render(&Nxn::solved(3), Camera::default(), None, 300, 300);
        let dark_still = still
            .rgba
            .chunks(4)
            .filter(|p| p[3] == 255 && p[0] < 40 && p[1] < 40 && p[2] < 40)
            .count();
        assert!(dark > dark_still + 200, "{dark} vs {dark_still}");
    }
}
