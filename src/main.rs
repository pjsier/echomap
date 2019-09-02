use std::convert::TryInto;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::iter::FromIterator;

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
    let file_path = env::args()
        .nth(1)
        .expect("Must supply a file path or '-' to read stdin");

    let spinner = ProgressBar::new_spinner();
    spinner.set_message("Reading file");
    spinner.enable_steady_tick(1);

    let mut geojson_str = String::new();
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

    let (height, width) = Term::stdout().size();
    let grid = MapGrid::new(width as f64, (height - 1) as f64, rect, rtree);
    spinner.finish_and_clear();
    grid.print();
}
