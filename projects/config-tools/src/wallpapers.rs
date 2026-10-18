//! Which image each output shows for a theme: the host's wallpaper mapping.
//!
//! A theme folder carries at most one image of its own, and the catalog names
//! the configured wallpaper every other theme shows. The host declares, in
//! Nix, what changes that: an image per light or dark theme, per output, and
//! per theme, each optionally per output. The catalog carries that declaration
//! as `wallpapers`, so it is rebuilt with every generation and never kept in
//! the helper's state. The helper resolves it for the published theme alone;
//! the shell only looks an output's name up in the result.
//!
//! For a theme on an output the first of these that is set wins:
//!
//! 1. the image mapped to that theme on that output;
//! 2. the image mapped to that theme;
//! 3. the theme's own image, from its folder;
//! 4. the image mapped to that output for the theme's light or dark mode;
//! 5. the image mapped to that output;
//! 6. the image mapped to the theme's light or dark mode;
//! 7. the configured wallpaper.
//!
//! What is declared for a theme beats what the theme brings, and what the
//! theme brings beats what is declared for every theme. Among the rest an
//! output comes before a mode, because an output's image is chosen for its
//! shape, which no mode changes. Light and dark are the theme's own mode, not
//! the slot it fills: a dark theme the light slot wears still looks dark, and
//! its wallpaper should too.
use crate::picture;
use crate::theme_contract::output_name;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// A wallpaper mapping names no more outputs than a desk holds, and paths no
/// longer than a store path needs, so what it adds to a publication stays well
/// inside the bound `selection.json` is read back with.
const OUTPUT_LIMIT: usize = 16;
const PATH_LIMIT: usize = 1024;

/// Images for one place: any mode, and light and dark themes.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Images {
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub light: Option<String>,
    #[serde(default)]
    pub dark: Option<String>,
}

impl Images {
    fn for_mode(&self, light: bool) -> Option<&str> {
        (if light { &self.light } else { &self.dark })
            .as_deref()
            .or(self.image.as_deref())
    }
}

/// Images for one theme. A theme has one mode, so it needs no other.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeImages {
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
}

/// The catalog's `wallpapers`. Every part is optional, and an empty mapping
/// leaves each theme its own image or the configured wallpaper.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Wallpapers {
    #[serde(default)]
    pub light: Option<String>,
    #[serde(default)]
    pub dark: Option<String>,
    #[serde(default)]
    pub outputs: BTreeMap<String, Images>,
    #[serde(default)]
    pub themes: BTreeMap<String, ThemeImages>,
}

/// What the publication carries: the image every output draws, and the
/// outputs that draw another one.
#[derive(Debug, PartialEq)]
pub struct Choice {
    pub wallpaper: String,
    pub outputs: BTreeMap<String, String>,
}

/// Why `file` cannot be a mapped image, if it cannot. A file that does not
/// exist is allowed, as it is for the configured wallpaper: the background
/// then shows its base colour, and a picture deleted later must not stop the
/// theme switcher. One that exists has to be an image Qt draws. As for a theme
/// folder's own image, a `thorough` read decodes it completely, as the build
/// does through `seele-theme check`; every other read, which `follow` repeats
/// each minute, rechecks only its signature.
fn image_problem(file: &str, thorough: bool) -> Option<String> {
    if file.is_empty()
        || !file.starts_with('/')
        || file.len() > PATH_LIMIT
        || file.chars().any(char::is_control)
    {
        return Some("is not an absolute path".to_owned());
    }
    let path = Path::new(file);
    if path.symlink_metadata().is_err() {
        None
    } else if thorough {
        picture::fault(path)
    } else {
        (!picture::signed(path)).then(|| "is not a PNG or JPEG image".to_owned())
    }
}

impl Wallpapers {
    /// Every rule the mapping breaks, for a catalog of `themes`. Each theme it
    /// names has to be in the catalog and each output has to be a name the
    /// compositor could give one, so a typo is refused rather than ignored.
    /// `thorough` decodes every mapped image (`image_problem`).
    pub fn violations(&self, themes: &BTreeSet<&str>, thorough: bool) -> Vec<String> {
        let mut errors = Vec::new();
        let mut file = |place: String, value: &Option<String>| {
            if let Some(problem) = value
                .as_deref()
                .and_then(|file| image_problem(file, thorough))
            {
                errors.push(format!(
                    "{place}: {:?} {problem}",
                    value.as_deref().unwrap_or_default()
                ));
            }
        };
        file("light".into(), &self.light);
        file("dark".into(), &self.dark);
        for (output, images) in &self.outputs {
            file(format!("outputs.{output:?}.image"), &images.image);
            file(format!("outputs.{output:?}.light"), &images.light);
            file(format!("outputs.{output:?}.dark"), &images.dark);
        }
        for (theme, images) in &self.themes {
            file(format!("themes.{theme:?}.image"), &images.image);
            for (output, path) in &images.outputs {
                file(
                    format!("themes.{theme:?}.outputs.{output:?}"),
                    &Some(path.clone()),
                );
            }
        }
        let mut outputs: BTreeSet<&str> = self.outputs.keys().map(String::as_str).collect();
        for (theme, images) in &self.themes {
            if !themes.contains(theme.as_str()) {
                errors.push(format!("themes.{theme:?} is not a theme in the catalog"));
            }
            outputs.extend(images.outputs.keys().map(String::as_str));
        }
        for output in &outputs {
            if !output_name(output) {
                errors.push(format!(
                    "{output:?} is not an output name: letters, digits, '.', '_' and '-', at most 64"
                ));
            }
        }
        if outputs.len() > OUTPUT_LIMIT {
            errors.push(format!(
                "names {} outputs, at most {OUTPUT_LIMIT}",
                outputs.len()
            ));
        }
        errors
    }

    /// The image a theme of `id` and mode `light`, whose own image is `own`,
    /// shows on `output`; with no output, on every output nothing more
    /// particular is mapped for. `configured` is the catalog's wallpaper.
    fn image(
        &self,
        id: &str,
        light: bool,
        own: Option<&str>,
        output: Option<&str>,
        configured: &str,
    ) -> String {
        let theme = self.themes.get(id);
        let place = output.and_then(|output| self.outputs.get(output));
        let mode = if light { &self.light } else { &self.dark };
        output
            .and_then(|output| theme?.outputs.get(output))
            .map(String::as_str)
            .or_else(|| theme?.image.as_deref())
            .or(own)
            .or_else(|| place?.for_mode(light))
            .or(mode.as_deref())
            .unwrap_or(configured)
            .to_owned()
    }

    /// The image theme `id` is linked to, to picture it where it is listed:
    /// the one mapped to the theme, its own, or the one mapped to its light or
    /// dark mode, as for an output nothing is mapped for. The configured
    /// wallpaper links no theme, since every theme shows it that has nothing
    /// else, and an output's image belongs to that output rather than to the
    /// theme.
    pub fn linked(&self, id: &str, light: bool, own: Option<&str>) -> Option<String> {
        let mode = if light { &self.light } else { &self.dark };
        self.themes
            .get(id)
            .and_then(|theme| theme.image.as_deref())
            .or(own)
            .or(mode.as_deref())
            .map(str::to_owned)
    }

    /// What the publication of theme `id` carries. An output appears only
    /// when its image differs from the one every other output draws, so a
    /// mapping that changes nothing for this theme publishes no outputs.
    pub fn choose(&self, id: &str, light: bool, own: Option<&str>, configured: &str) -> Choice {
        let wallpaper = self.image(id, light, own, None, configured);
        let named = self.outputs.keys().chain(
            self.themes
                .get(id)
                .into_iter()
                .flat_map(|theme| theme.outputs.keys()),
        );
        let mut outputs = BTreeMap::new();
        for output in named {
            let image = self.image(id, light, own, Some(output), configured);
            if image != wallpaper {
                outputs.insert(output.clone(), image);
            }
        }
        Choice { wallpaper, outputs }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picture::tests::{jpeg_image, png_image};
    use serde_json::json;

    const CONFIGURED: &str = "/etc/wallpaper/wallpaper.jpg";

    fn mapping(value: serde_json::Value) -> Wallpapers {
        serde_json::from_value(value).unwrap()
    }
    fn chosen(
        mapping: &Wallpapers,
        id: &str,
        light: bool,
        own: Option<&str>,
    ) -> (String, Vec<(&'static str, String)>) {
        let choice = mapping.choose(id, light, own, CONFIGURED);
        let outputs = ["DP-1", "HDMI-A-1", "eDP-1"]
            .into_iter()
            .filter_map(|output| {
                choice
                    .outputs
                    .get(output)
                    .map(|image| (output, image.clone()))
            })
            .collect();
        (choice.wallpaper, outputs)
    }

    #[test]
    fn without_a_mapping_a_theme_shows_its_own_image_or_the_configured_one() {
        let empty = Wallpapers::default();
        assert_eq!(
            chosen(&empty, "mocha", false, None),
            (CONFIGURED.into(), vec![])
        );
        assert_eq!(
            chosen(&empty, "pictured", false, Some("/own.png")),
            ("/own.png".into(), vec![])
        );
        assert_eq!(mapping(json!({})), empty);
    }

    #[test]
    fn light_and_dark_themes_each_take_their_mode_s_image() {
        let modes = mapping(json!({"light": "/day.png", "dark": "/night.png"}));
        assert_eq!(chosen(&modes, "latte", true, None).0, "/day.png");
        assert_eq!(chosen(&modes, "mocha", false, None).0, "/night.png");
        let light_only = mapping(json!({"light": "/day.png"}));
        assert_eq!(chosen(&light_only, "mocha", false, None).0, CONFIGURED);
        // A theme's own image is its look, in either mode.
        assert_eq!(
            chosen(&modes, "pictured", false, Some("/own.png")).0,
            "/own.png"
        );
    }

    #[test]
    fn an_output_takes_its_own_image_and_the_rest_keep_the_theme_s() {
        let desk = mapping(json!({
            "dark": "/night.png",
            "outputs": {
                "DP-1": {"image": "/wide.png"},
                "HDMI-A-1": {"light": "/side-day.png", "dark": "/side-night.png"},
            },
        }));
        assert_eq!(
            chosen(&desk, "mocha", false, None),
            (
                "/night.png".into(),
                vec![
                    ("DP-1", "/wide.png".into()),
                    ("HDMI-A-1", "/side-night.png".into())
                ]
            )
        );
        // An output whose image is the one everywhere else is not named.
        assert_eq!(
            chosen(&desk, "latte", true, None),
            (
                CONFIGURED.into(),
                vec![
                    ("DP-1", "/wide.png".into()),
                    ("HDMI-A-1", "/side-day.png".into())
                ]
            )
        );
        let dark_only = mapping(json!({"outputs": {"HDMI-A-1": {"dark": "/side-night.png"}}}));
        assert_eq!(
            chosen(&dark_only, "latte", true, None),
            (CONFIGURED.into(), vec![])
        );
        // An output is chosen for its shape, so its image comes before the mode's.
        let shaped =
            mapping(json!({"light": "/day.png", "outputs": {"DP-1": {"image": "/portrait.png"}}}));
        assert_eq!(
            chosen(&shaped, "latte", true, None),
            ("/day.png".into(), vec![("DP-1", "/portrait.png".into())])
        );
    }

    #[test]
    fn a_theme_s_own_image_comes_before_what_is_mapped_for_every_theme() {
        let desk =
            mapping(json!({"dark": "/night.png", "outputs": {"DP-1": {"image": "/wide.png"}}}));
        assert_eq!(
            chosen(&desk, "pictured", false, Some("/own.png")),
            ("/own.png".into(), vec![])
        );
    }

    #[test]
    fn a_theme_is_linked_to_its_own_image_or_one_mapped_to_it_or_its_mode() {
        let empty = Wallpapers::default();
        assert_eq!(
            empty.linked("mocha", false, None),
            None,
            "the configured wallpaper links no theme"
        );
        assert_eq!(
            empty.linked("pictured", false, Some("/own.png")).as_deref(),
            Some("/own.png")
        );
        let desk = mapping(json!({
            "dark": "/night.png",
            "outputs": {"DP-1": {"image": "/wide.png"}},
            "themes": {"nord": {"image": "/fjord.png"}, "latte": {"outputs": {"DP-1": "/latte-wide.png"}}},
        }));
        assert_eq!(
            desk.linked("mocha", false, None).as_deref(),
            Some("/night.png")
        );
        assert_eq!(
            desk.linked("latte", true, None),
            None,
            "an output's image is that output's"
        );
        assert_eq!(
            desk.linked("nord", false, Some("/own.png")).as_deref(),
            Some("/fjord.png")
        );
        assert_eq!(
            desk.linked("pictured", false, Some("/own.png")).as_deref(),
            Some("/own.png")
        );
    }

    #[test]
    fn what_is_mapped_to_a_theme_comes_before_everything_else() {
        let desk = mapping(json!({
            "dark": "/night.png",
            "outputs": {"DP-1": {"image": "/wide.png"}, "HDMI-A-1": {"dark": "/side-night.png"}},
            "themes": {
                "pictured": {"outputs": {"HDMI-A-1": "/pictured-side.png"}},
                "mocha": {"image": "/mocha.png", "outputs": {"eDP-1": "/mocha-laptop.png"}},
            },
        }));
        // Mapped per output only: its own image elsewhere, the mapped one there.
        assert_eq!(
            chosen(&desk, "pictured", false, Some("/own.png")),
            (
                "/own.png".into(),
                vec![("HDMI-A-1", "/pictured-side.png".into())]
            )
        );
        // Mapped as a whole: on every output, apart from the one it names.
        assert_eq!(
            chosen(&desk, "mocha", false, None),
            (
                "/mocha.png".into(),
                vec![("eDP-1", "/mocha-laptop.png".into())]
            )
        );
        // Another theme is untouched by both.
        assert_eq!(
            chosen(&desk, "frappe", false, None),
            (
                "/night.png".into(),
                vec![
                    ("DP-1", "/wide.png".into()),
                    ("HDMI-A-1", "/side-night.png".into())
                ]
            )
        );
    }

    #[test]
    fn the_build_decodes_every_mapped_image_and_other_reads_check_its_signature() {
        let directory = tempfile::tempdir().unwrap();
        let png = png_image(16, 9);
        let jpeg = jpeg_image(16, 9);
        let cut_png = directory.path().join("cut.png");
        std::fs::write(&cut_png, &png[..png.len() / 2]).unwrap();
        let cut_jpeg = directory.path().join("cut.jpg");
        std::fs::write(&cut_jpeg, &jpeg[..jpeg.len() - 2]).unwrap();
        let themes: BTreeSet<&str> = ["mocha"].into();
        let cut = mapping(json!({"light": cut_png, "dark": cut_jpeg}));
        // `follow` reads the catalog every minute: only the signature then.
        assert_eq!(cut.violations(&themes, false), Vec::<String>::new());
        let refused = cut.violations(&themes, true);
        assert_eq!(refused.len(), 2, "{refused:?}");
        assert!(
            refused[0].starts_with(&format!(
                "light: {:?} is not a complete PNG image",
                cut_png.to_str().unwrap()
            )),
            "{refused:?}"
        );
        assert!(
            refused[1].starts_with(&format!(
                "dark: {:?} is not a complete JPEG image",
                cut_jpeg.to_str().unwrap()
            )),
            "{refused:?}"
        );
    }

    #[test]
    fn a_mapping_is_refused_for_every_rule_it_breaks() {
        let directory = tempfile::tempdir().unwrap();
        let gif = directory.path().join("animated.gif");
        std::fs::write(&gif, b"GIF89a").unwrap();
        let png = directory.path().join("still.png");
        std::fs::write(&png, png_image(16, 9)).unwrap();
        let jpeg = directory.path().join("dusk #1?.jpg");
        std::fs::write(&jpeg, jpeg_image(16, 9)).unwrap();
        let themes: BTreeSet<&str> = ["mocha", "latte"].into();
        let fine = mapping(json!({
            "light": png,
            "dark": "/not/there/yet.jpg",
            "outputs": {"DP-1": {"image": jpeg}},
            "themes": {"mocha": {"outputs": {"HDMI-A-1": png}}},
        }));
        assert_eq!(fine.violations(&themes, false), Vec::<String>::new());
        assert_eq!(fine.violations(&themes, true), Vec::<String>::new());
        let long = mapping(json!({"light": format!("/{}", "x".repeat(PATH_LIMIT))}));
        assert_eq!(
            long.violations(&themes, true),
            [format!(
                "light: \"/{}\" is not an absolute path",
                "x".repeat(PATH_LIMIT)
            )]
        );
        let broken = mapping(json!({
            "light": "day.png",
            "dark": gif,
            "outputs": {"DP 1": {"image": png}, "HDMI-A-1": {"dark": directory.path()}},
            "themes": {"mocah": {"image": png}, "latte": {"outputs": {"../DP-1": png}}},
        }));
        assert_eq!(
            broken.violations(&themes, false),
            [
                "light: \"day.png\" is not an absolute path".to_owned(),
                format!(
                    "dark: {:?} is not a PNG or JPEG image",
                    gif.to_str().unwrap()
                ),
                format!(
                    "outputs.\"HDMI-A-1\".dark: {:?} is not a PNG or JPEG image",
                    directory.path().to_str().unwrap()
                ),
                "themes.\"mocah\" is not a theme in the catalog".to_owned(),
                "\"../DP-1\" is not an output name: letters, digits, '.', '_' and '-', at most 64"
                    .to_owned(),
                "\"DP 1\" is not an output name: letters, digits, '.', '_' and '-', at most 64"
                    .to_owned(),
            ]
        );
        let crowded = mapping(json!({
            "outputs": (0..17).map(|n| (format!("DP-{n}"), json!({}))).collect::<serde_json::Map<_, _>>(),
        }));
        assert_eq!(
            crowded.violations(&themes, true),
            ["names 17 outputs, at most 16"]
        );
        assert!(serde_json::from_value::<Wallpapers>(
            json!({"outputs": {"DP-1": {"sepia": "/x.png"}}})
        )
        .is_err());
        assert!(serde_json::from_value::<Wallpapers>(
            json!({"themes": {"mocha": {"light": "/x.png"}}})
        )
        .is_err());
    }
}
