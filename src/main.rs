use std::convert::TryInto;
use std::fs;
use std::io::{self, Read};

extern crate clap;

use clap::{App, Arg};
use console::Term;
use csv;
use geo_types::{LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon, Rect};
use geojson::{GeoJson, Geometry, Value};
use indicatif::ProgressBar;
use num_traits::{Float, FromPrimitive};
use rstar::{RTree, RTreeNum};

pub mod map_grid;
use map_grid::{GridGeom, MapGrid};

/// Process GeoJSON geometries
fn match_geometry<T: Float + RTreeNum + FromPrimitive>(
    geom: Geometry,
    is_area: bool,
) -> Vec<GridGeom<T>> {
    match geom.value {
        Value::Point(_) => {
            let pt: Point<T> = geom.value.try_into().unwrap();
            vec![GridGeom::Point(pt)]
        }
        Value::MultiPoint(_) => {
            let mpt: MultiPoint<T> = geom.value.try_into().unwrap();
            mpt.into_iter().map(|p| GridGeom::Point(p)).collect()
        }
        Value::LineString(_) => {
            let ls: LineString<T> = geom.value.try_into().unwrap();
            ls.lines().map(|l| GridGeom::Line(l)).collect()
        }
        Value::MultiLineString(_) => {
            let ml: MultiLineString<T> = geom.value.try_into().unwrap();
            ml.into_iter()
                .flat_map(|ls| ls.lines().collect::<Vec<_>>())
                .map(|l| GridGeom::Line(l))
                .collect()
        }
        Value::Polygon(_) => {
            let poly: Polygon<T> = geom.value.try_into().unwrap();
            match is_area {
                true => vec![GridGeom::Polygon(poly)],
                false => poly.exterior().lines().map(|l| GridGeom::Line(l)).collect(),
            }
        }
        Value::MultiPolygon(_) => {
            let mp: MultiPolygon<T> = geom.value.try_into().unwrap();
            match is_area {
                true => mp.into_iter().map(|p| GridGeom::Polygon(p)).collect(),
                false => mp
                    .into_iter()
                    .flat_map(|geometry| geometry.exterior().lines().collect::<Vec<_>>())
                    .map(|l| GridGeom::Line(l))
                    .collect(),
            }
        }
        Value::GeometryCollection(collection) => collection
            .into_iter()
            .flat_map(|geometry| match_geometry(geometry, is_area))
            .collect(),
    }
}

/// Process top-level GeoJSON items
fn process_geojson<T: Float + RTreeNum + FromPrimitive>(
    gj: GeoJson,
    is_area: bool,
) -> Vec<GridGeom<T>> {
    match gj {
        GeoJson::FeatureCollection(collection) => collection
            .features
            .into_iter()
            .filter_map(|feature| feature.geometry)
            .flat_map(|geometry| match_geometry(geometry, is_area))
            .collect(),
        GeoJson::Feature(feature) => {
            if let Some(geometry) = feature.geometry {
                match_geometry(geometry, is_area)
            } else {
                vec![]
            }
        }
        GeoJson::Geometry(geometry) => match_geometry(geometry, is_area),
    }
}

fn main() {
    let matches = App::new("echomap")
        .version("0.1.0")
        .about("Preview map files in the console")
        .author("Pat Sier <pjsier@gmail.com>")
        .arg(Arg::with_name("INPUT")
            .help("File to parse or '-' to read stdin")
            .required(true)
            .index(1))
        .arg(Arg::with_name("format")
            .short("f")
            .long("format")
            .value_name("FORMAT")
            .help("Input file format")
            .possible_values(&["geojson", "csv"])
            .default_value("geojson")
            .takes_value(true))
        .arg(Arg::with_name("lon")
            .long("lon")
            .value_name("LON")
            .takes_value(true)
            .default_value_if("format", Some("csv"), "lon"))
        .arg(Arg::with_name("lat")
            .long("lat")
            .value_name("LAT")
            .takes_value(true)
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

    let file_path = matches.value_of("INPUT").unwrap();
    let mut input_str = String::new();
    match file_path.as_ref() {
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

    spinner.set_message("Parsing geography");
    let geoms: Vec<GridGeom<f64>> = match matches.value_of("format").unwrap() {
        "geojson" => {
            let gj: GeoJson = input_str
                .parse::<GeoJson>()
                .expect("Unable to parse GeoJSON");
            process_geojson(gj, matches.is_present("area"))
        }
        "csv" => {
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
        _ => panic!("Invalid format supplied"),
    };

    // Create a combined LineString for bounds calculation
    spinner.set_message("Indexing geography");
    let rtree: RTree<GridGeom<f64>> = RTree::bulk_load(geoms);
    let envelope = rtree.root().envelope();
    let rect = Rect::new(envelope.lower(), envelope.upper());

    let (term_height, term_width) = Term::stdout().size();
    let height: f64 = match matches.value_of("rows") {
        Some(ref rows) => rows.parse().expect("Rows cannot be parsed as a number."),
        None => (term_height - 1) as f64,
    };
    let width: f64 = match matches.value_of("columns") {
        Some(ref cols) => cols.parse().expect("Columns cannot be parsed as a number."),
        None => term_width as f64,
    };
    let grid = MapGrid::new(width, height, rect, rtree);
    spinner.finish_and_clear();
    grid.print();
}
