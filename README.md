# `echomap`

[![crates.io](https://img.shields.io/crates/v/echomap.svg)](https://crates.io/crates/echomap)
[![Build status](https://github.com/pjsier/echomap/workflows/CI/badge.svg)](https://github.com/pjsier/echomap/actions?query=workflow%3ACI)

Preview map files in the terminal

![Terminal recording gif](https://raw.githubusercontent.com/pjsier/echomap/main/img/recording.gif?raw=true)

## Installation

If you have `cargo` installed, you can run `cargo install echomap` and then run it from `$HOME/.cargo/bin`. More details on this are available in [`cargo-install` documentation](https://doc.rust-lang.org/cargo/commands/cargo-install.html).

There are also [binaries available](https://github.com/pjsier/echomap/releases) for MacOS, Windows and Linux.

## Usage

```
USAGE:
    echomap [FLAGS] [OPTIONS] <INPUT>

FLAGS:
    -a, --area       Print polygon area instead of boundaries
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -c, --columns <COLUMNS>        Sets the number of columns (in characters) of the printed output. Defaults to
                                   terminal height minus 1.
    -f, --format <FORMAT>          Input file format (tries to infer from file extension by default) [possible values:
                                   geojson, topojson, csv, shp, wkt, polyline, kml]
        --lat <LAT>                Name of latitude column (if format is 'csv')
        --lon <LON>                Name of longitude column (if format is 'csv')
        --precision <precision>    Precision value for polyline parsing
    -r, --rows <ROWS>              Sets the number of rows (in characters) of the printed output. Defaults to terminal
                                   width.
    -s, --simplify <simplify>      Proportion of removable points to remove (0-1 or 0%-100%) [default: 0.01]

ARGS:
    <INPUT>    File to parse or '-' to read stdin
```

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md)
