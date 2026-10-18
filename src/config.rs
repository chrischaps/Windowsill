//! `windowsill.toml`: the sill's settings, re-read whenever the file is saved.
//!
//! The file lives next to the exe, or in `%APPDATA%\Windowsill`. Every key is
//! optional and a missing file means defaults. A bad edit is logged and the
//! last good config kept, so the sill never goes dark over a typo.
//!
//! The sill's shader is watched the same way while its source is on disk (on a
//! machine that built it), so the ledge can be retuned while it runs.

use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use winit::event_loop::EventLoopProxy;

use crate::Wake;
use crate::palette::Palette;
use crate::sill::{self, Ease};
use crate::sun::Place;

pub const FILE_NAME: &str = "windowsill.toml";

/// Where the shader was built from. It only exists on the machine that built it.
const SHADER_SOURCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/glass.wgsl");

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Where the sill is in the world, for the sun and moon. Unknown
    /// means plain daylight.
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// Frames per second while the room is quiet.
    pub fps_idle: u32,
    /// Frames per second while music plays (from M3).
    pub fps_music: u32,
    /// The ledge's colours, as `"#rrggbb"` in the file.
    pub palette: Palette,
    /// How slowly the sill follows a change, in seconds. Never under 2.
    pub ease: Ease,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            latitude: None,
            longitude: None,
            fps_idle: 15,
            fps_music: 30,
            palette: Palette::default(),
            ease: Ease::default(),
        }
    }
}

impl Config {
    /// Where the sill is, once both halves are known.
    pub fn place(&self) -> Option<Place> {
        Some(Place {
            latitude: self.latitude?,
            longitude: self.longitude?,
        })
    }

    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let config: Self = toml::from_str(text)?;
        config.check()?;
        Ok(config)
    }

    /// Reads `path`. A missing file means defaults.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).with_context(|| format!("in {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("could not read {}", path.display())),
        }
    }

    /// Values that parse but that the sill can't honour.
    fn check(&self) -> anyhow::Result<()> {
        if let Some(lat) = self.latitude
            && !(-90.0..=90.0).contains(&lat)
        {
            bail!("latitude {lat} is not between -90 and 90");
        }
        if let Some(lon) = self.longitude
            && !(-180.0..=180.0).contains(&lon)
        {
            bail!("longitude {lon} is not between -180 and 180");
        }
        for (name, fps) in [("fps_idle", self.fps_idle), ("fps_music", self.fps_music)] {
            if !(1..=60).contains(&fps) {
                bail!("{name} {fps} is not between 1 and 60");
            }
        }
        for (name, tau) in self.ease.each() {
            if !(tau >= sill::QUICKEST && tau.is_finite()) {
                bail!(
                    "ease.{name} {tau} is under {} seconds; nothing on the sill may move faster",
                    sill::QUICKEST
                );
            }
        }
        if !(0.0..=1.0).contains(&self.palette.glow_alpha) {
            bail!(
                "palette.glow_alpha {} is not between 0 and 1",
                self.palette.glow_alpha
            );
        }
        Ok(())
    }
}

/// The config file to use: one next to the exe if there is one, otherwise the
/// one in `%APPDATA%\Windowsill`, whether or not it exists yet.
pub fn locate() -> PathBuf {
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(FILE_NAME)));
    if let Some(path) = &beside_exe
        && path.is_file()
    {
        return path.clone();
    }
    match std::env::var_os("APPDATA") {
        Some(appdata) => Path::new(&appdata).join("Windowsill").join(FILE_NAME),
        None => beside_exe.unwrap_or_else(|| PathBuf::from(FILE_NAME)),
    }
}

/// The shader's source on disk, if it is there to be edited.
pub fn shader_source() -> Option<PathBuf> {
    let path = PathBuf::from(SHADER_SOURCE);
    path.is_file().then_some(path)
}

/// Which file was saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edited {
    Config,
    Shader,
}

/// Wakes the event loop with a [`Wake::Edited`] whenever the config file or the
/// shader source is written. Dropping it stops watching.
pub struct Watch {
    _watcher: RecommendedWatcher,
}

impl Watch {
    pub fn new(
        config: &Path,
        shader: Option<&Path>,
        proxy: EventLoopProxy<Wake>,
    ) -> anyhow::Result<Self> {
        let config = std::path::absolute(config)?;
        let config_dir = config
            .parent()
            .ok_or_else(|| anyhow!("{} has no folder", config.display()))?
            .to_path_buf();
        // The folder must exist to be watched, so a file saved there later is seen.
        std::fs::create_dir_all(&config_dir)
            .with_context(|| format!("could not make {}", config_dir.display()))?;
        let shader = shader.map(std::path::absolute).transpose()?;

        let (config_file, shader_file) = (config.clone(), shader.clone());
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else {
                return;
            };
            if !matches!(
                event.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
            ) {
                return;
            }
            for path in &event.paths {
                let edited = if *path == config_file {
                    Edited::Config
                } else if Some(path) == shader_file.as_ref() {
                    Edited::Shader
                } else {
                    continue;
                };
                // If the loop has gone the sill is closing; nothing to wake.
                let _ = proxy.send_event(Wake::Edited(edited));
            }
        })
        .context("could not start watching files")?;

        watcher
            .watch(&config_dir, RecursiveMode::NonRecursive)
            .with_context(|| format!("could not watch {}", config_dir.display()))?;
        if let Some(shader) = &shader
            && let Some(dir) = shader.parent()
            && dir != config_dir
        {
            watcher
                .watch(dir, RecursiveMode::NonRecursive)
                .with_context(|| format!("could not watch {}", dir.display()))?;
        }
        Ok(Self { _watcher: watcher })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::Colour;

    #[test]
    fn an_empty_file_is_all_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn a_missing_file_is_all_defaults() {
        let path = std::env::temp_dir().join("windowsill-no-such-config.toml");
        assert_eq!(Config::load(&path).unwrap(), Config::default());
    }

    #[test]
    fn keys_override_defaults() {
        let config = Config::parse(
            "latitude = 47.6\nlongitude = -122.3\nfps_idle = 10\n[palette]\nwood = \"#ff8000\"\n",
        )
        .unwrap();
        assert_eq!(config.latitude, Some(47.6));
        assert_eq!(config.fps_idle, 10);
        assert_eq!(config.fps_music, 30);
        assert_eq!(config.palette.wood, Colour::rgb(1.0, 128.0 / 255.0, 0.0));
        assert_eq!(config.palette.lip, Palette::default().lip);
    }

    #[test]
    fn the_example_file_is_the_defaults() {
        // Hex can only say the defaults to within one 8-bit step.
        let example = Config::parse(include_str!("../windowsill.example.toml")).unwrap();
        let near = |a: Colour, b: Colour| {
            a.0.iter()
                .zip(b.0)
                .all(|(a, b)| (a - b).abs() <= 1.0 / 255.0)
        };
        let (e, d) = (example.palette, Palette::default());
        assert!(near(e.wood, d.wood) && near(e.wood_shade, d.wood_shade));
        assert!(near(e.lip, d.lip) && near(e.glow, d.glow));
        assert_eq!(e.glow_alpha, d.glow_alpha);
        assert_eq!((example.fps_idle, example.fps_music), (15, 30));
        assert_eq!(example.ease, Ease::default());
    }

    #[test]
    fn bad_edits_are_refused() {
        for text in [
            "fps_idle = ",
            "fps_idel = 15",
            "fps_idle = 0",
            "fps_music = 240",
            "latitude = 91",
            "longitude = \"west\"",
            "[palette]\nwood = \"brown\"",
            "[palette]\nwood = \"#12345\"",
            "[palette]\nglow_alpha = 1.5",
        ] {
            assert!(Config::parse(text).is_err(), "accepted {text:?}");
        }
    }
}
