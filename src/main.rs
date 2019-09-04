use std::convert::TryInto;
use std::fs;
use std::io::{self, Read};
use std::iter::FromIterator;

extern crate clap;

use clap::{App, Arg};
use console::Term;
use geo::algorithm::bounding_rect::BoundingRect;
use geo_types::{Line, LineString, MultiLineString, MultiPolygon, Polygon};
use geojson::{GeoJson, Geometry, Value};
use indicatif::ProgressBar;
use num_traits::{Float, FromPrimitive};
use rstar::{RTree, RTreeNum};

pub mod map_grid;
use map_grid::MapGrid;

/// Process GeoJSON geometries
fn match_geometry<T: Float + RTreeNum + FromPrimitive>(geom: Geometry) -> Vec<Line<T>> {
    match geom.value {
        Value::LineString(_) => {
            let ls: LineString<T> = geom.value.try_into().unwrap();
            ls.lines().collect()
        }
        Value::Polygon(_) => {
            let poly: Polygon<T> = geom.value.try_into().unwrap();
            poly.exterior().lines().collect()
        }
        Value::MultiLineString(_) => {
            let ml: MultiLineString<T> = geom.value.try_into().unwrap();
            ml.into_iter()
                .flat_map(|ls| ls.lines().collect::<Vec<_>>())
                .collect()
        }
        Value::MultiPolygon(_) => {
            let mp: MultiPolygon<T> = geom.value.try_into().unwrap();
            mp.into_iter()
                .flat_map(|geometry| geometry.exterior().lines().collect::<Vec<_>>())
                .collect()
        }
        Value::GeometryCollection(collection) => collection
            .into_iter()
            .flat_map(|geometry| match_geometry(geometry))
            .collect(),
        _ => vec![],
    }
}

/// Process top-level GeoJSON items
fn process_geojson<T: Float + RTreeNum + FromPrimitive>(gj: GeoJson) -> Vec<Line<T>> {
    match gj {
        GeoJson::FeatureCollection(collection) => collection
            .features
            .into_iter()
            .filter_map(|feature| feature.geometry)
            .flat_map(|geometry| match_geometry(geometry))
            .collect(),
        GeoJson::Feature(feature) => {
            if let Some(geometry) = feature.geometry {
                match_geometry(geometry)
            } else {
                vec![]
            }
        }
        GeoJson::Geometry(geometry) => match_geometry(geometry),
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
        .get_matches();

    let spinner = ProgressBar::new_spinner();
    spinner.set_message("Reading file");
    spinner.enable_steady_tick(1);

    let mut geojson_str = String::new();
    let file_path = matches.value_of("INPUT").unwrap();
    match file_path.as_ref() {
        "-" => {
            io::stdin()
                .read_to_string(&mut geojson_str)
                .expect("There was an error reading from stdin");
        }
        _ => {
            fs::File::open(file_path)
                .unwrap()
                .read_to_string(&mut geojson_str)
                .expect("There was an error reading your file");
        }
    };

    spinner.set_message("Parsing geography");
    let gj: GeoJson = geojson_str.parse::<GeoJson>().unwrap();
    let mut lines: Vec<Line<f64>> = process_geojson(gj);

    // Create a combined LineString for bounds calculation
    let ls: LineString<f64> =
        LineString::from_iter(lines.iter_mut().flat_map(|l| vec![l.start, l.end]));

    spinner.set_message("Indexing geography");
    let rect = ls.bounding_rect().unwrap();
    let rtree: RTree<Line<f64>> = RTree::bulk_load_parallel(lines);

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
