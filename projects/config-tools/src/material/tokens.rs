//! Material 3 Expressive's design tokens, and the theme document that sets
//! the shell's contract slots to them.
//!
//! The reference values are Material's own system tokens: the type scale,
//! the corner scale, elevation, state layers and motion, as Material Web
//! publishes them (`tokens/versions/latest`) and, for the Expressive motion
//! scheme's springs, as Compose Material 3 does (`ExpressiveMotionTokens`).
//! Colour comes from the seed through `scheme`. `document` projects all of it
//! onto the shell's theme contract, slot by slot, each naming the token it
//! takes; a slot Material has no token for keeps its contract default.
use super::hct::{self, Argb, TonalPalette};
use super::scheme::{Role, Scheme};
use crate::theme_contract;
use serde_json::{json, Map, Value};

/// A style of the type scale, in pixels at Material's 1sp = 1px.
#[derive(Clone, Copy)]
pub struct Type {
    pub name: &'static str,
    pub size: u32,
    pub line_height: u32,
    pub tracking: f64,
    pub weight: &'static str,
    /// The Expressive scale's emphasized weight for the same style.
    pub emphasized: &'static str,
}
const fn style(
    name: &'static str,
    size: u32,
    line_height: u32,
    tracking: f64,
    weight: &'static str,
    emphasized: &'static str,
) -> Type {
    Type {
        name,
        size,
        line_height,
        tracking,
        weight,
        emphasized,
    }
}
pub const TYPE_SCALE: [Type; 15] = [
    style("displayLarge", 57, 64, -0.25, "Normal", "Medium"),
    style("displayMedium", 45, 52, 0.0, "Normal", "Medium"),
    style("displaySmall", 36, 44, 0.0, "Normal", "Medium"),
    style("headlineLarge", 32, 40, 0.0, "Normal", "Medium"),
    style("headlineMedium", 28, 36, 0.0, "Normal", "Medium"),
    style("headlineSmall", 24, 32, 0.0, "Normal", "Medium"),
    style("titleLarge", 22, 28, 0.0, "Normal", "Medium"),
    style("titleMedium", 16, 24, 0.15, "Medium", "Bold"),
    style("titleSmall", 14, 20, 0.1, "Medium", "Bold"),
    style("bodyLarge", 16, 24, 0.5, "Normal", "Medium"),
    style("bodyMedium", 14, 20, 0.25, "Normal", "Medium"),
    style("bodySmall", 12, 16, 0.4, "Normal", "Medium"),
    style("labelLarge", 14, 20, 0.1, "Medium", "Bold"),
    style("labelMedium", 12, 16, 0.5, "Medium", "Bold"),
    style("labelSmall", 11, 16, 0.5, "Medium", "Bold"),
];
fn type_style(name: &str) -> Type {
    *TYPE_SCALE
        .iter()
        .find(|style| style.name == name)
        .expect("a type scale style")
}

/// The corner scale, in pixels. `full` is a pill: half the shape's height.
pub const SHAPE: [(&str, u32); 9] = [
    ("none", 0),
    ("extraSmall", 4),
    ("small", 8),
    ("medium", 12),
    ("large", 16),
    ("largeIncreased", 20),
    ("extraLarge", 28),
    ("extraLargeIncreased", 32),
    ("extraExtraLarge", 48),
];
fn corner(name: &str) -> u32 {
    SHAPE
        .iter()
        .find(|(token, _)| *token == name)
        .map(|(_, value)| *value)
        .expect("a corner token")
}

/// Elevation: each level's height, and the surface role it is drawn in, since
/// Material 3 expresses height through tone before shadow.
pub const ELEVATION: [(u32, Role); 6] = [
    (0, Role::Surface),
    (1, Role::SurfaceContainerLow),
    (3, Role::SurfaceContainer),
    (6, Role::SurfaceContainerHigh),
    (8, Role::SurfaceContainerHighest),
    (12, Role::SurfaceContainerHighest),
];

/// State layer opacities, laid over a container in its content colour.
pub const STATE_HOVER: f64 = 0.08;
pub const STATE_FOCUS: f64 = 0.10;
pub const STATE_PRESSED: f64 = 0.10;
pub const STATE_DRAGGED: f64 = 0.16;
/// Disabled content; a disabled container is 0.12 of its content colour.
pub const STATE_DISABLED: f64 = 0.38;
pub const STATE_DISABLED_CONTAINER: f64 = 0.12;
/// The focus indicator's stroke and its offset outside the control.
pub const FOCUS_THICKNESS: u32 = 3;
pub const FOCUS_OFFSET: u32 = 2;

/// The Expressive motion scheme's springs: damping ratio and stiffness.
/// Spatial springs move things and may overshoot; effects springs change
/// colour and opacity and never do.
pub const SPRINGS: [(&str, f64, f64); 6] = [
    ("fastSpatial", 0.6, 800.0),
    ("defaultSpatial", 0.8, 380.0),
    ("slowSpatial", 0.8, 200.0),
    ("fastEffects", 1.0, 3800.0),
    ("defaultEffects", 1.0, 1600.0),
    ("slowEffects", 1.0, 800.0),
];
/// Durations for motion that is timed rather than sprung, in milliseconds.
pub const DURATIONS: [(&str, u32); 16] = [
    ("short1", 50),
    ("short2", 100),
    ("short3", 150),
    ("short4", 200),
    ("medium1", 250),
    ("medium2", 300),
    ("medium3", 350),
    ("medium4", 400),
    ("long1", 450),
    ("long2", 500),
    ("long3", 550),
    ("long4", 600),
    ("extraLong1", 700),
    ("extraLong2", 800),
    ("extraLong3", 900),
    ("extraLong4", 1000),
];
fn duration(name: &str) -> u32 {
    DURATIONS
        .iter()
        .find(|(token, _)| *token == name)
        .map(|(_, value)| *value)
        .expect("a duration token")
}
/// Easing curves as cubic Bézier control points.
pub const EASINGS: [(&str, [f64; 4]); 7] = [
    ("standard", [0.2, 0.0, 0.0, 1.0]),
    ("standardAccelerate", [0.3, 0.0, 1.0, 1.0]),
    ("standardDecelerate", [0.0, 0.0, 0.0, 1.0]),
    ("emphasizedAccelerate", [0.3, 0.0, 0.8, 0.15]),
    ("emphasizedDecelerate", [0.05, 0.7, 0.1, 1.0]),
    ("legacy", [0.4, 0.0, 0.2, 1.0]),
    ("linear", [0.0, 0.0, 1.0, 1.0]),
];
/// A curve as the contract's `curve` type and Qt's `easing.bezierCurve`
/// read it: one segment ending at (1, 1).
fn curve(name: &str) -> Value {
    let [x1, y1, x2, y2] = EASINGS
        .iter()
        .find(|(token, _)| *token == name)
        .map(|(_, points)| *points)
        .expect("an easing token");
    json!([x1, y1, x2, y2, 1.0, 1.0])
}

/// The design colours Material's custom colours are harmonized from: the
/// Material Design palette's Orange 700, Yellow 700, Green 600, Cyan 600 and
/// Brown 500. Each is turned toward the seed, so the terminal and the status
/// colours belong to the scheme without losing what they mean.
pub const CUSTOM: [(&str, Argb); 5] = [
    ("orange", 0xfff5_7c00),
    ("yellow", 0xfffb_c02d),
    ("green", 0xff43_a047),
    ("cyan", 0xff00_acc1),
    ("brown", 0xff79_5548),
];
/// A custom colour's `color` and `colorContainer` roles in this scheme.
fn custom(scheme: &Scheme, name: &str) -> (Argb, Argb) {
    let design = CUSTOM
        .iter()
        .find(|(token, _)| *token == name)
        .map(|(_, argb)| *argb)
        .expect("a custom colour");
    let palette = TonalPalette::from_argb(hct::harmonize(design, scheme.seed));
    if scheme.dark {
        (palette.tone(80.0), palette.tone(30.0))
    } else {
        (palette.tone(40.0), palette.tone(90.0))
    }
}

/// The nearest step of Material's 4dp grid, a half step rounding up.
fn on_grid(value: i64) -> i64 {
    (value + 2) / 4 * 4
}

fn opaque(argb: Argb) -> Value {
    Value::String(hct::hex(argb))
}
/// `argb` at `opacity`, as Qt reads `#aarrggbb`.
fn alpha(argb: Argb, opacity: f64) -> Value {
    let a = (opacity * 255.0 + 0.5).floor() as u32;
    Value::String(format!("#{a:02x}{:06x}", argb & 0x00ff_ffff))
}
/// `tint` laid over `base` at `opacity`: a state layer, flattened.
fn layer(base: Argb, tint: Argb, opacity: f64) -> Value {
    let mix = |c: fn(Argb) -> u8| {
        let (from, to) = (f64::from(c(base)), f64::from(c(tint)));
        (from + (to - from) * opacity + 0.5).floor() as u8
    };
    opaque(hct::argb_from_rgb(
        mix(hct::red),
        mix(hct::green),
        mix(hct::blue),
    ))
}

/// The source a Material theme document records: what `document` grows it
/// from, so registration can grow it again and compare.
pub fn source(seed: Argb, contrast: f64) -> Value {
    json!({"seed": hct::hex(seed), "contrast": contrast})
}

/// Every token, for `seele-theme material tokens`: both schemes' colour
/// roles and the reference scales.
pub fn tokens(seed: Argb, contrast: f64) -> Value {
    let schemes: Map<String, Value> = [false, true]
        .into_iter()
        .map(|dark| {
            let scheme = Scheme::new(seed, dark, contrast);
            let mut roles: Map<String, Value> = Role::ALL
                .iter()
                .map(|role| (role.name().to_owned(), opaque(scheme.argb(*role))))
                .collect();
            for (name, _) in CUSTOM {
                let (color, container) = custom(&scheme, name);
                roles.insert(name.to_owned(), opaque(color));
                roles.insert(format!("{name}_container"), opaque(container));
            }
            (
                if dark { "dark" } else { "light" }.to_owned(),
                Value::Object(roles),
            )
        })
        .collect();
    json!({
        "source": source(seed, contrast),
        "color": schemes,
        "typeScale": TYPE_SCALE.iter().map(|style| json!({
            "name": style.name, "size": style.size, "lineHeight": style.line_height,
            "tracking": style.tracking, "weight": style.weight, "emphasizedWeight": style.emphasized,
        })).collect::<Vec<_>>(),
        "shape": SHAPE.iter().map(|(name, value)| (name.to_string(), json!(value))).collect::<Map<_, _>>(),
        "elevation": ELEVATION.iter().enumerate().map(|(level, (height, role))| json!({
            "level": level, "height": height, "surface": role.name(),
        })).collect::<Vec<_>>(),
        "state": {
            "hover": STATE_HOVER, "focus": STATE_FOCUS, "pressed": STATE_PRESSED,
            "dragged": STATE_DRAGGED, "disabled": STATE_DISABLED,
            "disabledContainer": STATE_DISABLED_CONTAINER,
            "focusThickness": FOCUS_THICKNESS, "focusOffset": FOCUS_OFFSET,
        },
        "motion": {
            "springs": SPRINGS.iter().map(|(name, damping, stiffness)| (name.to_string(), json!({
                "damping": damping, "stiffness": stiffness,
            }))).collect::<Map<_, _>>(),
            "durations": DURATIONS.iter().map(|(name, value)| (name.to_string(), json!(value))).collect::<Map<_, _>>(),
            "easings": EASINGS.iter().map(|(name, points)| (name.to_string(), json!(points))).collect::<Map<_, _>>(),
        },
    })
}

/// A theme document of the shell's contract that draws every surface with
/// Material 3 Expressive's tokens: the scheme `seed` grows in `mode`, the
/// type, corner and spacing scales, tonal elevation, state layers and timed
/// motion. Material defines no typeface for a brand to keep, so the document
/// names none and the configured one stands.
pub fn document(seed: Argb, dark: bool, contrast: f64, id: &str, name: &str) -> Value {
    let scheme = Scheme::new(seed, dark, contrast);
    let c = |role| scheme.argb(role);
    let (green, green_container) = custom(&scheme, "green");
    let (yellow, _) = custom(&scheme, "yellow");
    // Base16 calls this the darkest step; in Material that is the dim surface
    // in a light scheme and the lowest container in a dark one.
    let crust = if scheme.tone(Role::SurfaceDim) < scheme.tone(Role::SurfaceContainerLowest) {
        Role::SurfaceDim
    } else {
        Role::SurfaceContainerLowest
    };
    // Base16's last foreground step: white in a dark scheme, black in a light.
    let extreme = if dark { 0xffff_ffff } else { 0xff00_0000 };
    let mut document = Map::new();
    let mut set = |key: &str, value: Value| {
        document.insert(key.to_owned(), value);
    };
    set("id", json!(id));
    set("name", json!(name));
    set("mode", json!(if dark { "dark" } else { "light" }));
    set("material", source(seed, contrast));

    // Palette: the eleven roles, from the scheme's colour roles.
    set("base", opaque(c(Role::Surface)));
    set("mantle", opaque(c(Role::SurfaceContainer)));
    set("crust", opaque(c(crust)));
    set("surface", opaque(c(Role::SurfaceContainerHigh)));
    set("overlay", opaque(c(Role::Outline)));
    set("text", opaque(c(Role::OnSurface)));
    set("subtext", opaque(c(Role::OnSurfaceVariant)));
    set("accent", opaque(c(Role::Primary)));
    set("red", opaque(c(Role::Error)));
    set("green", opaque(green));
    set("yellow", opaque(yellow));

    // Typography: each step of the shell's ramp takes the style of the scale
    // that plays its role. Icons take Material's icon sizes.
    for (slot, style) in [
        ("textMicro", "labelSmall"),
        ("textCaption", "bodySmall"),
        ("textLabel", "labelSmall"),
        ("textBody", "bodyMedium"),
        ("textStrong", "titleSmall"),
        ("textLead", "titleMedium"),
        ("textSubhead", "titleMedium"),
        ("textTitle", "titleLarge"),
        ("textDisplay", "headlineSmall"),
        ("textCode", "headlineLarge"),
        ("textHero", "displaySmall"),
    ] {
        set(slot, json!(type_style(style).size));
    }
    set("textIcon", json!(18)); // a button's and a chip's icon
    set("textCard", json!(24)); // the standard icon
    let body = type_style("bodyMedium");
    set("weightRegular", json!(body.weight));
    set("weightMedium", json!(type_style("labelLarge").weight));
    set("weightStrong", json!(type_style("labelLarge").emphasized));
    set("weightLight", json!(type_style("displaySmall").weight));
    set("trackingLabel", json!(type_style("labelLarge").tracking));
    set(
        "trackingCode",
        json!(type_style("headlineLarge").tracking as i64),
    );
    let leading = f64::from(body.line_height) / f64::from(body.size);
    set("lineHeightProse", json!(leading));
    set("lineBoxRatio", json!(leading));

    // Shape: the corner scale, outline and focus indicator strokes.
    set("radius", json!(corner("large")));
    set("radiusSmall", json!(corner("small")));
    set("hairline", json!(1));
    set("strokeStrong", json!(FOCUS_THICKNESS));
    set("meetingCellRadius", json!(corner("extraSmall")));
    set("meetingSelectionRadius", json!(corner("small")));

    // Spacing: the shell's own ramp, each step on Material's 4dp grid, so a
    // dense surface keeps its rhythm and every gap is a multiple of four.
    for (slot, value) in theme_contract::int_defaults("spacing").expect("the contract's spacing") {
        set(&slot, json!(on_grid(value)));
    }

    // Metrics: the heights of Material's components.
    for (slot, value) in [
        ("barHeight", 40),
        ("barItemHeight", 32), // extra-small buttons
        ("panelHeaderHeight", 40),
        ("chipHeight", 32),
        ("controlHeight", 40), // small buttons and fields
        // A row leads with a small icon button, at a small control's height.
        // The Control Center sets three of them beside its two levels, so it
        // stays the height the shell's levels are measured against.
        ("rowHeight", 40),
        ("detailRowHeight", 72), // a two-line list item
        ("controlTileHeight", 72),
        ("knobSize", 40), // a small icon button
        ("switchWidth", 52),
        ("switchHeight", 32),
        ("switchKnobInset", 4), // the selected handle, 24 of 32
        ("rowIconSize", 24),
        ("cardGlyphWidth", 24),
        ("notificationRowHeight", 72),
        ("meterHeight", 4), // linear progress
        ("tipPadding", 8),
        ("tipHeight", 24), // a plain tooltip
        ("actionButtonHeight", 40),
        ("dialogButtonHeight", 40),
        ("speedtestButtonHeight", 32),
        ("bluetoothRowButtonHeight", 32),
        ("bluetoothForgetSize", 32),
        ("vpnAppButtonSize", 40),
        ("deviceRowHeight", 48), // a menu item
        ("trayMenuRowHeight", 48),
        ("trayMenuSeparatorHeight", 17), // a divider with its 8dp on either side
        ("trayMenuIconSize", 24),
        ("trayCloseSize", 40),
        ("trayHideHeight", 32),
        ("barIconSize", 18),
        // The bar's elided labels -- the next event, the media title -- give
        // back the width Material's larger type takes everywhere else on the
        // bar, so it still fits a 1920-pixel output with every indicator.
        ("barLabelMaximumWidth", 120),
        ("barWindowIconSize", 18),
        ("barIconItemWidth", 32),
        ("barDotSize", 6), // the small badge
        ("privacyDotSize", 6),
        ("barBadgeHeight", 16), // the large badge
        ("osdIconSize", 24),
        ("osdMeterHeight", 4),
        ("gaugeTrackWidth", 4), // circular progress
    ] {
        set(slot, json!(value));
    }

    // Material: opaque tonal surfaces with no texture, wash or lit edge.
    // Height is tone: the bar on the surface, a panel one container up, a
    // card and a row above it, a well in the filled field's container, and a
    // control floating over a row on the brightest surface.
    set("grainOpacity", json!(0.0));
    set("washCrownOpacity", json!(0.0));
    set("washFadeOpacity", json!(0.0));
    set("washFootOpacity", json!(0.0));
    set("panelColor", opaque(c(Role::SurfaceContainer)));
    set("panelBorder", alpha(c(Role::SurfaceContainer), 0.0));
    set("edgeLight", alpha(c(Role::OnSurface), 0.0));
    set("edgeCrown", alpha(c(Role::OnSurface), 0.0));
    set("barStripColor", opaque(c(Role::Surface)));
    set("barRuleColor", alpha(c(Role::Surface), 0.0));
    set("cardColor", opaque(c(Role::SurfaceContainerHigh)));
    set("cardBorder", alpha(c(Role::SurfaceContainerHigh), 0.0));
    set("rowColor", opaque(c(Role::SurfaceContainerHighest)));
    set("wellColor", opaque(c(Role::SurfaceContainerHighest)));
    set("wellBorder", alpha(c(Role::OutlineVariant), 0.0));
    set("floatColor", opaque(c(Role::SurfaceBright)));
    set("separatorColor", opaque(c(Role::OutlineVariant)));
    set("swatchBorder", opaque(c(Role::OutlineVariant)));
    set("themesSliceBorder", opaque(c(Role::OutlineVariant)));
    // State layers, in the content colour they report.
    set("hoverColor", alpha(c(Role::OnSurface), STATE_HOVER));
    set("quietWash", alpha(c(Role::OnSurface), STATE_HOVER));
    set("pressColor", alpha(c(Role::Primary), STATE_PRESSED));
    set("disabledOpacity", json!(STATE_DISABLED));
    set("unavailableOpacity", json!(STATE_DISABLED));
    set("readOnlyOpacity", json!(STATE_DISABLED));
    // Selection and activity in the containers Material keeps for them.
    set("selectedColor", opaque(c(Role::SecondaryContainer)));
    set("activeTint", opaque(c(Role::PrimaryContainer)));
    set("primaryControlColor", opaque(c(Role::PrimaryContainer)));
    set("knobColor", opaque(c(Role::SecondaryContainer)));
    set("badgeColor", opaque(c(Role::SecondaryContainer)));
    set("markWell", opaque(c(Role::SecondaryContainer)));
    set("markWellBorder", alpha(c(Role::SecondaryContainer), 0.0));
    set("workspaceHoverColor", opaque(c(Role::PrimaryContainer)));
    set(
        "workspaceOccupiedColor",
        opaque(c(Role::SecondaryContainer)),
    );
    set(
        "workspaceEmptyColor",
        opaque(c(Role::SurfaceContainerHighest)),
    );
    set("workspaceEmptyText", opaque(c(Role::OnSurfaceVariant)));
    set("mutedBadgeColor", opaque(c(Role::SurfaceContainerHighest)));
    // A level's fill carries its label and value in the surface's content
    // colour, so it is the primary container rather than the primary.
    set("fillColor", opaque(c(Role::PrimaryContainer)));
    set("accentRing", opaque(c(Role::Primary)));
    set("selectedRing", opaque(c(Role::Primary)));
    set("accentEdge", opaque(c(Role::Primary)));
    // Danger and success in their containers, with the state layers a
    // hovered or pressed control lays over its container.
    let (error_container, on_error_container) =
        (c(Role::ErrorContainer), c(Role::OnErrorContainer));
    set("fillDanger", opaque(error_container));
    set("dangerBorder", opaque(c(Role::Error)));
    set("dangerTint", opaque(error_container));
    set(
        "dangerColor",
        layer(error_container, on_error_container, STATE_HOVER),
    );
    set(
        "dangerPress",
        layer(error_container, on_error_container, STATE_PRESSED),
    );
    set("successColor", opaque(green_container));
    set("warningBorder", opaque(yellow));

    // Motion: Material's timed durations and easing curves.
    set("durationFast", json!(duration("short3")));
    set("durationNormal", json!(duration("medium2")));
    set("durationGlance", json!(duration("long2")));
    set("durationSettle", json!(duration("long1")));
    set("durationGauge", json!(duration("long2")));
    set("durationPulse", json!(duration("extraLong3")));
    set("durationPulseFast", json!(duration("long4")));
    for (slot, token) in [
        ("easingStandard", "standard"),
        ("easingExit", "emphasizedAccelerate"),
        ("easingPulse", "legacy"),
    ] {
        set(slot, json!("BezierSpline"));
        set(&format!("{slot}Curve"), curve(token));
    }

    // Base16, for the applications themed from it: the surfaces and their
    // content in Base16's order, then the accents. Blue is the primary, as a
    // preset's accent is its blue; magenta the tertiary; the rest harmonized.
    let base16 = [
        c(Role::Surface),
        c(Role::SurfaceContainer),
        c(Role::SurfaceContainerHighest),
        c(Role::Outline),
        c(Role::OnSurfaceVariant),
        c(Role::OnSurface),
        c(Role::InverseSurface),
        extreme,
        c(Role::Error),
        custom(&scheme, "orange").0,
        yellow,
        green,
        custom(&scheme, "cyan").0,
        c(Role::Primary),
        c(Role::Tertiary),
        custom(&scheme, "brown").0,
    ];
    set(
        "palette",
        Value::Object(
            base16
                .iter()
                .enumerate()
                .map(|(index, argb)| (format!("base0{index:X}"), opaque(*argb)))
                .collect(),
        ),
    );
    Value::Object(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn both() -> [Value; 2] {
        [
            document(0xff67_50a4, false, 0.0, "material-light", "Material Light"),
            document(0xff67_50a4, true, 0.0, "material-dark", "Material Dark"),
        ]
    }

    #[test]
    fn each_document_is_a_valid_theme_and_grows_back_from_its_seed() {
        for document in both() {
            assert_eq!(
                theme_contract::violations(&document).unwrap(),
                Vec::<String>::new()
            );
            let text = serde_json::to_string_pretty(&document).unwrap();
            let read: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(super::super::drift(&read), Vec::<String>::new());
        }
    }

    #[test]
    fn light_and_dark_share_every_token_but_colour() {
        let [light, dark] = both();
        let colour = |value: &Value| value.as_str().is_some_and(|text| text.starts_with('#'));
        for (key, value) in light.as_object().unwrap() {
            if ["id", "name", "mode", "palette"].contains(&key.as_str()) || colour(value) {
                continue;
            }
            assert_eq!(Some(value), dark.get(key), "{key}");
        }
        assert_eq!(
            light.as_object().unwrap().len(),
            dark.as_object().unwrap().len()
        );
    }

    #[test]
    fn the_shell_takes_material_values() {
        let [light, dark] = both();
        for document in [&light, &dark] {
            assert_eq!(document["radius"], json!(16));
            assert_eq!(document["textBody"], json!(14));
            assert_eq!(document["rowHeight"], json!(40));
            assert_eq!(document["spaceSmall"], json!(8));
            assert_eq!(document["panelGap"], json!(4));
            assert_eq!(document["dialogPadding"], json!(24));
            assert_eq!(document["hoverColor"].as_str().unwrap().len(), 9);
            assert_eq!(
                document["easingStandardCurve"],
                json!([0.2, 0.0, 0.0, 1.0, 1.0, 1.0])
            );
            assert_eq!(document["grainOpacity"], json!(0.0));
        }
        // Tonal elevation: each layer a step further from the surface.
        let tone = |document: &Value, key: &str| {
            hct::Hct::from_argb(hct::parse(document[key].as_str().unwrap()).unwrap()).tone
        };
        let ramp = ["base", "panelColor", "cardColor", "rowColor"];
        for pair in ramp.windows(2) {
            assert!(tone(&dark, pair[0]) < tone(&dark, pair[1]), "{pair:?}");
            assert!(tone(&light, pair[0]) > tone(&light, pair[1]), "{pair:?}");
        }
        assert_eq!(light["palette"]["base0D"], light["accent"]);
    }

    #[test]
    fn the_tokens_list_both_schemes_and_every_scale() {
        let tokens = tokens(0xff67_50a4, 0.0);
        for mode in ["light", "dark"] {
            let roles = tokens["color"][mode].as_object().unwrap();
            assert_eq!(roles.len(), Role::ALL.len() + 2 * CUSTOM.len());
        }
        assert_eq!(tokens["typeScale"].as_array().unwrap().len(), 15);
        assert_eq!(
            tokens["motion"]["springs"]["fastSpatial"]["damping"],
            json!(0.6)
        );
        assert_eq!(tokens["shape"]["extraLarge"], json!(28));
        assert_eq!(
            tokens["elevation"][2]["surface"],
            json!("surface_container")
        );
    }
}
