use std::{thread, time};
use std::path::{Path};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_derive::Deserialize;
use signal_hook;

mod amdgpu;
mod control;
use control::ControlCurve;

fn main() {
    match run() {
        Err(err) => {
            println!("Error: {}", err);
            std::process::exit(1)
        },
        Ok(_) => std::process::exit(0),
    }
}

fn run() -> Result<(), Error> {
    let config_files = vec![
        "amdgpu-fan.toml",
        "/etc/amdgpu-fan.toml",
    ];
    let config = load_config(config_files.iter())?;

    println!("Card: {}", config.control.card_path.display());
    println!("Poll: {}ms", config.control.poll_interval_millis);

    let mut hwmons = amdgpu::Hwmon::for_device(config.control.card_path)?;
    let mut device = hwmons.pop().expect("There should be at least one `hwmon` directory");

    let exit = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::SIGTERM, Arc::clone(&exit))?;
    signal_hook::flag::register(signal_hook::SIGINT, Arc::clone(&exit))?;

    println!("Enabling manual fan control");
    device.set_pwm_mode(amdgpu::PwmMode::Manual)?;

    let curve = config.curve.to_curve();
    let poll_interval = time::Duration::from_millis(config.control.poll_interval_millis);

    let result = control_loop(&mut device, poll_interval, &curve, exit);

    if let Err(_) = &result {
        println!("Control loop aborted");
    } else {
        println!("Control loop stopped");
    }

    if let Err(err) = device.set_pwm_mode(amdgpu::PwmMode::Automatic) {
        println!("CRITICAL: could not restore automatic fan control");
        println!("{}", err);
    } else {
        println!("Automatic fan control restored");
    }

    result.map_err(Into::into)
}

fn control_loop(device: &mut amdgpu::Hwmon, poll_interval: time::Duration, curve: &ControlCurve, exit_var: Arc<AtomicBool>) -> Result<(), amdgpu::GpuError> {
    while !exit_var.load(Ordering::Relaxed) {
        let temperature_celcius = device.get_temperature()?.as_celcius();
        let fan_speed_relative = curve.control(temperature_celcius);
        let fan_speed_pwm = amdgpu::Pwm::from_percentage(device.get_pwm_min(), device.get_pwm_max(), fan_speed_relative)?;

        println!("T_cur={: >5.1}°C\tV_rel={: >5.1}%\tV_pwm={: >3}", temperature_celcius, fan_speed_relative * 100.0, fan_speed_pwm.as_raw());

        device.set_pwm(fan_speed_pwm)?;

        thread::sleep(poll_interval);
    }
    Ok(())
}

pub enum Error {
    ConfigParse(toml::de::Error),
    ConfigIo(std::io::Error),
    Control(amdgpu::GpuError),
    ConfigurationMissing,
    InvalidCurve,
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::ConfigIo(err)
    }
}

impl From<toml::de::Error> for Error {
    fn from(err: toml::de::Error) -> Error {
        Error::ConfigParse(err)
    }
}

impl From<amdgpu::GpuError> for Error {
    fn from(err: amdgpu::GpuError) -> Error {
        Error::Control(err)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self {
            &Error::ConfigIo(err) => write!(f, "{}", err),
            &Error::ConfigParse(err) => write!(f, "{}", err),
            &Error::Control(err) => write!(f, "{}", err),
            &Error::InvalidCurve => write!(f, "Curve definition must contain at least one entry, and an equal number of temperatures and fan speeds."),
            &Error::ConfigurationMissing => write!(f, "No valid configuration file found"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Config {
    curve: CurveConfig,
    control: ControlConfig,
}

#[derive(Debug, Deserialize)]
struct CurveConfig {
    temperatures: Vec<f64>,
    fan_speeds: Vec<f64>,
}

impl CurveConfig {
    fn to_curve(&self) -> ControlCurve {
        ControlCurve::new(
            self.temperatures.iter().cloned()
            .zip(self.fan_speeds.iter().cloned())
            .collect::<Vec<_>>()
        )
    }
}

#[derive(Debug, Deserialize)]
struct ControlConfig {
    card_path: std::path::PathBuf,
    poll_interval_millis: u64,
}

fn load_config<I, P>(paths_to_check: I) -> Result<Config, Error> where
    I: Iterator<Item=P>,
    P: AsRef<Path>
{
    paths_to_check
    .map(|path| {
        let cfg_result = load_config_file(path.as_ref());
        (path, cfg_result)
    })
    .find_map(|(path, cfg)| match cfg {
        Ok(cfg) => {
            println!("{}: loaded", path.as_ref().display());
            Some(cfg)
        },
        Err(no_cfg) => {
            println!("{}: {}", path.as_ref().display(), no_cfg);
            None
        }
    })
    .ok_or(Error::ConfigurationMissing)
}

fn load_config_file(path: &Path) -> Result<Config, Error> {
    let contents = std::fs::read_to_string(path)?;
    let config = toml::from_str::<Config>(contents.as_ref())?;
    if config.curve.temperatures.len() != config.curve.fan_speeds.len()
        || config.curve.temperatures.is_empty() {
        Err(Error::InvalidCurve)
    } else {
        Ok(config)
    }
}