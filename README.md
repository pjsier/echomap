# `echomap`

Preview map files in the console

[![asciicast](https://asciinema.org/a/a3y3vFk4TOY9cvMEcrzKwOPW9.svg)](https://asciinema.org/a/a3y3vFk4TOY9cvMEcrzKwOPW9)

## Usage

```
USAGE:
    echomap [FLAGS] [OPTIONS] <INPUT>

FLAGS:
    -a, --area       Print polygon area instead of boundaries
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -c, --columns <COLUMNS>    Sets the number of columns (in characters) of the printed output. Defaults to terminal
                               height minus 1.
    -f, --format <FORMAT>      Input file format [default: geojson]  [possible values: geojson, csv]
        --lat <LAT>            
        --lon <LON>            
    -r, --rows <ROWS>          Sets the number of rows (in characters) of the printed output. Defaults to terminal
                               width.

ARGS:
    <INPUT>    File to parse or '-' to read stdin
```
