mod config;
mod light;
mod packet;
mod util;
use anyhow::{anyhow, Context, Result};
use clap::{App, AppSettings, Arg, ArgMatches};
use futures::StreamExt;
use std::net::SocketAddr;

pub use config::*;
pub use light::*;
pub use packet::*;
pub use util::*;

pub const LIFX_PORT: u16 = 56700;

const DEVICE: &str = "device";
const TIMEOUT: &str = "timeout";
const DURATION: &str = "duration";
const DISCOVER: &str = "discover";
const POWER: &str = "power";
const TOGGLE: &str = "toggle";
const LABEL: &str = "label";
const BRIGHTNESS: &str = "brightness";
const COLOR: &str = "color";

#[tokio::main]
async fn main() -> Result<()> {
    // Load config
    let config = Config::load()
        .await
        .context("Unable to parse configuration file")?;

    let device_arg = Arg::new(DEVICE)
        .about("Address or alias of device to control")
        .env("LIFXC_DEVICE")
        .long("device")
        .short('d')
        .takes_value(true);

    let matches = App::new("lifxc")
        .version("0.1.0")
        .author("Harrison Rigg <riggh@icloud.com>")
        .about("Command line utility for controlling LIFX smart lights")
        .global_setting(AppSettings::HelpRequired)
        .global_setting(AppSettings::InferSubcommands)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .setting(AppSettings::DisableHelpSubcommand)
        .subcommand(
            App::new(DISCOVER)
                .about("Discover devices on your local network")
                .arg(
                    Arg::new(TIMEOUT)
                        .about("Timeout (in milliseconds) for device discovery")
                        .long("timeout")
                        .short('t')
                        .default_value("1000"),
                ),
        )
        .subcommand(
            App::new(LABEL)
                .about("Get or set label of the specified device")
                .arg(&device_arg)
                .arg(
                    Arg::new(LABEL)
                        .about("Label to assign to device")
                        .long("set")
                        .takes_value(true),
                ),
        )
        .subcommand(
            App::new(POWER)
                .about("Get or set power state of the specified device")
                .arg(&device_arg)
                .arg(
                    Arg::new(POWER)
                        .about("Power state to set device")
                        .long("set")
                        .possible_values(["on", "off"]),
                ),
        )
        .subcommand(
            App::new(TOGGLE)
                .about("Toggle power state of the specified device")
                .arg(&device_arg),
        )
        .subcommand(
            App::new(BRIGHTNESS)
                .about("Get or set brightness of the specified device")
                .arg(&device_arg)
                .args(&[
                    Arg::new(BRIGHTNESS)
                        .about("Brightness (in percent) to set device")
                        .long("set")
                        .takes_value(true),
                    Arg::new(DURATION)
                        .about("Duration (in milliseconds) of brightness transition")
                        .long("duration")
                        .requires(BRIGHTNESS)
                        .takes_value(true),
                ]),
        )
        .subcommand(
            App::new(COLOR)
                .about("Get or set color of the specified device")
                .arg(&device_arg)
                .args(&[
                    Arg::new("hue")
                        .about("Hue (in degrees) to set device")
                        .long("hue")
                        .takes_value(true),
                    Arg::new("saturation")
                        .about("Saturation (in percent) to set device")
                        .long("saturation")
                        .takes_value(true),
                    Arg::new("brightness")
                        .about("Brightness (in percent) to set device")
                        .long("brightness")
                        .takes_value(true),
                    Arg::new("kelvin")
                        .about("Color temperature (in kelvin) to set device")
                        .long("kelvin")
                        .takes_value(true),
                    Arg::new(DURATION)
                        .about("Duration (in milliseconds) of color transition")
                        .long("duration")
                        .takes_value(true),
                ]),
        )
        .get_matches();

    match matches.subcommand() {
        Some((DISCOVER, sm)) => {
            let device_stream = LightConnection::device_stream().await?;
            let fut = device_stream.for_each(|d| async move {
                let mut conn = LightConnection::new(d).await.unwrap();
                let (_, _, _, _, _, label) = conn.get_state().await.unwrap();
                println!("Found device: {} {}", label, d);
            });

            let tm = sm.value_of_t(TIMEOUT).unwrap();
            let _ = timeout(fut, tm).await;
        }
        Some((LABEL, sm)) => {
            let device = find_device(&config, sm)?;
            let mut conn = LightConnection::new(device).await?;

            if let Some(label) = sm.value_of(LABEL) {
                conn.set_label(label).await?;
                println!("Success");
            } else {
                let label = conn.get_label().await?;
                println!("{}", label);
            }
        }
        Some((POWER, sm)) => {
            let device = find_device(&config, sm)?;
            let mut conn = LightConnection::new(device).await?;

            if let Some(power) = sm.value_of(POWER) {
                conn.set_power(power == "on").await?;
            } else {
                let power = conn.get_power().await?;
                println!("{}", if power { "on" } else { "off" });
            }
        }
        Some((TOGGLE, sm)) => {
            let device = find_device(&config, sm)?;
            let mut conn = LightConnection::new(device).await?;

            let power = conn.get_power().await?;
            conn.set_power(!power).await?;
        }
        Some((BRIGHTNESS, sm)) => {
            let device = find_device(&config, sm)?;
            let mut conn = LightConnection::new(device).await?;

            if let Ok(b) = sm.value_of_t::<f32>(BRIGHTNESS) {
                let (h, s, _, k, ..) = conn.get_state().await?;
                let b = (b / 100.0) * 0x10000 as f32;
                let duration = sm
                    .value_of(DURATION)
                    .map(|d| d.parse::<u32>())
                    .transpose()?
                    .unwrap_or(0);
                conn.set_color(h, s, b as u16, k, duration).await?;
            } else {
                let (_, _, b, ..) = conn.get_state().await?;
                println!("{:.1}%", 100.0 * b as f32 / 0x10000 as f32);
            }
        }
        Some((COLOR, sm)) => {
            let device = find_device(&config, sm)?;
            let mut conn = LightConnection::new(device).await?;

            let hue = sm.value_of("hue").map(|h| h.parse::<f32>()).transpose()?;
            let saturation = sm
                .value_of("saturation")
                .map(|s| s.parse::<f32>())
                .transpose()?;
            let brightness = sm
                .value_of("brightness")
                .map(|b| b.parse::<f32>())
                .transpose()?;
            let kelvin = sm
                .value_of("kelvin")
                .map(|k| k.parse::<u16>())
                .transpose()?;

            if hue.is_some() || saturation.is_some() || brightness.is_some() || kelvin.is_some() {
                let (mut h, mut s, mut b, mut k, ..) = conn.get_state().await?;

                if let Some(hue) = hue {
                    h = (hue * 0x10000 as f32 / 360.0) as u16;
                }
                if let Some(saturation) = saturation {
                    s = (saturation * 0x10000 as f32 / 100.0) as u16;
                }
                if let Some(brightness) = brightness {
                    b = (brightness * 0x10000 as f32 / 100.0) as u16;
                }
                if let Some(kelvin) = kelvin {
                    k = kelvin;
                }

                let duration = sm
                    .value_of(DURATION)
                    .map(|d| d.parse::<u32>())
                    .transpose()?
                    .unwrap_or(0);

                conn.set_color(h, s, b, k, duration).await?;
            } else {
                let (h, s, b, k, ..) = conn.get_state().await?;
                println!("Hue: {:.1}", 360.0 * h as f32 / 0x10000 as f32);
                println!("Saturation: {:.1}%", 100.0 * s as f32 / 0x10000 as f32);
                println!("Brightness: {:.1}%", 100.0 * b as f32 / 0x10000 as f32);
                println!("Kelvin: {}", k);
            }
        }
        _ => (),
    }

    Ok(())
}

fn find_device(config: &Config, matches: &ArgMatches) -> Result<SocketAddr> {
    if let Some(device) = matches.value_of(DEVICE) {
        // Passed as argument or environment variable
        if let Some(addr) = config.find_alias(device) {
            Ok(addr)
        } else {
            parse_address(device)
                .ok_or_else(|| anyhow!("Device is neither a valid IP address or alias."))
        }
    } else if let Some(device) = config.default_device {
        Ok(device)
    } else {
        // No device set
        Err(anyhow!("No device address specified."))
    }
}
