<div align="center">

# lifxc  

lifxc is a command line utility for controlling LIFX smart lights.
Currently, communication over the LIFX LAN protocol is supported.
</div>

## Usage
Discover devices on your local network:
```
$ lifxc discover
```

After determining the IP address of your devices, you can individually control
them by providing the `device` argument:
```
$ lifxc brightness --set 50 --device <ADDRESS>
```

To control the color of your devices, use the `color` subcommand:
```
$ lifxc color --hue 270 --saturation 100 --duration 3000
```

See `lifxc --help` for a complete list of commands.

## Configuration
On Linux, the lifxc configuration file is located at `~/.config/lifxc/config.toml`.  
Example configuration file:
```toml
default_device = "office"

# default_device may also be set to an IP address

[[devices]]
alias = "office"
address = "192.168.0.4"

[[devices]]
alias = "kitchen"
address = "192.168.0.6"
```

## Building
Building lifxc requires a stable rust installation.
To build lifxc:
```
$ git clone https://github.com/psr31/lifxc
$ cd lifxc
$ cargo build --release
$ ./target/release/lifxc
```
