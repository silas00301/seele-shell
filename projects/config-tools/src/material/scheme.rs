//! Material 3 Expressive dynamic colour: the colour roles of a light or dark
//! scheme grown from one seed colour.
//!
//! This is Material Color Utilities' `SchemeExpressive` under the 2025 colour
//! spec (`color_spec_2025.ts`, the spec Material 3 Expressive introduced, and
//! the one its 2026 spec still falls back to for this variant), on the phone
//! platform: the watch platform is for round black screens, and phone is the
//! one Material ships for every other surface. Only that variant and platform
//! are ported, so each role reads as a straight line of its definition there.
use super::hct::{self, Argb, Hct, TonalPalette};
use std::{cell::RefCell, collections::HashMap};

macro_rules! roles {
    ($($role:ident => $name:literal,)*) => {
        /// A colour role, in Material's own order.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Role { $($role,)* }
        impl Role {
            pub const ALL: &'static [Role] = &[$(Role::$role,)*];
            /// The role's name as Material spells it, such as `on_primary`.
            pub fn name(self) -> &'static str {
                match self { $(Role::$role => $name,)* }
            }
        }
    };
}
roles! {
    Background => "background",
    OnBackground => "on_background",
    Surface => "surface",
    SurfaceDim => "surface_dim",
    SurfaceBright => "surface_bright",
    SurfaceContainerLowest => "surface_container_lowest",
    SurfaceContainerLow => "surface_container_low",
    SurfaceContainer => "surface_container",
    SurfaceContainerHigh => "surface_container_high",
    SurfaceContainerHighest => "surface_container_highest",
    OnSurface => "on_surface",
    SurfaceVariant => "surface_variant",
    OnSurfaceVariant => "on_surface_variant",
    InverseSurface => "inverse_surface",
    InverseOnSurface => "inverse_on_surface",
    Outline => "outline",
    OutlineVariant => "outline_variant",
    Shadow => "shadow",
    Scrim => "scrim",
    SurfaceTint => "surface_tint",
    Primary => "primary",
    PrimaryDim => "primary_dim",
    OnPrimary => "on_primary",
    PrimaryContainer => "primary_container",
    OnPrimaryContainer => "on_primary_container",
    InversePrimary => "inverse_primary",
    PrimaryFixed => "primary_fixed",
    PrimaryFixedDim => "primary_fixed_dim",
    OnPrimaryFixed => "on_primary_fixed",
    OnPrimaryFixedVariant => "on_primary_fixed_variant",
    Secondary => "secondary",
    SecondaryDim => "secondary_dim",
    OnSecondary => "on_secondary",
    SecondaryContainer => "secondary_container",
    OnSecondaryContainer => "on_secondary_container",
    SecondaryFixed => "secondary_fixed",
    SecondaryFixedDim => "secondary_fixed_dim",
    OnSecondaryFixed => "on_secondary_fixed",
    OnSecondaryFixedVariant => "on_secondary_fixed_variant",
    Tertiary => "tertiary",
    TertiaryDim => "tertiary_dim",
    OnTertiary => "on_tertiary",
    TertiaryContainer => "tertiary_container",
    OnTertiaryContainer => "on_tertiary_container",
    TertiaryFixed => "tertiary_fixed",
    TertiaryFixedDim => "tertiary_fixed_dim",
    OnTertiaryFixed => "on_tertiary_fixed",
    OnTertiaryFixedVariant => "on_tertiary_fixed_variant",
    Error => "error",
    ErrorDim => "error_dim",
    OnError => "on_error",
    ErrorContainer => "error_container",
    OnErrorContainer => "on_error_container",
}

#[derive(Clone, Copy)]
enum Palette {
    Primary,
    Secondary,
    Tertiary,
    Neutral,
    Error,
}

/// The contrast a role keeps against its background at each contrast level.
#[derive(Clone, Copy)]
struct Curve {
    low: f64,
    normal: f64,
    medium: f64,
    high: f64,
}
impl Curve {
    /// The curve 2025 roles are written in, by their default ratio.
    fn of(normal: f64) -> Curve {
        let (low, medium, high) = match normal {
            1.5 => (1.5, 3.0, 5.5),
            3.0 => (3.0, 4.5, 7.0),
            4.5 => (4.5, 7.0, 11.0),
            6.0 => (6.0, 7.0, 11.0),
            7.0 => (7.0, 11.0, 21.0),
            9.0 => (9.0, 11.0, 21.0),
            11.0 => (11.0, 21.0, 21.0),
            _ => (normal, 7.0, 21.0),
        };
        Curve {
            low,
            normal,
            medium,
            high,
        }
    }
    fn at(self, level: f64) -> f64 {
        let lerp = |a: f64, b: f64, t: f64| (1.0 - t) * a + t * b;
        if level <= -1.0 {
            self.low
        } else if level < 0.0 {
            lerp(self.low, self.normal, level + 1.0)
        } else if level < 0.5 {
            lerp(self.normal, self.medium, level / 0.5)
        } else if level < 1.0 {
            lerp(self.medium, self.high, (level - 0.5) / 0.5)
        } else {
            self.high
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Polarity {
    /// `a` is darker than `b`.
    Darker,
    /// `a` is further from the surface than `b`: lighter in a light scheme.
    RelativeLighter,
}
#[derive(Clone, Copy, PartialEq)]
enum Constraint {
    Exact,
    Farther,
}
/// Two roles held a tone `delta` apart, such as a container and its accent.
#[derive(Clone, Copy)]
struct Pair {
    a: Role,
    b: Role,
    delta: f64,
    polarity: Polarity,
    constraint: Constraint,
}

/// A role's definition, apart from its starting tone.
struct Spec {
    palette: Palette,
    is_background: bool,
    chroma: f64,
    background: Option<Role>,
    curve: Option<Curve>,
    pair: Option<Pair>,
}
impl Spec {
    fn on(palette: Palette) -> Spec {
        Spec {
            palette,
            is_background: false,
            chroma: 1.0,
            background: None,
            curve: None,
            pair: None,
        }
    }
    fn ground(mut self) -> Spec {
        self.is_background = true;
        self
    }
    fn chroma(mut self, multiplier: f64) -> Spec {
        self.chroma = multiplier;
        self
    }
    fn against(mut self, background: Role, curve: Option<Curve>) -> Spec {
        self.background = Some(background);
        self.curve = curve;
        self
    }
    fn paired(mut self, a: Role, b: Role, polarity: Polarity, constraint: Constraint) -> Spec {
        self.pair = Some(Pair {
            a,
            b,
            delta: 5.0,
            polarity,
            constraint,
        });
        self
    }
}

fn piecewise(source: f64, breakpoints: &[f64], hues: &[f64]) -> f64 {
    let size = (breakpoints.len() - 1).min(hues.len());
    for i in 0..size {
        if source >= breakpoints[i] && source < breakpoints[i + 1] {
            return hct::sanitize_degrees(hues[i]);
        }
    }
    source
}
fn rotated(source: f64, breakpoints: &[f64], rotations: &[f64]) -> f64 {
    hct::sanitize_degrees(source + piecewise(source, breakpoints, rotations))
}
const ACCENT_BREAKS: [f64; 9] = [0.0, 105.0, 140.0, 204.0, 253.0, 278.0, 300.0, 333.0, 360.0];

/// The tone nearest `tone` at which `hue` reaches the most of `chroma`.
fn best_tone(hue: f64, chroma: f64, mut tone: f64, decreasing: bool) -> f64 {
    let mut answer = tone;
    let mut best = Hct::new(hue, chroma, answer);
    while best.chroma < chroma {
        if !(0.0..=100.0).contains(&tone) {
            break;
        }
        tone += if decreasing { -1.0 } else { 1.0 };
        let candidate = Hct::new(hue, chroma, tone);
        if best.chroma < candidate.chroma {
            best = candidate;
            answer = tone;
        }
    }
    answer
}
/// The lightest tone of the palette at its full chroma, within bounds.
fn max_chroma_tone(palette: TonalPalette, lower: f64, upper: f64) -> f64 {
    hct::clamp(
        lower,
        upper,
        best_tone(palette.hue, palette.chroma, 100.0, true),
    )
}
/// The darkest tone of the palette at its full chroma, within bounds.
fn min_chroma_tone(palette: TonalPalette, lower: f64, upper: f64) -> f64 {
    hct::clamp(
        lower,
        upper,
        best_tone(palette.hue, palette.chroma, 0.0, false),
    )
}

fn prefers_light_foreground(tone: f64) -> bool {
    (tone + 0.5).floor() < 60.0
}
/// A tone with at least `ratio` against `background`, on the side Material
/// prefers for it.
fn foreground_tone(background: f64, ratio: f64) -> f64 {
    let lighter = hct::lighter_unsafe(background, ratio);
    let darker = hct::darker_unsafe(background, ratio);
    let lighter_ratio = hct::ratio_of_tones(lighter, background);
    let darker_ratio = hct::ratio_of_tones(darker, background);
    if prefers_light_foreground(background) {
        let negligible = (lighter_ratio - darker_ratio).abs() < 0.1
            && lighter_ratio < ratio
            && darker_ratio < ratio;
        if lighter_ratio >= ratio || lighter_ratio >= darker_ratio || negligible {
            lighter
        } else {
            darker
        }
    } else if darker_ratio >= ratio || darker_ratio >= lighter_ratio {
        darker
    } else {
        lighter
    }
}

/// One light or dark Material 3 Expressive scheme.
pub struct Scheme {
    pub seed: Argb,
    pub dark: bool,
    /// From -1, reduced, through 0, Material's default, to 1, high contrast.
    pub contrast: f64,
    primary: TonalPalette,
    secondary: TonalPalette,
    tertiary: TonalPalette,
    neutral: TonalPalette,
    error: TonalPalette,
    tones: RefCell<HashMap<Role, f64>>,
}

impl Scheme {
    pub fn new(seed: Argb, dark: bool, contrast: f64) -> Scheme {
        let source = Hct::from_argb(seed);
        let hue = source.hue;
        let neutral_hue = rotated(
            hue,
            &[0.0, 71.0, 124.0, 253.0, 278.0, 300.0, 360.0],
            &[10.0, 0.0, 10.0, 0.0, 10.0, 0.0],
        );
        let neutral_chroma = if dark {
            if Hct::is_yellow(neutral_hue) {
                6.0
            } else {
                14.0
            }
        } else {
            18.0
        };
        let error_hue = piecewise(
            hue,
            &[0.0, 3.0, 13.0, 23.0, 33.0, 43.0, 153.0, 273.0, 360.0],
            &[12.0, 22.0, 32.0, 12.0, 22.0, 32.0, 22.0, 12.0],
        );
        Scheme {
            seed,
            dark,
            contrast,
            primary: TonalPalette::new(hue, if dark { 36.0 } else { 48.0 }),
            secondary: TonalPalette::new(
                rotated(
                    hue,
                    &ACCENT_BREAKS,
                    &[-160.0, 155.0, -100.0, 96.0, -96.0, -156.0, -165.0, -160.0],
                ),
                if dark { 16.0 } else { 24.0 },
            ),
            tertiary: TonalPalette::new(
                rotated(
                    hue,
                    &ACCENT_BREAKS,
                    &[-165.0, 160.0, -105.0, 101.0, -101.0, -160.0, -170.0, -165.0],
                ),
                48.0,
            ),
            // Expressive's 2025 roles take their variant colours from this
            // palette at more chroma, so its neutral variant palette is unused.
            neutral: TonalPalette::new(neutral_hue, neutral_chroma),
            error: TonalPalette::new(error_hue, 64.0),
            tones: RefCell::new(HashMap::new()),
        }
    }

    /// The scheme a fixed role is measured in: this one's palettes, as a light
    /// scheme at the default contrast. Fixed roles keep one tone in light and
    /// dark, and Material takes it from the light container of the palettes
    /// actually in use, which in a dark scheme are the dark ones.
    fn fixed(&self) -> Scheme {
        Scheme {
            dark: false,
            contrast: 0.0,
            tones: RefCell::new(HashMap::new()),
            ..*self
        }
    }

    fn palette(&self, palette: Palette) -> TonalPalette {
        match palette {
            Palette::Primary => self.primary,
            Palette::Secondary => self.secondary,
            Palette::Tertiary => self.tertiary,
            Palette::Neutral => self.neutral,
            Palette::Error => self.error,
        }
    }
    /// The surface every accent is measured against: the dimmest in a light
    /// scheme, the brightest in a dark one.
    fn highest_surface(&self) -> Role {
        if self.dark {
            Role::SurfaceBright
        } else {
            Role::SurfaceDim
        }
    }
    /// A container's own contrast floor, which only high contrast enforces.
    fn container_curve(&self) -> Option<Curve> {
        (self.contrast > 0.0).then(|| Curve::of(1.5))
    }
    fn yellow_neutral(&self) -> bool {
        Hct::is_yellow(self.neutral.hue)
    }
    /// The extra chroma Expressive gives the text and outlines on a surface.
    fn content_chroma(&self) -> f64 {
        if self.yellow_neutral() {
            if self.dark {
                3.0
            } else {
                2.3
            }
        } else {
            1.6
        }
    }
    fn surface_chroma(&self, yellow: f64, other: f64) -> f64 {
        if self.yellow_neutral() {
            yellow
        } else {
            other
        }
    }

    // Roles Material defines as copies of another, apart from their name.
    fn canonical(role: Role) -> Role {
        match role {
            Role::Background => Role::Surface,
            Role::OnBackground => Role::OnSurface,
            Role::SurfaceVariant => Role::SurfaceContainerHighest,
            Role::SurfaceTint => Role::Primary,
            other => other,
        }
    }

    fn spec(&self, role: Role) -> Spec {
        use Constraint::*;
        use Polarity::*;
        use Role::*;
        let highest = self.highest_surface();
        let curve = |ratio| Some(Curve::of(ratio));
        match Self::canonical(role) {
            Surface | SurfaceContainerLowest | InverseSurface => {
                Spec::on(Palette::Neutral).ground()
            }
            SurfaceDim => Spec::on(Palette::Neutral).ground().chroma(if self.dark {
                1.0
            } else {
                self.surface_chroma(2.7, 1.75)
            }),
            SurfaceBright => Spec::on(Palette::Neutral).ground().chroma(if self.dark {
                self.surface_chroma(2.7, 1.75)
            } else {
                1.0
            }),
            SurfaceContainerLow => Spec::on(Palette::Neutral)
                .ground()
                .chroma(self.surface_chroma(1.3, 1.15)),
            SurfaceContainer => Spec::on(Palette::Neutral)
                .ground()
                .chroma(self.surface_chroma(1.6, 1.3)),
            SurfaceContainerHigh => Spec::on(Palette::Neutral)
                .ground()
                .chroma(self.surface_chroma(1.95, 1.45)),
            SurfaceContainerHighest => Spec::on(Palette::Neutral)
                .ground()
                .chroma(self.surface_chroma(2.3, 1.6)),
            OnSurface => Spec::on(Palette::Neutral)
                .chroma(self.content_chroma())
                .against(highest, curve(if self.dark { 11.0 } else { 9.0 })),
            OnSurfaceVariant => Spec::on(Palette::Neutral)
                .chroma(self.content_chroma())
                .against(highest, curve(if self.dark { 6.0 } else { 4.5 })),
            Outline => Spec::on(Palette::Neutral)
                .chroma(self.content_chroma())
                .against(highest, curve(3.0)),
            OutlineVariant => Spec::on(Palette::Neutral)
                .chroma(self.content_chroma())
                .against(highest, curve(1.5)),
            InverseOnSurface => Spec::on(Palette::Neutral).against(InverseSurface, curve(7.0)),
            Shadow | Scrim => Spec::on(Palette::Neutral),

            Primary => Spec::on(Palette::Primary)
                .ground()
                .against(highest, curve(4.5))
                .paired(PrimaryContainer, Role::Primary, RelativeLighter, Farther),
            PrimaryDim => Spec::on(Palette::Primary)
                .ground()
                .against(SurfaceContainerHigh, curve(4.5))
                .paired(PrimaryDim, Role::Primary, Darker, Farther),
            OnPrimary => Spec::on(Palette::Primary).against(Role::Primary, curve(6.0)),
            PrimaryContainer => Spec::on(Palette::Primary)
                .ground()
                .against(highest, self.container_curve()),
            OnPrimaryContainer => Spec::on(Palette::Primary).against(PrimaryContainer, curve(6.0)),
            InversePrimary => Spec::on(Palette::Primary).against(InverseSurface, curve(6.0)),
            PrimaryFixed => Spec::on(Palette::Primary)
                .ground()
                .against(highest, self.container_curve()),
            PrimaryFixedDim => Spec::on(Palette::Primary).ground().paired(
                PrimaryFixedDim,
                PrimaryFixed,
                Darker,
                Exact,
            ),
            OnPrimaryFixed => Spec::on(Palette::Primary).against(PrimaryFixedDim, curve(7.0)),
            OnPrimaryFixedVariant => {
                Spec::on(Palette::Primary).against(PrimaryFixedDim, curve(4.5))
            }

            Secondary => Spec::on(Palette::Secondary)
                .ground()
                .against(highest, curve(4.5))
                .paired(
                    SecondaryContainer,
                    Role::Secondary,
                    RelativeLighter,
                    Farther,
                ),
            SecondaryDim => Spec::on(Palette::Secondary)
                .ground()
                .against(SurfaceContainerHigh, curve(4.5))
                .paired(SecondaryDim, Role::Secondary, Darker, Farther),
            OnSecondary => Spec::on(Palette::Secondary).against(Role::Secondary, curve(6.0)),
            SecondaryContainer => Spec::on(Palette::Secondary)
                .ground()
                .against(highest, self.container_curve()),
            OnSecondaryContainer => {
                Spec::on(Palette::Secondary).against(SecondaryContainer, curve(6.0))
            }
            SecondaryFixed => Spec::on(Palette::Secondary)
                .ground()
                .against(highest, self.container_curve()),
            SecondaryFixedDim => Spec::on(Palette::Secondary).ground().paired(
                SecondaryFixedDim,
                SecondaryFixed,
                Darker,
                Exact,
            ),
            OnSecondaryFixed => Spec::on(Palette::Secondary).against(SecondaryFixedDim, curve(7.0)),
            OnSecondaryFixedVariant => {
                Spec::on(Palette::Secondary).against(SecondaryFixedDim, curve(4.5))
            }

            Tertiary => Spec::on(Palette::Tertiary)
                .ground()
                .against(highest, curve(4.5))
                .paired(TertiaryContainer, Role::Tertiary, RelativeLighter, Farther),
            TertiaryDim => Spec::on(Palette::Tertiary)
                .ground()
                .against(SurfaceContainerHigh, curve(4.5))
                .paired(TertiaryDim, Role::Tertiary, Darker, Farther),
            OnTertiary => Spec::on(Palette::Tertiary).against(Role::Tertiary, curve(6.0)),
            TertiaryContainer => Spec::on(Palette::Tertiary)
                .ground()
                .against(highest, self.container_curve()),
            OnTertiaryContainer => {
                Spec::on(Palette::Tertiary).against(TertiaryContainer, curve(6.0))
            }
            TertiaryFixed => Spec::on(Palette::Tertiary)
                .ground()
                .against(highest, self.container_curve()),
            TertiaryFixedDim => Spec::on(Palette::Tertiary).ground().paired(
                TertiaryFixedDim,
                TertiaryFixed,
                Darker,
                Exact,
            ),
            OnTertiaryFixed => Spec::on(Palette::Tertiary).against(TertiaryFixedDim, curve(7.0)),
            OnTertiaryFixedVariant => {
                Spec::on(Palette::Tertiary).against(TertiaryFixedDim, curve(4.5))
            }

            Role::Error => Spec::on(Palette::Error)
                .ground()
                .against(highest, curve(4.5))
                .paired(ErrorContainer, Role::Error, RelativeLighter, Farther),
            ErrorDim => Spec::on(Palette::Error)
                .ground()
                .against(SurfaceContainerHigh, curve(4.5))
                .paired(ErrorDim, Role::Error, Darker, Farther),
            OnError => Spec::on(Palette::Error).against(Role::Error, curve(6.0)),
            ErrorContainer => Spec::on(Palette::Error)
                .ground()
                .against(highest, self.container_curve()),
            OnErrorContainer => Spec::on(Palette::Error).against(ErrorContainer, curve(4.5)),

            Background | OnBackground | SurfaceVariant | SurfaceTint => {
                unreachable!("canonical() maps every copy")
            }
        }
    }

    /// The tone a role starts from, before contrast is enforced.
    fn initial_tone(&self, role: Role) -> f64 {
        use Role::*;
        let dark = self.dark;
        let light_surface = |yellow: f64, other: f64| {
            if self.yellow_neutral() {
                yellow
            } else {
                other
            }
        };
        let cyan = |palette: TonalPalette| Hct::is_cyan(palette.hue);
        match Self::canonical(role) {
            Surface => {
                if dark {
                    4.0
                } else {
                    light_surface(99.0, 98.0)
                }
            }
            SurfaceDim => {
                if dark {
                    4.0
                } else {
                    light_surface(90.0, 87.0)
                }
            }
            SurfaceBright => {
                if dark {
                    18.0
                } else {
                    light_surface(99.0, 98.0)
                }
            }
            SurfaceContainerLowest => {
                if dark {
                    0.0
                } else {
                    100.0
                }
            }
            SurfaceContainerLow => {
                if dark {
                    6.0
                } else {
                    light_surface(98.0, 96.0)
                }
            }
            SurfaceContainer => {
                if dark {
                    9.0
                } else {
                    light_surface(96.0, 94.0)
                }
            }
            SurfaceContainerHigh => {
                if dark {
                    12.0
                } else {
                    light_surface(94.0, 92.0)
                }
            }
            SurfaceContainerHighest => {
                if dark {
                    15.0
                } else {
                    light_surface(92.0, 90.0)
                }
            }
            InverseSurface => {
                if dark {
                    98.0
                } else {
                    4.0
                }
            }
            Shadow | Scrim => 0.0,
            Primary => {
                let upper = if dark {
                    if cyan(self.primary) {
                        88.0
                    } else {
                        98.0
                    }
                } else if Hct::is_yellow(self.primary.hue) {
                    25.0
                } else {
                    98.0
                };
                max_chroma_tone(self.primary, 0.0, upper)
            }
            PrimaryDim | InversePrimary => max_chroma_tone(self.primary, 0.0, 100.0),
            PrimaryContainer => {
                if dark {
                    min_chroma_tone(self.primary, 30.0, 93.0)
                } else {
                    let upper = if cyan(self.primary) { 88.0 } else { 90.0 };
                    max_chroma_tone(self.primary, 78.0, upper)
                }
            }
            PrimaryFixed => self.fixed().tone(PrimaryContainer),
            PrimaryFixedDim => self.tone(PrimaryFixed),
            Secondary => {
                if dark {
                    80.0
                } else {
                    max_chroma_tone(self.secondary, 0.0, 100.0)
                }
            }
            SecondaryDim => max_chroma_tone(self.secondary, 0.0, 90.0),
            SecondaryContainer => {
                if dark {
                    15.0
                } else {
                    max_chroma_tone(self.secondary, 90.0, 95.0)
                }
            }
            SecondaryFixed => self.fixed().tone(SecondaryContainer),
            SecondaryFixedDim => self.tone(SecondaryFixed),
            Tertiary => {
                let upper = if cyan(self.tertiary) {
                    88.0
                } else if dark {
                    98.0
                } else {
                    100.0
                };
                max_chroma_tone(self.tertiary, 0.0, upper)
            }
            TertiaryDim => max_chroma_tone(self.tertiary, 0.0, 100.0),
            TertiaryContainer => {
                let upper = if cyan(self.tertiary) {
                    88.0
                } else if dark {
                    93.0
                } else {
                    100.0
                };
                max_chroma_tone(self.tertiary, 75.0, upper)
            }
            TertiaryFixed => self.fixed().tone(TertiaryContainer),
            TertiaryFixedDim => self.tone(TertiaryFixed),
            Role::Error => {
                if dark {
                    min_chroma_tone(self.error, 0.0, 98.0)
                } else {
                    max_chroma_tone(self.error, 0.0, 100.0)
                }
            }
            ErrorDim => min_chroma_tone(self.error, 0.0, 100.0),
            ErrorContainer => {
                if dark {
                    min_chroma_tone(self.error, 30.0, 93.0)
                } else {
                    max_chroma_tone(self.error, 0.0, 90.0)
                }
            }
            // Every other role starts from its background's tone.
            other => match self.spec(other).background {
                Some(background) => self.tone(background),
                None => 50.0,
            },
        }
    }

    /// Material's phone rule for a role drawn as a background: never in the
    /// 50s, where neither light nor dark content reads well on it.
    fn clamp_background(&self, role: Role, spec: &Spec, tone: f64) -> f64 {
        if spec.is_background && !role.name().ends_with("_fixed_dim") {
            if tone >= 57.0 {
                hct::clamp(65.0, 100.0, tone)
            } else {
                hct::clamp(0.0, 49.0, tone)
            }
        } else {
            tone
        }
    }

    /// The tone of `role` in this scheme, with its contrast enforced.
    pub fn tone(&self, role: Role) -> f64 {
        if let Some(tone) = self.tones.borrow().get(&role) {
            return *tone;
        }
        let tone = self.solve_tone(role);
        self.tones.borrow_mut().insert(role, tone);
        tone
    }

    fn solve_tone(&self, role: Role) -> f64 {
        let spec = self.spec(role);
        let enforce = |tone: f64| match (spec.background, spec.curve) {
            (Some(background), Some(curve)) => {
                let background = self.tone(background);
                let ratio = curve.at(self.contrast);
                if hct::ratio_of_tones(background, tone) >= ratio && self.contrast >= 0.0 {
                    tone
                } else {
                    foreground_tone(background, ratio)
                }
            }
            _ => tone,
        };
        let Some(pair) = spec.pair else {
            let tone = self.initial_tone(role);
            if spec.background.is_none() || spec.curve.is_none() {
                return tone;
            }
            return self.clamp_background(role, &spec, enforce(tone));
        };
        let toward_negative = match pair.polarity {
            Polarity::Darker => true,
            Polarity::RelativeLighter => self.dark,
        };
        let absolute = if toward_negative {
            -pair.delta
        } else {
            pair.delta
        };
        let am_a = Self::canonical(role) == pair.a;
        let (own, other) = if am_a {
            (pair.a, pair.b)
        } else {
            (pair.b, pair.a)
        };
        let mut tone = self.initial_tone(own);
        let reference = self.tone(other);
        let relative = absolute * if am_a { 1.0 } else { -1.0 };
        match pair.constraint {
            Constraint::Exact => tone = hct::clamp(0.0, 100.0, reference + relative),
            Constraint::Farther => {
                tone = if relative > 0.0 {
                    hct::clamp(reference + relative, 100.0, tone)
                } else {
                    hct::clamp(0.0, reference + relative, tone)
                }
            }
        }
        self.clamp_background(role, &spec, enforce(tone))
    }

    /// The colour of `role` in this scheme.
    pub fn argb(&self, role: Role) -> Argb {
        let spec = self.spec(role);
        let palette = self.palette(spec.palette);
        let tone = self.tone(role);
        if spec.chroma == 1.0 {
            return palette.tone(tone);
        }
        let chroma = palette.chroma * spec.chroma;
        // A multiplied yellow tone 99 keeps the palette's averaged tone.
        if tone == 99.0 && Hct::is_yellow(palette.hue) {
            return TonalPalette::new(palette.hue, chroma).tone(tone);
        }
        Hct::new(palette.hue, chroma, tone).argb
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Every colour that differs from a reference listing in the form
    /// `tests/material-oracle.mjs` writes.
    fn mismatches(listing: &Value) -> Vec<String> {
        let names: Vec<&str> = listing["roles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap())
            .collect();
        assert!(
            Role::ALL.iter().all(|role| names.contains(&role.name())),
            "the reference names every role"
        );
        let mut wrong = Vec::new();
        for case in listing["cases"].as_array().unwrap() {
            let seed = case["seed"].as_str().unwrap();
            let dark = case["dark"].as_bool().unwrap();
            let contrast = case["contrast"].as_f64().unwrap();
            let scheme = Scheme::new(hct::parse(seed).unwrap(), dark, contrast);
            for (name, expected) in names.iter().zip(case["colors"].as_array().unwrap()) {
                let role = Role::ALL.iter().find(|role| role.name() == *name).unwrap();
                let actual = hct::hex(scheme.argb(*role));
                if actual != expected.as_str().unwrap() {
                    let mode = if dark { "dark" } else { "light" };
                    wrong.push(format!(
                        "{seed} {mode} {contrast} {name}: {actual}, reference {expected}"
                    ));
                }
            }
        }
        wrong
    }

    #[test]
    fn every_role_matches_material_color_utilities() {
        let listing: Value =
            serde_json::from_str(include_str!("../../tests/material-oracle.json")).unwrap();
        assert_eq!(listing["cases"].as_array().unwrap().len(), 96);
        let wrong = mismatches(&listing);
        assert!(wrong.is_empty(), "{} differ: {wrong:#?}", wrong.len());
    }

    /// A wider listing, such as one written for many more seeds, when
    /// `SEELE_MATERIAL_ORACLE` names one.
    #[test]
    fn a_wider_reference_matches_when_one_is_given() {
        let Ok(path) = std::env::var("SEELE_MATERIAL_ORACLE") else {
            return;
        };
        let listing: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let wrong = mismatches(&listing);
        assert!(wrong.is_empty(), "{} differ: {wrong:#?}", wrong.len());
    }

    #[test]
    fn content_keeps_its_contrast_on_every_surface() {
        for dark in [false, true] {
            let scheme = Scheme::new(0xff67_50a4, dark, 0.0);
            for surface in [
                Role::Surface,
                Role::SurfaceContainerLow,
                Role::SurfaceContainer,
                Role::SurfaceContainerHigh,
                Role::SurfaceContainerHighest,
            ] {
                let ratio = |role| hct::ratio_of_tones(scheme.tone(role), scheme.tone(surface));
                assert!(ratio(Role::OnSurface) >= 7.0, "{dark} {surface:?}");
                assert!(ratio(Role::OnSurfaceVariant) >= 4.5, "{dark} {surface:?}");
                assert!(ratio(Role::Primary) >= 3.0, "{dark} {surface:?}");
            }
            let on = |role, ground| hct::ratio_of_tones(scheme.tone(role), scheme.tone(ground));
            assert!(on(Role::OnPrimary, Role::Primary) >= 6.0);
            assert!(on(Role::OnSecondaryContainer, Role::SecondaryContainer) >= 6.0);
            assert!(on(Role::OnErrorContainer, Role::ErrorContainer) >= 4.5);
        }
    }
}
