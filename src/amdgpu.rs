//! A module for controlling the fan speed of AMD GPUs.

use std::string::String;
use std::path::{PathBuf, Path};
use std::{io, fs, fmt};
use std::io::{BufRead};

#[derive(Debug)]
pub enum GpuError {
    /// IO failure while accessing the GPU device files
    Io(io::Error),
    /// Unexpected data has been read from a GPU device file
    Parse(PathBuf, Option<String>),
    InvalidFanSpeed { min: Pwm, max: Pwm, percentage: f64 },
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            &GpuError::Io(err) => write!(f, "{}", err),
            &GpuError::Parse(path, contents) => write!(f, "Could not parse {:?} from {}", contents, path.to_string_lossy()),
            &GpuError::InvalidFanSpeed {min, max, percentage} => write!(f, "Computation resulted in invalid fan speed (min={:?} max={:?} percentage={})", min, max, percentage),
        }
    }
}

impl From<io::Error> for GpuError {
    fn from(other: io::Error) -> GpuError {
        GpuError::Io(other)
    }
}

/// A GPU temperature in raw units (in thousandths of a degree celcius)
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub struct Temperature(i32);

impl Temperature {
    pub fn as_celcius(self) -> f64 {
        self.0 as f64 / 1000.0
    }
}

impl fmt::Display for Temperature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}°C", self.as_celcius())
    }
}

/// GPU fan PWM value. The exact meaning depends on the min/max values that may or may not vary.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub struct Pwm(i32);

impl Pwm {
    pub fn as_raw(self) -> i32 {
        self.0
    }

    pub fn from_percentage(min: Pwm, max: Pwm, percentage: f64) -> Result<Pwm, GpuError> {
        let actual_percentage = percentage.min(1.0).max(0.0);
        let pwm_float = min.0 as f64 + (max.0 as f64 - min.0 as f64) * actual_percentage;
        if pwm_float.is_finite() {
            let pwm = pwm_float.floor() as i32;
            Ok(Pwm(pwm))
        } else {
            Err(GpuError::InvalidFanSpeed { min, max, percentage })
        }
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum PwmMode {
    Manual,
    Automatic
}

pub struct Hwmon {
    path_temperature: PathBuf,
    path_pwm_enable: PathBuf,
    path_pwm: PathBuf,
    pwm_min: Pwm,
    pwm_max: Pwm,
}

impl Hwmon {
    pub fn for_device<P: AsRef<Path>>(device_path: P) -> Result<Vec<Hwmon>, GpuError> {
        let mut result = Vec::new();
        let hwmons = device_path.as_ref().join("hwmon");
        for entry in fs::read_dir(&hwmons)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let hwmon = Hwmon::new(path)?;
                result.push(hwmon);
            }
        }
        Ok(result)
    }

    pub fn new<P: AsRef<Path>>(hwmon_path: P) -> Result<Self, GpuError> {
        let pwm_min_path = hwmon_path.as_ref().join("pwm1_min");
        let pwm_max_path = hwmon_path.as_ref().join("pwm1_max");
        let pwm_min_raw = Self::read_value(&pwm_min_path)?;
        let pwm_max_raw = Self::read_value(&pwm_max_path)?;

        Ok(Hwmon {
            path_temperature: hwmon_path.as_ref().join("temp1_input"),
            path_pwm_enable: hwmon_path.as_ref().join("pwm1_enable"),
            path_pwm: hwmon_path.as_ref().join("pwm1"),
            pwm_min: Pwm(pwm_min_raw),
            pwm_max: Pwm(pwm_max_raw),
        })
    }

    pub fn get_temperature(&self) -> Result<Temperature, GpuError> {
        let temp_raw = Self::read_value(&self.path_temperature)?;
        Ok(Temperature(temp_raw))
    }

    pub fn get_pwm_min(&self) -> Pwm {
        self.pwm_min
    }

    pub fn get_pwm_max(&self) -> Pwm {
        self.pwm_max
    }

    pub fn set_pwm_mode(&mut self, mode: PwmMode) -> Result<(), GpuError> {
        let value = match mode {
            PwmMode::Automatic => "2",
            PwmMode::Manual => "1",
        };
        Self::write_value(&self.path_pwm_enable, value)
    }

    pub fn set_pwm(&mut self, value: Pwm) -> Result<(), GpuError> {
        let value_str = format!("{}\n", value.0);
        Self::write_value(&self.path_pwm, &value_str)
    }

    fn read_value<P: AsRef<Path>, V: std::str::FromStr>(path: P) -> Result<V, GpuError> {
        let file = fs::File::open(path.as_ref())?;
        let reader = io::BufReader::new(file);

        let contents = reader.lines().next().transpose()?;

        match contents.as_ref().map(|s| s.parse()) {
            Some(Ok(value)) => Ok(value),
            _ => Err(GpuError::Parse(path.as_ref().to_owned(), contents)),
        }
    }

    fn write_value<P: AsRef<Path>>(path: P, value: &str) -> Result<(), GpuError> {
        fs::write(path, value)?;
        Ok(())
    }
}