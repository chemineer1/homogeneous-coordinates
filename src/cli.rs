use std::{collections::HashSet, ffi::OsString, path::PathBuf};

#[derive(Debug, PartialEq)]
pub struct Options {
    pub path: PathBuf,
    pub still: bool,
    pub seconds: f32,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, PartialEq)]
pub enum Request {
    Help,
    Version,
    Render(Options),
}

pub fn help(duration: f32) -> String {
    format!(
        "\
Render a {duration}-second silent animation of homogeneous coordinates.

Usage:
  one-dimension-up [--output FILE.mp4] [OPTIONS]
  one-dimension-up --still FILE.png [--time SECONDS] [OPTIONS]

Options:
  --output FILE.mp4   Video destination (default: homogeneous-coordinates.mp4)
  --still FILE.png    Render one PNG instead of a video; conflicts with --output
  --time SECONDS      Still-frame time, 0 <= time < {duration} (default: 0; requires --still)
  --fps N            Frames per second, 12–60 (default: 30)
  --width N          Even width, 640–3840 pixels (default: 1920)
  --height N         Even height, 360–2160 pixels (default: 1080)
  -h, --help         Show this help
  -V, --version      Show the version

Both --option VALUE and --option=VALUE are supported.
Existing output files are overwritten. No window is opened.
Requires a compatible GPU; MP4 export also requires FFmpeg with libx264."
    )
}

pub fn parse(args: impl IntoIterator<Item = OsString>, duration: f32) -> Result<Request, String> {
    let args: Vec<_> = args.into_iter().collect();
    if args.len() == 1 {
        match args[0].to_str() {
            Some("--help" | "-h") => return Ok(Request::Help),
            Some("--version" | "-V") => return Ok(Request::Version),
            _ => {}
        }
    }
    let mut options = Options {
        path: "homogeneous-coordinates.mp4".into(),
        still: false,
        seconds: 0.,
        fps: 30,
        width: 1920,
        height: 1080,
    };
    let mut seen = HashSet::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let arg = arg.to_str().ok_or("option names must be valid UTF-8")?;
        let (flag, inline) = arg
            .split_once('=')
            .map_or((arg, None), |(a, b)| (a, Some(b)));
        match flag {
            "--output" | "--still" | "--time" | "--fps" | "--width" | "--height" => {}
            "--help" | "-h" | "--version" | "-V" => return Err(format!("use {flag} on its own")),
            _ => return Err(format!("unknown argument '{flag}'")),
        }
        if !seen.insert(flag.to_owned()) {
            return Err(format!("{flag} was supplied more than once"));
        }
        let value = match inline {
            Some(value) => OsString::from(value),
            None => args
                .next()
                .ok_or_else(|| format!("{flag} requires a value"))?,
        };
        if value.is_empty() || value.to_str().is_some_and(|s| s.starts_with("--")) {
            return Err(format!("{flag} requires a value"));
        }
        match flag {
            "--output" | "--still" => {
                options.path = value.into();
                options.still = flag == "--still";
            }
            "--time" => {
                let time = value.to_str().and_then(|s| s.parse::<f32>().ok())
                    .filter(|s| s.is_finite() && *s >= 0. && *s < duration)
                    .ok_or_else(|| format!("--time must be a finite number from 0 up to, but not including, {duration}"))?;
                options.seconds = time;
            }
            "--fps" => options.fps = integer(flag, &value, 12, 60, false)?,
            "--width" => options.width = integer(flag, &value, 640, 3840, true)?,
            "--height" => options.height = integer(flag, &value, 360, 2160, true)?,
            _ => unreachable!(),
        }
    }
    if seen.contains("--output") && seen.contains("--still") {
        return Err("--output and --still cannot be used together".into());
    }
    if seen.contains("--time") && !options.still {
        return Err("--time requires --still".into());
    }
    let extension = if options.still { "png" } else { "mp4" };
    if !options
        .path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case(extension))
    {
        return Err(format!("output filename must end in .{extension}"));
    }
    Ok(Request::Render(options))
}

fn integer(flag: &str, value: &OsString, min: u32, max: u32, even: bool) -> Result<u32, String> {
    value
        .to_str()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|n| (min..=max).contains(n) && (!even || n % 2 == 0))
        .ok_or_else(|| {
            format!(
                "{flag} must be {}integer from {min} to {max}",
                if even { "an even " } else { "an " }
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Result<Request, String> {
        parse(values.iter().map(OsString::from), 42.5)
    }
    #[test]
    fn defaults_and_valid_overrides() {
        assert_eq!(
            args(&[]),
            Ok(Request::Render(Options {
                path: "homogeneous-coordinates.mp4".into(),
                still: false,
                seconds: 0.,
                fps: 30,
                width: 1920,
                height: 1080,
            }))
        );
        assert_eq!(
            args(&[
                "--still=frame.png",
                "--time",
                "16.8",
                "--fps=60",
                "--width",
                "3840",
                "--height=2160"
            ]),
            Ok(Request::Render(Options {
                path: "frame.png".into(),
                still: true,
                seconds: 16.8,
                fps: 60,
                width: 3840,
                height: 2160
            }))
        );
    }
    #[test]
    fn invalid_numbers_are_rejected_instead_of_changed() {
        for pair in [
            ["--fps", "abc"],
            ["--fps", "11"],
            ["--fps", "61"],
            ["--width", "641"],
            ["--width", "4000"],
            ["--height", "0"],
            ["--height", "361"],
        ] {
            assert!(args(&pair).is_err(), "{pair:?}");
        }
        for value in ["NaN", "inf", "-1", "42.5", "not-a-number"] {
            assert!(args(&["--still", "frame.png", "--time", value]).is_err());
        }
    }
    #[test]
    fn malformed_arguments_and_conflicts_are_rejected() {
        for values in [
            vec!["--bogus"],
            vec!["movie.mp4"],
            vec!["--width"],
            vec!["--width="],
            vec!["--output", "--fps", "30"],
            vec!["--fps", "30", "--fps", "60"],
            vec!["--still", "a.png", "--output", "b.mp4"],
            vec!["--output", "a.mp4", "--still", "b.png"],
            vec!["--time", "1"],
            vec!["--still", "a.mp4"],
            vec!["--output", "a.png"],
            vec!["--help", "--bogus"],
        ] {
            assert!(args(&values).is_err(), "{values:?}");
        }
    }
    #[test]
    fn informational_commands_need_no_renderer() {
        assert_eq!(args(&["-h"]), Ok(Request::Help));
        assert_eq!(args(&["--help"]), Ok(Request::Help));
        assert_eq!(args(&["-V"]), Ok(Request::Version));
        assert_eq!(args(&["--version"]), Ok(Request::Version));
    }
}
