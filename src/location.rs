//! Where the sill is in the world, without asking anyone for a city.
//!
//! `latitude` and `longitude` in `windowsill.toml` (or `--lat/--lon`) come
//! first, and while they are set Windows is never asked. Without them the sill
//! asks Windows' own location service, on a thread of its own, when it starts,
//! about once a day after that, and after waking from sleep. Nothing here ever
//! prompts; whatever Windows shows the first time is Windows' own.
//!
//! The answer is kept in `%APPDATA%\Windowsill\place.toml`, rounded to about a
//! kilometre, so a cold start has the true sun before Windows answers. If
//! location is off or denied, the kept place is forgotten too: that is someone
//! saying the sill shouldn't know where it is. With no place at all the sill
//! shows plain daylight. It never guesses from the time zone.

use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::Deserialize;
use winit::event_loop::EventLoopProxy;

use crate::Wake;
use crate::config;
use crate::sun::Place;

const FILE_NAME: &str = "place.toml";

/// How long a place from Windows is good for before asking again.
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// After waking from sleep the sill asks again, unless it asked this recently.
const SINCE_WAKING: Duration = Duration::from_secs(60 * 60);

/// Why Windows didn't say where the sill is.
#[derive(Clone, Debug, PartialEq)]
pub enum Unknown {
    /// Location is off, for the whole machine or for desktop apps.
    Off,
    /// The sill in particular isn't allowed to know.
    Denied,
    /// Location is on, but Windows couldn't find a place this time.
    Lost(String),
}

impl Unknown {
    /// Whether this is someone choosing that the sill shouldn't know.
    fn chosen(&self) -> bool {
        matches!(self, Unknown::Off | Unknown::Denied)
    }
}

impl fmt::Display for Unknown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unknown::Off => write!(f, "location is turned off"),
            Unknown::Denied => write!(f, "the sill isn't allowed to know its location"),
            Unknown::Lost(why) => write!(f, "Windows couldn't find a place ({why})"),
        }
    }
}

/// What Windows said.
pub type Found = Result<Place, Unknown>;

/// What the sill knows of where it is, and when it last asked.
#[derive(Debug, Default)]
pub struct Whereabouts {
    /// The last place Windows gave, if it still stands.
    pub place: Option<Place>,
    asked: Option<SystemTime>,
    asking: bool,
    /// The last reason heard for not knowing, so it is logged once.
    unknown: Option<Unknown>,
}

/// What changed after hearing from Windows.
#[derive(Debug, PartialEq)]
pub struct Heard {
    /// The place moved, appeared or was forgotten.
    pub moved: bool,
    /// A reason for not knowing that hasn't been logged yet.
    pub news: Option<Unknown>,
}

impl Whereabouts {
    /// Starts from the place kept on disk, if there is one.
    pub fn remembered() -> Self {
        Self {
            place: path().and_then(|path| {
                let text = std::fs::read_to_string(path).ok()?;
                parse(&text)
            }),
            ..Self::default()
        }
    }

    /// Whether it is time to ask Windows again. A clock set back counts.
    pub fn due(&self, now: SystemTime) -> bool {
        !self.asking
            && self
                .asked
                .is_none_or(|asked| now.duration_since(asked).map_or(true, |since| since >= DAY))
    }

    /// Asks Windows, off this thread. The answer comes back as [`Wake::Located`].
    pub fn ask(&mut self, now: SystemTime, proxy: EventLoopProxy<Wake>) {
        self.asking = true;
        self.asked = Some(now);
        let spawned = std::thread::Builder::new()
            .name("location".into())
            .spawn(move || {
                // Only fails once the loop has exited, when no one is listening.
                let _ = proxy.send_event(Wake::Located(locate()));
            });
        if let Err(err) = spawned {
            self.asking = false;
            tracing::warn!("could not ask Windows where the sill is: {err}");
        }
    }

    /// The machine woke from sleep, and may have travelled while it slept.
    pub fn woke(&mut self, now: SystemTime) {
        if self
            .asked
            .is_some_and(|asked| now.duration_since(asked).is_ok_and(|s| s >= SINCE_WAKING))
        {
            self.asked = None;
        }
    }

    /// Takes in what Windows said, keeping or forgetting the place on disk.
    pub fn hear(&mut self, found: Found) -> Heard {
        let heard = self.take(found);
        if heard.moved {
            match self.place {
                Some(place) => keep(place),
                None => forget(),
            }
        }
        heard
    }

    /// [`Whereabouts::hear`] without the disk.
    fn take(&mut self, found: Found) -> Heard {
        self.asking = false;
        let was = self.place;
        let news = match found {
            Ok(place) => {
                self.place = Some(rounded(place));
                self.unknown = None;
                None
            }
            Err(why) => {
                if why.chosen() {
                    self.place = None;
                }
                let news = (self.unknown.as_ref() != Some(&why)).then(|| why.clone());
                self.unknown = Some(why);
                news
            }
        };
        Heard {
            moved: self.place != was,
            news,
        }
    }
}

/// Kept to two decimal places, about a kilometre: plenty for the sun, and no
/// more than it needs to know.
fn rounded(place: Place) -> Place {
    let round = |degrees: f64| (degrees * 100.0).round() / 100.0;
    Place {
        latitude: round(place.latitude),
        longitude: round(place.longitude),
    }
}

fn path() -> Option<PathBuf> {
    config::app_dir().map(|dir| dir.join(FILE_NAME))
}

/// The place in a kept file, if it is whole and makes sense.
fn parse(text: &str) -> Option<Place> {
    #[derive(Deserialize)]
    struct Kept {
        latitude: f64,
        longitude: f64,
    }
    let kept: Kept = toml::from_str(text).ok()?;
    ((-90.0..=90.0).contains(&kept.latitude) && (-180.0..=180.0).contains(&kept.longitude))
        .then_some(Place {
            latitude: kept.latitude,
            longitude: kept.longitude,
        })
}

fn render(place: Place) -> String {
    format!(
        "# Where Windows last said the sill is, so it starts in the true light.\n\
         # It is forgotten when location is turned off. Set latitude and\n\
         # longitude in windowsill.toml instead to choose the place yourself.\n\
         latitude = {:.2}\n\
         longitude = {:.2}\n",
        place.latitude, place.longitude
    )
}

fn keep(place: Place) {
    let Some(path) = path() else { return };
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, render(place)));
    if let Err(err) = written {
        tracing::warn!("could not keep the place in {}: {err}", path.display());
    }
}

fn forget() {
    let Some(path) = path() else { return };
    match std::fs::remove_file(&path) {
        Ok(()) => tracing::info!("forgot the place kept in {}", path.display()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => tracing::warn!("could not forget {}: {err}", path.display()),
    }
}

/// Asks Windows where the machine is, waiting up to a minute for an answer.
#[cfg(windows)]
fn locate() -> Found {
    use windows::Devices::Geolocation::{Geolocator, PositionAccuracy, PositionStatus};
    use windows::Foundation::TimeSpan;
    use windows::Win32::Foundation::E_ACCESSDENIED;

    // TimeSpan counts in 100 ns ticks.
    const SECOND: i64 = 10_000_000;
    let lost = |err: windows::core::Error| Unknown::Lost(err.message().trim().to_owned());

    let locator = Geolocator::new().map_err(lost)?;
    // The sun doesn't need to know the street.
    locator
        .SetDesiredAccuracy(PositionAccuracy::Default)
        .map_err(lost)?;
    if locator.LocationStatus().map_err(lost)? == PositionStatus::Disabled {
        return Err(Unknown::Off);
    }
    let position = locator
        .GetGeopositionAsyncWithAgeAndTimeout(
            TimeSpan {
                Duration: 60 * 60 * SECOND,
            },
            TimeSpan {
                Duration: 60 * SECOND,
            },
        )
        .and_then(|asking| asking.join())
        .map_err(|err| match err.code() {
            E_ACCESSDENIED => Unknown::Denied,
            _ => lost(err),
        })?;
    let at = position
        .Coordinate()
        .and_then(|coordinate| coordinate.Point())
        .and_then(|point| point.Position())
        .map_err(lost)?;
    Ok(Place {
        latitude: at.Latitude,
        longitude: at.Longitude,
    })
}

#[cfg(not(windows))]
fn locate() -> Found {
    Err(Unknown::Lost(
        "there is no location service off Windows".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONDON: Place = Place {
        latitude: 51.5074,
        longitude: -0.1278,
    };

    fn at(hours: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(hours * 60 * 60)
    }

    #[test]
    fn rounds_to_about_a_kilometre() {
        assert_eq!(
            rounded(LONDON),
            Place {
                latitude: 51.51,
                longitude: -0.13
            }
        );
    }

    #[test]
    fn a_kept_place_reads_back() {
        let place = rounded(LONDON);
        assert_eq!(parse(&render(place)), Some(place));
    }

    #[test]
    fn a_broken_or_impossible_kept_place_is_ignored() {
        for text in [
            "",
            "latitude = 51.5",
            "latitude = 91.0\nlongitude = 0.0",
            "latitude = 0.0\nlongitude = -181.0",
            "latitude = \"north\"\nlongitude = 0.0",
        ] {
            assert_eq!(parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn asks_at_start_then_once_a_day() {
        let mut whereabouts = Whereabouts::default();
        assert!(whereabouts.due(at(0)));
        whereabouts.asked = Some(at(0));
        assert!(!whereabouts.due(at(23)));
        assert!(whereabouts.due(at(24)));
    }

    #[test]
    fn never_asks_twice_at_once() {
        let mut whereabouts = Whereabouts {
            asking: true,
            ..Whereabouts::default()
        };
        assert!(!whereabouts.due(at(0)));
        whereabouts.take(Ok(LONDON));
        assert!(whereabouts.due(at(0)));
    }

    #[test]
    fn a_clock_set_back_counts_as_time_to_ask() {
        let whereabouts = Whereabouts {
            asked: Some(at(10)),
            ..Whereabouts::default()
        };
        assert!(whereabouts.due(at(9)));
    }

    #[test]
    fn waking_asks_again_unless_it_just_asked() {
        let mut whereabouts = Whereabouts {
            asked: Some(at(0)),
            ..Whereabouts::default()
        };
        whereabouts.woke(SystemTime::UNIX_EPOCH + Duration::from_secs(10 * 60));
        assert!(!whereabouts.due(at(1)));
        whereabouts.woke(at(2));
        assert!(whereabouts.due(at(2)));
    }

    #[test]
    fn says_why_once() {
        let mut whereabouts = Whereabouts::default();
        let lost = || Err(Unknown::Lost("no fix".into()));
        assert_eq!(
            whereabouts.take(lost()).news,
            Some(Unknown::Lost("no fix".into()))
        );
        assert_eq!(whereabouts.take(lost()).news, None);
        assert_eq!(whereabouts.take(Err(Unknown::Off)).news, Some(Unknown::Off));
        whereabouts.take(Ok(LONDON));
        assert_eq!(whereabouts.take(Err(Unknown::Off)).news, Some(Unknown::Off));
    }

    #[test]
    fn turning_location_off_forgets_the_place_but_a_lost_fix_keeps_it() {
        let mut whereabouts = Whereabouts::default();
        assert!(whereabouts.take(Ok(LONDON)).moved);
        assert!(!whereabouts.take(Ok(LONDON)).moved);
        assert!(!whereabouts.take(Err(Unknown::Lost("no fix".into()))).moved);
        assert_eq!(whereabouts.place, Some(rounded(LONDON)));
        assert!(whereabouts.take(Err(Unknown::Denied)).moved);
        assert_eq!(whereabouts.place, None);
    }
}
