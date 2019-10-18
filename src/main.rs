use std::convert::{TryFrom, TryInto};
use std::fs;
use std::io::{self, Read};

extern crate clap;

use clap::{App, Arg};
use console::Term;
use csv;
use geo_types::{Geometry, Point};
use geojson::{self, GeoJson};
use indicatif::ProgressBar;
use rstar::RTree;
use shapefile;
use topojson::{to_geojson, TopoJson};

pub mod map_grid;
use map_grid::{GridGeom, MapGrid};

/// Read file path (or stdin) to string
fn read_input_to_string(file_path: &str) -> String {
    let mut input_str = String::new();
    match file_path {
        "-" => {
            io::stdin()
                .read_to_string(&mut input_str)
                .expect("There was an error reading from stdin");
        }
        _ => {
            fs::File::open(file_path)
                .unwrap()
                .read_to_string(&mut input_str)
                .expect("There was an error reading your file");
        }
    };
    input_str
}

/// Simplify geometries into component pieces for GridGeom
fn convert_geometry(geom: Geometry<f64>, is_area: bool) -> Vec<GridGeom<f64>> {
    match geom {
        Geometry::Point(s) => vec![GridGeom::Point(s)],
        Geometry::MultiPoint(s) => s.into_iter().map(GridGeom::Point).collect(),
        Geometry::Line(s) => vec![GridGeom::Line(s)],
        Geometry::LineString(s) => s.lines().map(GridGeom::Line).collect(),
        Geometry::MultiLineString(s) => s
            .into_iter()
            .flat_map(|ls| ls.lines().collect::<Vec<_>>())
            .map(GridGeom::Line)
            .collect(),
        Geometry::Polygon(s) => {
            if is_area {
                vec![GridGeom::Polygon(s)]
            } else {
                s.exterior().lines().map(GridGeom::Line).collect()
            }
        }
        Geometry::MultiPolygon(s) => {
            if is_area {
                s.into_iter().map(GridGeom::Polygon).collect()
            } else {
                s.into_iter()
                    .flat_map(|p| p.exterior().lines().collect::<Vec<_>>())
                    .map(GridGeom::Line)
                    .collect()
            }
        }
        Geometry::GeometryCollection(s) => s
            .into_iter()
            .flat_map(|g| convert_geometry(g, is_area))
            .collect(),
    }
}

/// Process top-level GeoJSON items
fn process_geojson(gj: GeoJson, is_area: bool) -> Vec<GridGeom<f64>> {
    match gj {
        GeoJson::FeatureCollection(collection) => collection
            .features
            .into_iter()
            .filter_map(|feature| feature.geometry)
            .flat_map(|g| {
                let geom: Geometry<f64> = g.value.try_into().unwrap();
                convert_geometry(geom, is_area)
            })
            .collect(),
        GeoJson::Feature(feature) => {
            if let Some(geometry) = feature.geometry {
                let geom: Geometry<f64> = geometry.value.try_into().unwrap();
                convert_geometry(geom, is_area)
            } else {
                vec![]
            }
        }
        GeoJson::Geometry(geometry) => {
            let geom: Geometry<f64> = geometry.value.try_into().unwrap();
            convert_geometry(geom, is_area)
        }
    }
}

fn main() {
    let matches = App::new("echomap")
        .version("0.2.4")
        .about("Preview map files in the terminal")
        .author("Pat Sier <pjsier@gmail.com>")
        .arg(Arg::with_name("INPUT")
            .help("File to parse or '-' to read stdin")
            .required(true)
            .index(1))
        .arg(Arg::with_name("format")
            .short("f")
            .long("format")
            .value_name("FORMAT")
            .help("Input file format (tries to infer from file extension by default)")
            .possible_values(&["geojson", "topojson", "csv", "shp"])
            .default_value_if("INPUT", Some("-"), "geojson")
            .takes_value(true))
        .arg(Arg::with_name("lon")
            .long("lon")
            .value_name("LON")
            .takes_value(true)
            .help("Name of longitude column (if format is 'csv')")
            .default_value_if("format", Some("csv"), "lon"))
        .arg(Arg::with_name("lat")
            .long("lat")
            .value_name("LAT")
            .takes_value(true)
            .help("Name of latitude column (if format is 'csv')")
            .default_value_if("format", Some("csv"), "lat"))
        .arg(Arg::with_name("rows")
            .short("r")
            .long("rows")
            .value_name("ROWS")
            .help("Sets the number of rows (in characters) of the printed output. Defaults to terminal width.")
            .takes_value(true))
        .arg(Arg::with_name("columns")
            .short("c")
            .long("columns")
            .value_name("COLUMNS")
            .help("Sets the number of columns (in characters) of the printed output. Defaults to terminal height minus 1.")
            .takes_value(true))
        .arg(Arg::with_name("area")
            .short("a")
            .long("area")
            .help("Print polygon area instead of boundaries"))
        .get_matches();

    let spinner = ProgressBar::new_spinner();
    spinner.set_message("Reading file");
    spinner.enable_steady_tick(1);
    spinner.set_message("Parsing geography");

    // Get file format from flag or infer from file path
    let file_format = match matches.value_of("format") {
        Some(f) => String::from(f),
        _ => matches
            .value_of("INPUT")
            .unwrap()
            .split('.')
            .last()
            .unwrap()
            .to_ascii_lowercase(),
    };

    let geoms: Vec<GridGeom<f64>> = match file_format.as_ref() {
        "geojson" => {
            let input_str = read_input_to_string(matches.value_of("INPUT").unwrap());
            let gj: GeoJson = input_str
                .parse::<GeoJson>()
                .expect("Unable to parse GeoJSON");
            process_geojson(gj, matches.is_present("area"))
        }
        "topojson" => {
            let input_str = read_input_to_string(matches.value_of("INPUT").unwrap());
            let topo = input_str
                .parse::<TopoJson>()
                .expect("Unable to parse TopoJSON");
            match topo {
                TopoJson::Topology(t) => t
                    .list_names()
                    .into_iter()
                    .map(|n| to_geojson(&t, &n))
                    .filter_map(|g| g.ok())
                    .map(GeoJson::FeatureCollection)
                    .flat_map(|g| process_geojson(g, matches.is_present("area")))
                    .collect(),
                _ => unimplemented!(),
            }
        }
        "csv" => {
            let input_str = read_input_to_string(matches.value_of("INPUT").unwrap());
            let mut rdr = csv::Reader::from_reader(input_str.as_bytes());
            let headers = rdr.headers().expect("Unable to load CSV headers");
            let lat_col = matches.value_of("lat").unwrap();
            let lon_col = matches.value_of("lon").unwrap();

            let lat_idx = headers
                .iter()
                .position(|v| v == lat_col)
                .expect("Lat column not found");
            let lon_idx = headers
                .iter()
                .position(|v| v == lon_col)
                .expect("Lon column not found");

            rdr.records()
                .map(|rec_val| {
                    let rec = rec_val.unwrap();
                    let lat_val: f64 = rec
                        .get(lat_idx)
                        .unwrap()
                        .parse()
                        .expect("Could not parse lat value from record");
                    let lon_val: f64 = rec
                        .get(lon_idx)
                        .unwrap()
                        .parse()
                        .expect("Could not parse lon value from record");
                    let pt: Point<f64> = Point::new(lon_val, lat_val);
                    GridGeom::Point(pt)
                })
                .collect()
        }
        "shp" => {
            let rdr = shapefile::Reader::from_path(matches.value_of("INPUT").unwrap())
                .expect("There was an error opening the shapefile");
            rdr.iter_shapes()
                .filter_map(|s| s.ok())
                .flat_map(|s| match Geometry::<f64>::try_from(s) {
                    Ok(geom) => convert_geometry(geom, matches.is_present("area")),
                    Err(_) => vec![],
                })
                .collect()
        }
        _ => panic!("Invalid format supplied"),
    };

    // Create a combined LineString for bounds calculation
    spinner.set_message("Indexing geography");
    let rtree: RTree<GridGeom<f64>> = RTree::bulk_load(geoms);

    let (term_height, term_width) = Term::stdout().size();
    let height: f64 = match matches.value_of("rows") {
        Some(ref rows) => rows.parse().expect("Rows cannot be parsed as a number."),
        None => f64::from(term_height - 1),
    };
    let width: f64 = match matches.value_of("columns") {
        Some(ref cols) => cols.parse().expect("Columns cannot be parsed as a number."),
        None => f64::from(term_width),
    };
    let grid = MapGrid::new(width, height, rtree);
    spinner.finish_and_clear();
    grid.print();
}
