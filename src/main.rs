mod cli;
mod film;
mod math;
mod scene;

use bevy::{
    app::ScheduleRunnerPlugin,
    camera::RenderTarget,
    prelude::*,
    render::{
        render_resource::{TextureFormat, TextureUsages},
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    window::ExitCondition,
    winit::WinitPlugin,
};
use bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext};
use std::{
    io::Write,
    path::PathBuf,
    process::{Child, ChildStdin, Command, ExitCode, Stdio},
    time::Duration,
};

#[derive(Resource)]
struct Export {
    path: PathBuf,
    still: bool,
    fps: u32,
    width: u32,
    height: u32,
    first: u32,
    count: u32,
    frame: u32,
    wait: u8,
    in_flight: bool,
    target: Handle<Image>,
    encoder: Option<Child>,
    pipe: Option<ChildStdin>,
}
fn main() -> ExitCode {
    match cli::parse(std::env::args_os().skip(1), film::total_seconds()) {
        Ok(cli::Request::Help) => {
            println!("{}", cli::help(film::total_seconds()));
            ExitCode::SUCCESS
        }
        Ok(cli::Request::Version) => {
            println!("one-dimension-up {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(cli::Request::Render(options)) => match run(options) {
            Ok(exit) if exit.is_success() => ExitCode::SUCCESS,
            Ok(_) => ExitCode::FAILURE,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("error: {error}\nTry '--help' for usage.");
            ExitCode::from(2)
        }
    }
}
fn run(options: cli::Options) -> std::result::Result<AppExit, String> {
    let cli::Options {
        path,
        still,
        seconds,
        fps,
        width,
        height,
    } = options;
    let path = std::path::absolute(&path)
        .map_err(|e| format!("cannot resolve output path '{}': {e}", path.display()))?;
    if path.is_dir() {
        return Err(format!("output '{}' is a directory", path.display()));
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create output directory '{}': {e}", parent.display()))?;
    }
    let first = if still {
        (seconds * fps as f32) as u32
    } else {
        0
    };
    let mut frame = film::Frame::default();
    frame.seek(first, fps);
    let mut export = Export {
        path,
        still,
        fps,
        width,
        height,
        first,
        count: if still { 1 } else { film::total_frames(fps) },
        frame: 0,
        wait: 120,
        in_flight: false,
        target: default(),
        encoder: None,
        pipe: None,
    };
    if !still {
        export.start_encoder()?;
    }
    Ok(App::new()
        .insert_resource(ClearColor(Color::srgb_u8(12, 17, 26)))
        .insert_resource(frame)
        .insert_resource(export)
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .disable::<WinitPlugin>(),
        )
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_millis(2)))
        .add_plugins(EguiPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(EguiPrimaryContextPass, film::render)
        .add_systems(Update, capture_next)
        .run())
}
fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut export: ResMut<Export>,
    mut settings: ResMut<EguiGlobalSettings>,
) {
    settings.auto_create_primary_context = false;
    let mut image = Image::new_target_texture(
        export.width,
        export.height,
        TextureFormat::Rgba8UnormSrgb,
        None,
    );
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let target = images.add(image);
    export.target = target.clone();
    commands.spawn((
        Camera2d,
        RenderTarget::Image(target.into()),
        PrimaryEguiContext,
    ));
    eprintln!(
        "Rendering {} frames at {}x{} to {}",
        export.count,
        export.width,
        export.height,
        export.path.display()
    );
}
impl Export {
    fn start_encoder(&mut self) -> std::result::Result<(), String> {
        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgb24",
                "-video_size",
                &format!("{}x{}", self.width, self.height),
                "-framerate",
                &self.fps.to_string(),
                "-i",
                "pipe:0",
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ])
            .arg(&self.path)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| {
                format!(
                    "cannot start FFmpeg: {e}. Install FFmpeg with libx264 and ensure it is on PATH"
                )
            })?;
        self.pipe = child.stdin.take();
        self.encoder = Some(child);
        Ok(())
    }

    fn fail(&mut self, message: String, exit: &mut MessageWriter<AppExit>) {
        eprintln!("error: {message}");
        self.in_flight = true;
        self.stop_encoder();
        exit.write(AppExit::error());
    }
    fn stop_encoder(&mut self) {
        drop(self.pipe.take());
        if let Some(mut child) = self.encoder.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Drop for Export {
    fn drop(&mut self) {
        self.stop_encoder();
    }
}
fn capture_next(mut commands: Commands, mut export: ResMut<Export>) {
    if export.in_flight {
        return;
    }
    if export.wait > 0 {
        export.wait -= 1;
        return;
    }
    export.in_flight = true;
    commands
        .spawn(Screenshot::image(export.target.clone()))
        .observe(receive_frame);
}
fn receive_frame(
    event: On<ScreenshotCaptured>,
    mut export: ResMut<Export>,
    mut frame: ResMut<film::Frame>,
    mut exit: MessageWriter<AppExit>,
) {
    let image = match event.image.clone().try_into_dynamic() {
        Ok(image) => image.to_rgb8(),
        Err(error) => {
            export.fail(format!("cannot read rendered frame: {error}"), &mut exit);
            return;
        }
    };
    let result = if export.still {
        image
            .save(&export.path)
            .map_err(|e| format!("cannot save '{}': {e}", export.path.display()))
    } else {
        export
            .pipe
            .as_mut()
            .ok_or_else(|| "FFmpeg input is unavailable".to_owned())
            .and_then(|pipe| {
                pipe.write_all(image.as_raw()).map_err(|e| {
                    format!("cannot send frame to FFmpeg: {e}; check the encoder output above")
                })
            })
    };
    if let Err(error) = result {
        export.fail(error, &mut exit);
        return;
    }
    export.frame += 1;
    if frame.progress >= 1. {
        eprintln!("Completed {}", film::CHAPTERS[frame.chapter]);
    }
    if export.frame >= export.count {
        drop(export.pipe.take());
        if let Some(mut encoder) = export.encoder.take() {
            match encoder.wait() {
                Ok(status) if status.success() => {}
                Ok(status) => {
                    export.fail(
                        format!("FFmpeg exited with {status}; check the encoder output above"),
                        &mut exit,
                    );
                    return;
                }
                Err(error) => {
                    export.fail(format!("cannot finish FFmpeg export: {error}"), &mut exit);
                    return;
                }
            }
        }
        eprintln!("Saved {}", export.path.display());
        exit.write(AppExit::Success);
    } else {
        frame.seek(export.first + export.frame, export.fps);
        export.wait = 2; // Let the next exact frame reach the render world before readback.
        export.in_flight = false;
    }
}
