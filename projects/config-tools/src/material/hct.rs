//! Hue, chroma and tone: Material's perceptual colour space, as Material Color
//! Utilities defines it (CAM16 under its default viewing conditions, with
//! L* as tone), and the contrast arithmetic dynamic colour is built on.
//!
//! A line-for-line port of the Apache-2.0 `material-color-utilities`
//! TypeScript sources (`hct/`, `utils/`, `contrast/`, `palettes/tonal_palette`
//! and `blend/`). `tests/material-oracle.json` pins its output to the
//! reference implementation.

pub type Argb = u32;

const SRGB_TO_XYZ: [[f64; 3]; 3] = [
    [0.41233895, 0.35762064, 0.18051042],
    [0.2126, 0.7152, 0.0722],
    [0.01932141, 0.11916382, 0.95034478],
];
const WHITE_POINT_D65: [f64; 3] = [95.047, 100.0, 108.883];

pub fn clamp(min: f64, max: f64, input: f64) -> f64 {
    if input < min {
        min
    } else if input > max {
        max
    } else {
        input
    }
}
pub fn sanitize_degrees(degrees: f64) -> f64 {
    let degrees = degrees % 360.0;
    if degrees < 0.0 {
        degrees + 360.0
    } else {
        degrees
    }
}
fn signum(number: f64) -> f64 {
    if number < 0.0 {
        -1.0
    } else if number == 0.0 {
        0.0
    } else {
        1.0
    }
}
fn lerp(start: f64, stop: f64, amount: f64) -> f64 {
    (1.0 - amount) * start + amount * stop
}
fn multiply(row: [f64; 3], matrix: &[[f64; 3]; 3]) -> [f64; 3] {
    let dot = |line: &[f64; 3]| row[0] * line[0] + row[1] * line[1] + row[2] * line[2];
    [dot(&matrix[0]), dot(&matrix[1]), dot(&matrix[2])]
}
// JavaScript's Math.round, which rounds halves up rather than away from zero.
fn round(value: f64) -> f64 {
    (value + 0.5).floor()
}

pub fn argb_from_rgb(red: u8, green: u8, blue: u8) -> Argb {
    0xff00_0000 | (u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue)
}
fn channel(argb: Argb, shift: u32) -> u8 {
    ((argb >> shift) & 0xff) as u8
}
pub fn red(argb: Argb) -> u8 {
    channel(argb, 16)
}
pub fn green(argb: Argb) -> u8 {
    channel(argb, 8)
}
pub fn blue(argb: Argb) -> u8 {
    channel(argb, 0)
}
fn linearized(component: u8) -> f64 {
    let normalized = f64::from(component) / 255.0;
    if normalized <= 0.040449936 {
        normalized / 12.92 * 100.0
    } else {
        ((normalized + 0.055) / 1.055).powf(2.4) * 100.0
    }
}
fn delinearized(component: f64) -> u8 {
    let normalized = component / 100.0;
    let value = if normalized <= 0.0031308 {
        normalized * 12.92
    } else {
        1.055 * normalized.powf(1.0 / 2.4) - 0.055
    };
    clamp(0.0, 255.0, round(value * 255.0)) as u8
}
fn argb_from_linrgb(linrgb: [f64; 3]) -> Argb {
    argb_from_rgb(
        delinearized(linrgb[0]),
        delinearized(linrgb[1]),
        delinearized(linrgb[2]),
    )
}
fn xyz_from_argb(argb: Argb) -> [f64; 3] {
    multiply(
        [
            linearized(red(argb)),
            linearized(green(argb)),
            linearized(blue(argb)),
        ],
        &SRGB_TO_XYZ,
    )
}
fn lab_f(t: f64) -> f64 {
    let e = 216.0 / 24389.0;
    let kappa = 24389.0 / 27.0;
    if t > e {
        t.powf(1.0 / 3.0)
    } else {
        (kappa * t + 16.0) / 116.0
    }
}
fn lab_invf(ft: f64) -> f64 {
    let e = 216.0 / 24389.0;
    let kappa = 24389.0 / 27.0;
    let ft3 = ft * ft * ft;
    if ft3 > e {
        ft3
    } else {
        (116.0 * ft - 16.0) / kappa
    }
}
pub fn y_from_lstar(lstar: f64) -> f64 {
    100.0 * lab_invf((lstar + 16.0) / 116.0)
}
pub fn lstar_from_y(y: f64) -> f64 {
    lab_f(y / 100.0) * 116.0 - 16.0
}
fn lstar_from_argb(argb: Argb) -> f64 {
    116.0 * lab_f(xyz_from_argb(argb)[1] / 100.0) - 16.0
}
fn argb_from_lstar(lstar: f64) -> Argb {
    let component = delinearized(y_from_lstar(lstar));
    argb_from_rgb(component, component, component)
}

/// CAM16's default viewing conditions: D65, an adapting luminance of an L* 50
/// grey at 200 lux, an L* 50 background, average surround.
struct Viewing {
    n: f64,
    aw: f64,
    nbb: f64,
    ncb: f64,
    c: f64,
    nc: f64,
    rgb_d: [f64; 3],
    fl: f64,
    z: f64,
}
fn viewing() -> &'static Viewing {
    static VIEWING: std::sync::OnceLock<Viewing> = std::sync::OnceLock::new();
    VIEWING.get_or_init(|| {
        let white = WHITE_POINT_D65;
        let adapting = (200.0 / std::f64::consts::PI) * y_from_lstar(50.0) / 100.0;
        let (background, surround) = (50.0, 2.0);
        let r_w = white[0] * 0.401288 + white[1] * 0.650173 + white[2] * -0.051461;
        let g_w = white[0] * -0.250268 + white[1] * 1.204414 + white[2] * 0.045854;
        let b_w = white[0] * -0.002079 + white[1] * 0.048952 + white[2] * 0.953127;
        let f = 0.8 + surround / 10.0;
        let c = if f >= 0.9 {
            lerp(0.59, 0.69, (f - 0.9) * 10.0)
        } else {
            lerp(0.525, 0.59, (f - 0.8) * 10.0)
        };
        let d = clamp(
            0.0,
            1.0,
            f * (1.0 - (1.0 / 3.6) * ((-adapting - 42.0) / 92.0).exp()),
        );
        let rgb_d = [
            d * (100.0 / r_w) + 1.0 - d,
            d * (100.0 / g_w) + 1.0 - d,
            d * (100.0 / b_w) + 1.0 - d,
        ];
        let k = 1.0 / (5.0 * adapting + 1.0);
        let k4 = k * k * k * k;
        let k4f = 1.0 - k4;
        let fl = k4 * adapting + 0.1 * k4f * k4f * (5.0 * adapting).cbrt();
        let n = y_from_lstar(background) / white[1];
        let z = 1.48 + n.sqrt();
        let nbb = 0.725 / n.powf(0.2);
        let factor = |component: f64, white: f64| (fl * component * white / 100.0).powf(0.42);
        let factors = [
            factor(rgb_d[0], r_w),
            factor(rgb_d[1], g_w),
            factor(rgb_d[2], b_w),
        ];
        let rgb_a = factors.map(|factor| 400.0 * factor / (factor + 27.13));
        let aw = (2.0 * rgb_a[0] + rgb_a[1] + 0.05 * rgb_a[2]) * nbb;
        Viewing {
            n,
            aw,
            nbb,
            ncb: nbb,
            c,
            nc: f,
            rgb_d,
            fl,
            z,
        }
    })
}

/// CAM16 hue and chroma of `argb` under the default viewing conditions.
fn cam16(argb: Argb) -> (f64, f64) {
    let vc = viewing();
    let (r, g, b) = (
        linearized(red(argb)),
        linearized(green(argb)),
        linearized(blue(argb)),
    );
    let x = 0.41233895 * r + 0.35762064 * g + 0.18051042 * b;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = 0.01932141 * r + 0.11916382 * g + 0.95034478 * b;
    let r_c = 0.401288 * x + 0.650173 * y - 0.051461 * z;
    let g_c = -0.250268 * x + 1.204414 * y + 0.045854 * z;
    let b_c = -0.002079 * x + 0.048952 * y + 0.953127 * z;
    let adapt = |component: f64, d: f64| {
        let scaled = d * component;
        let af = (vc.fl * scaled.abs() / 100.0).powf(0.42);
        signum(scaled) * 400.0 * af / (af + 27.13)
    };
    let (r_a, g_a, b_a) = (
        adapt(r_c, vc.rgb_d[0]),
        adapt(g_c, vc.rgb_d[1]),
        adapt(b_c, vc.rgb_d[2]),
    );
    let a = (11.0 * r_a + -12.0 * g_a + b_a) / 11.0;
    let b = (r_a + g_a - 2.0 * b_a) / 9.0;
    let u = (20.0 * r_a + 20.0 * g_a + 21.0 * b_a) / 20.0;
    let p2 = (40.0 * r_a + 20.0 * g_a + b_a) / 20.0;
    let hue = sanitize_degrees(b.atan2(a) * 180.0 / std::f64::consts::PI);
    let ac = p2 * vc.nbb;
    let j = 100.0 * (ac / vc.aw).powf(vc.c * vc.z);
    let hue_prime = if hue < 20.14 { hue + 360.0 } else { hue };
    let e_hue = 0.25 * ((hue_prime * std::f64::consts::PI / 180.0 + 2.0).cos() + 3.8);
    let p1 = (50000.0 / 13.0) * e_hue * vc.nc * vc.ncb;
    let t = p1 * (a * a + b * b).sqrt() / (u + 0.305);
    let alpha = t.powf(0.9) * (1.64 - 0.29f64.powf(vc.n)).powf(0.73);
    (hue, alpha * (j / 100.0).sqrt())
}

// The HCT solver: the sRGB colour of a hue, chroma and tone, or the most
// chromatic one of that hue and tone when the chroma is out of gamut.
const SCALED_DISCOUNT_FROM_LINRGB: [[f64; 3]; 3] = [
    [
        0.001200833568784504,
        0.002389694492170889,
        0.0002795742885861124,
    ],
    [
        0.0005891086651375999,
        0.0029785502573438758,
        0.0003270666104008398,
    ],
    [
        0.00010146692491640572,
        0.0005364214359186694,
        0.0032979401770712076,
    ],
];
const LINRGB_FROM_SCALED_DISCOUNT: [[f64; 3]; 3] = [
    [1373.2198709594231, -1100.4251190754821, -7.278681089101213],
    [-271.815969077903, 559.6580465940733, -32.46047482791194],
    [1.9622899599665666, -57.173814538844006, 308.7233197812385],
];
const Y_FROM_LINRGB: [f64; 3] = [0.2126, 0.7152, 0.0722];
#[rustfmt::skip]
const CRITICAL_PLANES: [f64; 255] = [
    0.015176349177441876, 0.045529047532325624, 0.07588174588720938,
    0.10623444424209313, 0.13658714259697685, 0.16693984095186062,
    0.19729253930674434, 0.2276452376616281, 0.2579979360165119,
    0.28835063437139563, 0.3188300904430532, 0.350925934958123,
    0.3848314933096426, 0.42057480301049466, 0.458183274052838,
    0.4976837250274023, 0.5391024159806381, 0.5824650784040898,
    0.6277969426914107, 0.6751227633498623, 0.7244668422128921,
    0.775853049866786, 0.829304845476233, 0.8848452951698498,
    0.942497089126609, 1.0022825574869039, 1.0642236851973577,
    1.1283421258858297, 1.1946592148522128, 1.2631959812511864,
    1.3339731595349034, 1.407011200216447, 1.4823302800086415,
    1.5599503113873272, 1.6398909516233677, 1.7221716113234105,
    1.8068114625156377, 1.8938294463134073, 1.9832442801866852,
    2.075074464868551, 2.1693382909216234, 2.2660538449872063,
    2.36523901573795, 2.4669114995532007, 2.5710888059345764,
    2.6777882626779785, 2.7870270208169257, 2.898822059350997,
    3.0131901897720907, 3.1301480604002863, 3.2497121605402226,
    3.3718988244681087, 3.4967242352587946, 3.624204428461639,
    3.754355295633311, 3.887192587735158, 4.022731918402185,
    4.160988767090289, 4.301978482107941, 4.445716283538092,
    4.592217266055746, 4.741496401646282, 4.893568542229298,
    5.048448422192488, 5.20615066083972, 5.3666897647573375,
    5.5300801301023865, 5.696336044816294, 5.865471690767354,
    6.037501145825082, 6.212438385869475, 6.390297286737924,
    6.571091626112461, 6.7548350853498045, 6.941541251256611,
    7.131223617812143, 7.323895587840543, 7.5195704746346665,
    7.7182615035334345, 7.919981813454504, 8.124744458384042,
    8.332562408825165, 8.543448553206703, 8.757415699253682,
    8.974476575321063, 9.194643831691977, 9.417930041841839,
    9.644347703669503, 9.873909240696694, 10.106627003236781,
    10.342513269534024, 10.58158024687427, 10.8238400726681,
    11.069304815507364, 11.317986476196008, 11.569896988756009,
    11.825048221409341, 12.083451977536606, 12.345119996613247,
    12.610063955123938, 12.878295467455942, 13.149826086772048,
    13.42466730586372, 13.702830557985108, 13.984327217668513,
    14.269168601521828, 14.55736596900856, 14.848930523210871,
    15.143873411576273, 15.44220572664832, 15.743938506781891,
    16.04908273684337, 16.35764934889634, 16.66964922287304,
    16.985093187232053, 17.30399201960269, 17.62635644741625,
    17.95219714852476, 18.281524751807332, 18.614349837764564,
    18.95068293910138, 19.290534541298456, 19.633915083172692,
    19.98083495742689, 20.331304511189067, 20.685334046541502,
    21.042933821039977, 21.404114048223256, 21.76888489811322,
    22.137256497705877, 22.50923893145328, 22.884842241736916,
    23.264076429332462, 23.6469514538663, 24.033477234264016,
    24.42366364919083, 24.817520537484558, 25.21505769858089,
    25.61628489293138, 26.021211842414342, 26.429848230738664,
    26.842203703840827, 27.258287870275353, 27.678110301598522,
    28.10168053274597, 28.529008062403893, 28.96010235337422,
    29.39497283293396, 29.83362889318845, 30.276079891419332,
    30.722335150426627, 31.172403958865512, 31.62629557157785,
    32.08401920991837, 32.54558406207592, 33.010999283389665,
    33.4802739966603, 33.953417292456834, 34.430438229418264,
    34.911345834551085, 35.39614910352207, 35.88485700094671,
    36.37747846067349, 36.87402238606382, 37.37449765026789,
    37.87891309649659, 38.38727753828926, 38.89959975977785,
    39.41588851594697, 39.93615253289054, 40.460400508064545,
    40.98864111053629, 41.520882981230194, 42.05713473317016,
    42.597404951718396, 43.141702194811224, 43.6900349931913,
    44.24241185063697, 44.798841244188324, 45.35933162437017,
    45.92389141541209, 46.49252901546552, 47.065252796817916,
    47.64207110610409, 48.22299226451468, 48.808024568002054,
    49.3971762874833, 49.9904556690408, 50.587870934119984,
    51.189430279724725, 51.79514187861014, 52.40501387947288,
    53.0190544071392, 53.637271562750364, 54.259673423945976,
    54.88626804504493, 55.517063457223934, 56.15206766869424,
    56.79128866487574, 57.43473440856916, 58.08241284012621,
    58.734331877617365, 59.39049941699807, 60.05092333227251,
    60.715611475655585, 61.38457167773311, 62.057811747619894,
    62.7353394731159, 63.417162620860914, 64.10328893648692,
    64.79372614476921, 65.48848194977529, 66.18756403501224,
    66.89098006357258, 67.59873767827808, 68.31084450182222,
    69.02730813691093, 69.74813616640164, 70.47333615344107,
    71.20291564160104, 71.93688215501312, 72.67524319850172,
    73.41800625771542, 74.16517879925733, 74.9167682708136,
    75.67278210128072, 76.43322770089146, 77.1981124613393,
    77.96744375590167, 78.74122893956174, 79.51947534912904,
    80.30219030335869, 81.08938110306934, 81.88105503125999,
    82.67721935322541, 83.4778813166706, 84.28304815182372,
    85.09272707154808, 85.90692527145302, 86.72564993000343,
    87.54890820862819, 88.3767072518277, 89.2090541872801,
    90.04595612594655, 90.88742016217518, 91.73345337380438,
    92.58406282226491, 93.43925555268066, 94.29903859396902,
    95.16341895893969, 96.03240364439274, 96.9059996312159,
    97.78421388448044, 98.6670533535366, 99.55452497210776,
];

fn sanitize_radians(angle: f64) -> f64 {
    (angle + std::f64::consts::PI * 8.0) % (std::f64::consts::PI * 2.0)
}
fn true_delinearized(component: f64) -> f64 {
    let normalized = component / 100.0;
    let value = if normalized <= 0.0031308 {
        normalized * 12.92
    } else {
        1.055 * normalized.powf(1.0 / 2.4) - 0.055
    };
    value * 255.0
}
fn chromatic_adaptation(component: f64) -> f64 {
    let af = component.abs().powf(0.42);
    signum(component) * 400.0 * af / (af + 27.13)
}
fn hue_of(linrgb: [f64; 3]) -> f64 {
    let scaled = multiply(linrgb, &SCALED_DISCOUNT_FROM_LINRGB);
    let (r_a, g_a, b_a) = (
        chromatic_adaptation(scaled[0]),
        chromatic_adaptation(scaled[1]),
        chromatic_adaptation(scaled[2]),
    );
    let a = (11.0 * r_a + -12.0 * g_a + b_a) / 11.0;
    let b = (r_a + g_a - 2.0 * b_a) / 9.0;
    b.atan2(a)
}
fn in_cyclic_order(a: f64, b: f64, c: f64) -> bool {
    sanitize_radians(b - a) < sanitize_radians(c - a)
}
fn set_coordinate(source: [f64; 3], coordinate: f64, target: [f64; 3], axis: usize) -> [f64; 3] {
    let t = (coordinate - source[axis]) / (target[axis] - source[axis]);
    [
        source[0] + (target[0] - source[0]) * t,
        source[1] + (target[1] - source[1]) * t,
        source[2] + (target[2] - source[2]) * t,
    ]
}
fn bounded(x: f64) -> bool {
    (0.0..=100.0).contains(&x)
}
fn nth_vertex(y: f64, n: usize) -> Option<[f64; 3]> {
    let [k_r, k_g, k_b] = Y_FROM_LINRGB;
    let coord_a = if n % 4 <= 1 { 0.0 } else { 100.0 };
    let coord_b = if n.is_multiple_of(2) { 0.0 } else { 100.0 };
    let vertex = if n < 4 {
        let (g, b) = (coord_a, coord_b);
        [(y - g * k_g - b * k_b) / k_r, g, b]
    } else if n < 8 {
        let (b, r) = (coord_a, coord_b);
        [r, (y - r * k_r - b * k_b) / k_g, b]
    } else {
        let (r, g) = (coord_a, coord_b);
        [r, g, (y - r * k_r - g * k_g) / k_b]
    };
    let solved = if n < 4 {
        0
    } else if n < 8 {
        1
    } else {
        2
    };
    bounded(vertex[solved]).then_some(vertex)
}
fn bisect_to_segment(y: f64, target: f64) -> ([f64; 3], [f64; 3]) {
    let mut left = [-1.0; 3];
    let mut right = left;
    let (mut left_hue, mut right_hue) = (0.0, 0.0);
    let (mut initialized, mut uncut) = (false, true);
    for n in 0..12 {
        let Some(mid) = nth_vertex(y, n) else {
            continue;
        };
        let mid_hue = hue_of(mid);
        if !initialized {
            (left, right, left_hue, right_hue, initialized) = (mid, mid, mid_hue, mid_hue, true);
            continue;
        }
        if uncut || in_cyclic_order(left_hue, mid_hue, right_hue) {
            uncut = false;
            if in_cyclic_order(left_hue, target, mid_hue) {
                (right, right_hue) = (mid, mid_hue);
            } else {
                (left, left_hue) = (mid, mid_hue);
            }
        }
    }
    (left, right)
}
fn bisect_to_limit(y: f64, target: f64) -> [f64; 3] {
    let (mut left, mut right) = bisect_to_segment(y, target);
    let mut left_hue = hue_of(left);
    for axis in 0..3 {
        if left[axis] == right[axis] {
            continue;
        }
        let (mut l_plane, mut r_plane) = if left[axis] < right[axis] {
            (
                (true_delinearized(left[axis]) - 0.5).floor(),
                (true_delinearized(right[axis]) - 0.5).ceil(),
            )
        } else {
            (
                (true_delinearized(left[axis]) - 0.5).ceil(),
                (true_delinearized(right[axis]) - 0.5).floor(),
            )
        };
        for _ in 0..8 {
            if (r_plane - l_plane).abs() <= 1.0 {
                break;
            }
            let m_plane = ((l_plane + r_plane) / 2.0).floor();
            let mid = set_coordinate(left, CRITICAL_PLANES[m_plane as usize], right, axis);
            let mid_hue = hue_of(mid);
            if in_cyclic_order(left_hue, target, mid_hue) {
                right = mid;
                r_plane = m_plane;
            } else {
                left = mid;
                left_hue = mid_hue;
                l_plane = m_plane;
            }
        }
    }
    [
        (left[0] + right[0]) / 2.0,
        (left[1] + right[1]) / 2.0,
        (left[2] + right[2]) / 2.0,
    ]
}
fn inverse_chromatic_adaptation(adapted: f64) -> f64 {
    let magnitude = adapted.abs();
    let base = (27.13 * magnitude / (400.0 - magnitude)).max(0.0);
    signum(adapted) * base.powf(1.0 / 0.42)
}
fn find_result_by_j(hue: f64, chroma: f64, y: f64) -> Option<Argb> {
    let vc = viewing();
    let mut j = y.sqrt() * 11.0;
    let t_inner = 1.0 / (1.64 - 0.29f64.powf(vc.n)).powf(0.73);
    let e_hue = 0.25 * ((hue + 2.0).cos() + 3.8);
    let p1 = e_hue * (50000.0 / 13.0) * vc.nc * vc.ncb;
    let (h_sin, h_cos) = (hue.sin(), hue.cos());
    for round in 0..5 {
        let j_normalized = j / 100.0;
        let alpha = if chroma == 0.0 || j == 0.0 {
            0.0
        } else {
            chroma / j_normalized.sqrt()
        };
        let t = (alpha * t_inner).powf(1.0 / 0.9);
        let ac = vc.aw * j_normalized.powf(1.0 / vc.c / vc.z);
        let p2 = ac / vc.nbb;
        let gamma = 23.0 * (p2 + 0.305) * t / (23.0 * p1 + 11.0 * t * h_cos + 108.0 * t * h_sin);
        let (a, b) = (gamma * h_cos, gamma * h_sin);
        let r_a = (460.0 * p2 + 451.0 * a + 288.0 * b) / 1403.0;
        let g_a = (460.0 * p2 - 891.0 * a - 261.0 * b) / 1403.0;
        let b_a = (460.0 * p2 - 220.0 * a - 6300.0 * b) / 1403.0;
        let linrgb = multiply(
            [
                inverse_chromatic_adaptation(r_a),
                inverse_chromatic_adaptation(g_a),
                inverse_chromatic_adaptation(b_a),
            ],
            &LINRGB_FROM_SCALED_DISCOUNT,
        );
        if linrgb.iter().any(|component| *component < 0.0) {
            return None;
        }
        let fnj = Y_FROM_LINRGB[0] * linrgb[0]
            + Y_FROM_LINRGB[1] * linrgb[1]
            + Y_FROM_LINRGB[2] * linrgb[2];
        if fnj <= 0.0 {
            return None;
        }
        if round == 4 || (fnj - y).abs() < 0.002 {
            if linrgb.iter().any(|component| *component > 100.01) {
                return None;
            }
            return Some(argb_from_linrgb(linrgb));
        }
        j -= (fnj - y) * j / (2.0 * fnj);
    }
    None
}
fn solve(hue: f64, chroma: f64, lstar: f64) -> Argb {
    if chroma < 0.0001 || !(0.0001..=99.9999).contains(&lstar) {
        return argb_from_lstar(lstar);
    }
    let hue = sanitize_degrees(hue) / 180.0 * std::f64::consts::PI;
    let y = y_from_lstar(lstar);
    find_result_by_j(hue, chroma, y).unwrap_or_else(|| argb_from_linrgb(bisect_to_limit(y, hue)))
}

/// A colour by hue, chroma and tone, holding the sRGB colour it resolved to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hct {
    pub hue: f64,
    pub chroma: f64,
    pub tone: f64,
    pub argb: Argb,
}
impl Hct {
    pub fn from_argb(argb: Argb) -> Hct {
        let (hue, chroma) = cam16(argb);
        Hct {
            hue,
            chroma,
            tone: lstar_from_argb(argb),
            argb,
        }
    }
    pub fn new(hue: f64, chroma: f64, tone: f64) -> Hct {
        Hct::from_argb(solve(hue, chroma, tone))
    }
    pub fn is_blue(hue: f64) -> bool {
        (250.0..270.0).contains(&hue)
    }
    pub fn is_yellow(hue: f64) -> bool {
        (105.0..125.0).contains(&hue)
    }
    pub fn is_cyan(hue: f64) -> bool {
        (170.0..207.0).contains(&hue)
    }
}

// Contrast between tones, as WCAG measures it on their luminance.
pub fn ratio_of_tones(a: f64, b: f64) -> f64 {
    ratio_of_ys(
        y_from_lstar(clamp(0.0, 100.0, a)),
        y_from_lstar(clamp(0.0, 100.0, b)),
    )
}
fn ratio_of_ys(y1: f64, y2: f64) -> f64 {
    let lighter = if y1 > y2 { y1 } else { y2 };
    let darker = if lighter == y2 { y1 } else { y2 };
    (lighter + 5.0) / (darker + 5.0)
}
/// The tone `ratio` lighter than `tone`, or none within 0..=100.
pub fn lighter(tone: f64, ratio: f64) -> Option<f64> {
    if !(0.0..=100.0).contains(&tone) {
        return None;
    }
    let dark_y = y_from_lstar(tone);
    let light_y = ratio * (dark_y + 5.0) - 5.0;
    let real = ratio_of_ys(light_y, dark_y);
    if real < ratio && (real - ratio).abs() > 0.04 {
        return None;
    }
    let value = lstar_from_y(light_y) + 0.4;
    (0.0..=100.0).contains(&value).then_some(value)
}
/// The tone `ratio` darker than `tone`, or none within 0..=100.
pub fn darker(tone: f64, ratio: f64) -> Option<f64> {
    if !(0.0..=100.0).contains(&tone) {
        return None;
    }
    let light_y = y_from_lstar(tone);
    let dark_y = (light_y + 5.0) / ratio - 5.0;
    let real = ratio_of_ys(light_y, dark_y);
    if real < ratio && (real - ratio).abs() > 0.04 {
        return None;
    }
    let value = lstar_from_y(dark_y) - 0.4;
    (0.0..=100.0).contains(&value).then_some(value)
}
pub fn lighter_unsafe(tone: f64, ratio: f64) -> f64 {
    lighter(tone, ratio).unwrap_or(100.0)
}
pub fn darker_unsafe(tone: f64, ratio: f64) -> f64 {
    darker(tone, ratio).unwrap_or(0.0)
}

/// Every tone of one hue at one chroma.
#[derive(Clone, Copy, Debug)]
pub struct TonalPalette {
    pub hue: f64,
    pub chroma: f64,
}
impl TonalPalette {
    pub fn new(hue: f64, chroma: f64) -> TonalPalette {
        TonalPalette { hue, chroma }
    }
    pub fn from_argb(argb: Argb) -> TonalPalette {
        let hct = Hct::from_argb(argb);
        TonalPalette::new(hct.hue, hct.chroma)
    }
    pub fn tone(&self, tone: f64) -> Argb {
        // A yellow's tone 99 is the average of 98 and 100, which keeps it
        // from reading as a cream.
        if tone == 99.0 && Hct::is_yellow(self.hue) {
            let (a, b) = (self.tone(98.0), self.tone(100.0));
            let mean = |c: fn(Argb) -> u8| round((f64::from(c(a)) + f64::from(c(b))) / 2.0) as u8;
            return argb_from_rgb(mean(red), mean(green), mean(blue));
        }
        solve(self.hue, self.chroma, tone)
    }
    pub fn hct(&self, tone: f64) -> Hct {
        Hct::from_argb(self.tone(tone))
    }
}

/// `design` turned toward `source` by half their hue difference, at most
/// 15 degrees: Material's harmonized custom colours.
pub fn harmonize(design: Argb, source: Argb) -> Argb {
    let (from, to) = (Hct::from_argb(design), Hct::from_argb(source));
    let difference = 180.0 - ((from.hue - to.hue).abs() - 180.0).abs();
    let rotation = (difference * 0.5).min(15.0);
    let direction = if sanitize_degrees(to.hue - from.hue) <= 180.0 {
        1.0
    } else {
        -1.0
    };
    Hct::new(
        sanitize_degrees(from.hue + rotation * direction),
        from.chroma,
        from.tone,
    )
    .argb
}

pub fn hex(argb: Argb) -> String {
    format!("#{:06x}", argb & 0x00ff_ffff)
}
pub fn parse(hex: &str) -> Option<Argb> {
    let digits = hex.strip_prefix('#')?;
    (digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| 0xff00_0000 | u32::from_str_radix(digits, 16).unwrap_or(0))
}
