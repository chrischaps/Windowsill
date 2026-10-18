//! The command line, which is only for checking how the sill looks: a whole
//! day sped up, a moment held still, or the sky of somewhere else.

use std::time::Duration;

use anyhow::{Context, anyhow, bail};

pub const USAGE: &str = "\
usage: windowsill [--day-in <span>] [--at <hh:mm>] [--lat <degrees> --lon <degrees>]

  --day-in 60s   run a whole day in the given span (s, m or h; plain seconds)
  --at 06:30     start the day at this local time, or hold it there without --day-in
  --lat, --lon   look at the sky from here instead of windowsill.toml's place";

/// What the command line asked for. Everything is optional; nothing means the
/// real clock and the config's place.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Args {
    pub day_in: Option<Duration>,
    pub at: Option<(u32, u32)>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub help: bool,
}

impl Args {
    /// Reads the arguments after the program's name. Each flag takes its value
    /// either as the next argument or after `=`.
    pub fn parse(args: impl IntoIterator<Item = String>) -> anyhow::Result<Self> {
        let mut parsed = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_owned(), Some(value.to_owned())),
                None => (arg, None),
            };
            if flag == "--help" || flag == "-h" {
                parsed.help = true;
                continue;
            }
            let value = inline
                .or_else(|| args.next())
                .ok_or_else(|| anyhow!("{flag} needs a value"))?;
            match flag.as_str() {
                "--day-in" => parsed.day_in = Some(span(&value)?),
                "--at" => parsed.at = Some(clock_time(&value)?),
                "--lat" => parsed.latitude = Some(degrees(&value, 90.0, "--lat")?),
                "--lon" => parsed.longitude = Some(degrees(&value, 180.0, "--lon")?),
                _ => bail!("unknown option {flag}"),
            }
        }
        if parsed.latitude.is_some() != parsed.longitude.is_some() {
            bail!("--lat and --lon go together");
        }
        Ok(parsed)
    }
}

/// A span like `60s`, `1.5m`, `2h` or `90`, at least a second long.
fn span(text: &str) -> anyhow::Result<Duration> {
    let (number, unit) = match text.char_indices().last() {
        Some((i, 's')) => (&text[..i], 1.0),
        Some((i, 'm')) => (&text[..i], 60.0),
        Some((i, 'h')) => (&text[..i], 3600.0),
        _ => (text, 1.0),
    };
    let seconds = number
        .parse::<f64>()
        .ok()
        .map(|n| n * unit)
        .filter(|s| s.is_finite())
        .with_context(|| format!("--day-in {text} is not a span like 60s, 2m or 1h"))?;
    if seconds < 1.0 {
        bail!("--day-in {text} is shorter than a second");
    }
    Ok(Duration::from_secs_f64(seconds))
}

/// A clock time like `06:30` or `6:30`.
fn clock_time(text: &str) -> anyhow::Result<(u32, u32)> {
    text.split_once(':')
        .and_then(|(h, m)| Some((h.parse().ok()?, m.parse().ok()?)))
        .filter(|&(h, m): &(u32, u32)| h < 24 && m < 60)
        .with_context(|| format!("--at {text} is not a time like 06:30"))
}

/// Degrees within `±limit`.
fn degrees(text: &str, limit: f64, flag: &str) -> anyhow::Result<f64> {
    text.parse::<f64>()
        .ok()
        .filter(|d| d.abs() <= limit)
        .with_context(|| format!("{flag} {text} is not between -{limit} and {limit}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> anyhow::Result<Args> {
        Args::parse(line.split_whitespace().map(str::to_owned))
    }

    #[test]
    fn nothing_means_nothing_changed() {
        assert_eq!(parse("").unwrap(), Args::default());
    }

    #[test]
    fn a_day_in_a_minute() {
        let args = parse("--day-in 60s").unwrap();
        assert_eq!(args.day_in, Some(Duration::from_secs(60)));
        assert_eq!(parse("--day-in=60s").unwrap(), args);
        assert_eq!(parse("--day-in 60").unwrap(), args);
        assert_eq!(parse("--day-in 1m").unwrap(), args);
        assert_eq!(
            parse("--day-in 1.5h").unwrap().day_in,
            Some(Duration::from_secs(5400))
        );
    }

    #[test]
    fn spans_that_are_not_spans() {
        for bad in [
            "--day-in",
            "--day-in soon",
            "--day-in 0s",
            "--day-in -5m",
            "--day-in inf",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_time_and_a_place() {
        let args = parse("--at 6:30 --lat -33.9 --lon=151.2 --day-in 2m").unwrap();
        assert_eq!(args.at, Some((6, 30)));
        assert_eq!(args.latitude, Some(-33.9));
        assert_eq!(args.longitude, Some(151.2));
        assert_eq!(args.day_in, Some(Duration::from_secs(120)));
    }

    #[test]
    fn times_and_places_that_do_not_exist() {
        for bad in [
            "--at 24:00",
            "--at 6:60",
            "--at noon",
            "--lat 91 --lon 0",
            "--lat 0 --lon 181",
            "--lat 10",
            "--lon 10",
            "--fast",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn help() {
        assert!(parse("--help").unwrap().help);
        assert!(parse("-h --day-in 60s").unwrap().help);
    }
}
