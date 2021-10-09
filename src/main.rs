use std::convert::{TryFrom, TryInto};
use std::fs;
use std::io::{self, Read};
use std::str::FromStr;

use anyhow::{self, Context, Result};
use clap::{App, Arg};
use console::Term;
use geo::{Geometry, Point};
use geojson::{self, GeoJson};
use indicatif::ProgressBar;
use kml::{quick_collection, Kml};
use polyline::decode_polyline;
use rstar::RTree;
use topojson::{to_geojson, TopoJson};
use wkt::Wkt;

mod map_grid;
use map_grid::{GridGeom, MapGrid};

#[derive(Debug, PartialEq)]
enum InputFormat {
    GeoJson,
    TopoJson,
    Csv,
    Shapefile,
    Wkt,
    Polyline,
    Kml,
}

impl FromStr for InputFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<InputFormat> {
        match s.to_ascii_lowercase().as_ref() {
            "geojson" => Ok(InputFormat::GeoJson),
            "topojson" => Ok(InputFormat::TopoJson),
            "csv" => Ok(InputFormat::Csv),
            "shp" => Ok(InputFormat::Shapefile),
            "wkt" => Ok(InputFormat::Wkt),
            "polyline" => Ok(InputFormat::Polyline),
            "kml" => Ok(InputFormat::Kml),
            f => Err(anyhow::anyhow!("Invalid format supplied: {}", f)),
        }
    }
}

/// Get file format from flag or infer from file path
fn get_file_format(file_path: &str, file_format: Option<&str>) -> Result<InputFormat> {
    let format_str = match file_format {
        Some(f) => f,
        None => file_path.split('.').last().unwrap(),
    };
    format_str.parse()
}

/// Parse simplification value from float or percentage string
fn get_simplification(simplify: &str) -> Result<f64> {
    if simplify.contains('%') {
        let simplify = simplify
            .replace("%", "")
            .parse::<f64>()
            .with_context(|| format!("Could not parse simplify value: {}", simplify))?;
        Ok(simplify / 100.)
    } else {
        simplify
            .parse::<f64>()
            .with_context(|| format!("Could not parse simplify value: {}", simplify))
    }
}

/// Read file path (or stdin) to string
fn read_input_to_string(file_path: &str) -> Result<String> {
    let mut input_str = String::new();
    match file_path {
        "-" => io::stdin()
            .read_to_string(&mut input_str)
            .context("There was an error reading from stdin"),
        _ => fs::File::open(file_path)
            .unwrap()
            .read_to_string(&mut input_str)
            .with_context(|| format!("There was an error reading from file: {}", file_path)),
    }?;
    Ok(input_str)
}

/// Process top-level GeoJSON items
pub fn process_geojson(gj: GeoJson, simplification: f64, is_area: bool) -> Vec<GridGeom<f64>> {
    match gj {
        GeoJson::FeatureCollection(collection) => collection
            .features
            .into_iter()
            .filter_map(|feature| feature.geometry)
            .flat_map(|g| {
                let geom: Geometry<f64> = g.value.try_into().unwrap();
                GridGeom::<f64>::vec_from_geom(geom, simplification, is_area)
            })
            .collect(),
        GeoJson::Feature(feature) => {
            if let Some(geometry) = feature.geometry {
                let geom: Geometry<f64> = geometry.value.try_into().unwrap();
                GridGeom::<f64>::vec_from_geom(geom, simplification, is_area)
            } else {
                vec![]
            }
        }
        GeoJson::Geometry(geometry) => {
            let geom: Geometry<f64> = geometry.value.try_into().unwrap();
            GridGeom::<f64>::vec_from_geom(geom, simplification, is_area)
        }
    }
}

fn handle_geojson(
    input_str: String,
    simplification: f64,
    is_area: bool,
) -> Result<Vec<GridGeom<f64>>> {
    let gj: GeoJson = input_str
        .parse::<GeoJson>()
        .context("Unable to parse GeoJSON")?;
    Ok(process_geojson(gj, simplification, is_area))
}

fn handle_topojson(
    input_str: String,
    simplification: f64,
    is_area: bool,
) -> Result<Vec<GridGeom<f64>>> {
    let topo = input_str
        .parse::<TopoJson>()
        .context("Unable to parse TopoJSON")?;
    match topo {
        TopoJson::Topology(t) => Ok(t
            .list_names()
            .into_iter()
            .map(|n| to_geojson(&t, &n))
            .filter_map(|g| g.ok())
            .map(GeoJson::FeatureCollection)
            .flat_map(|g| process_geojson(g, simplification, is_area))
            .collect()),
        _ => unimplemented!(),
    }
}

fn handle_csv(input_str: String, lat_col: &str, lon_col: &str) -> Result<Vec<GridGeom<f64>>> {
    let mut rdr = csv::Reader::from_reader(input_str.as_bytes());
    let headers = rdr.headers().context("Unable to load CSV headers")?;

    let lat_idx = headers
        .iter()
        .position(|v| v == lat_col)
        .with_context(|| format!("Lat column {} not found", lat_col))?;
    let lon_idx = headers
        .iter()
        .position(|v| v == lon_col)
        .with_context(|| format!("Lon column {} not found", lat_col))?;

    rdr.records()
        .map(|rec_val| {
            let rec = rec_val.context("Could not parse CSV record")?;
            let lat_val: f64 = rec
                .get(lat_idx)
                .unwrap()
                .parse()
                .context("Could not parse lat value from record")?;
            let lon_val: f64 = rec
                .get(lon_idx)
                .unwrap()
                .parse()
                .context("Could not parse lon value from record")?;
            let pt: Point<f64> = Point::new(lon_val, lat_val);
            Ok(GridGeom::Point(pt))
        })
        .collect()
}

fn handle_shp(file_path: &str, simplification: f64, is_area: bool) -> Result<Vec<GridGeom<f64>>> {
    let mut rdr = shapefile::ShapeReader::from_path(file_path)
        .with_context(|| format!("There was an error opening shapefile {}", file_path))?;
    Ok(rdr
        .iter_shapes()
        .filter_map(|s| s.ok())
        .flat_map(|s| match Geometry::<f64>::try_from(s) {
            Ok(geom) => GridGeom::<f64>::vec_from_geom(geom, simplification, is_area),
            Err(_) => vec![],
        })
        .collect())
}

fn handle_wkt(input_str: String, simplification: f64, is_area: bool) -> Result<Vec<GridGeom<f64>>> {
    let wkt = Wkt::<f64>::from_str(&input_str)
        .map_err(|_| anyhow::anyhow!("There was an error parsing WKT"))?;
    Ok(wkt
        .items
        .into_iter()
        .filter_map(|s| s.try_into().ok())
        .flat_map(|geo| GridGeom::<f64>::vec_from_geom(geo, simplification, is_area))
        .collect())
}

fn handle_polyline(
    input_str: String,
    precision: &str,
    simplification: f64,
) -> Result<Vec<GridGeom<f64>>> {
    let precision: u32 = precision
        .parse()
        .context("Precision has to be defined for polyline format")?;
    let lines = decode_polyline(&input_str, precision).unwrap();
    Ok(GridGeom::vec_from_geom(
        geo_types::Geometry::LineString(lines),
        simplification,
        false,
    ))
}

fn handle_kml(input_str: String, simplification: f64, is_area: bool) -> Result<Vec<GridGeom<f64>>> {
    let kml: Kml = input_str
        .parse()
        .map_err(|_| anyhow::anyhow!("There was an error parsing KML"))?;
    Ok(GridGeom::<f64>::vec_from_geom(
        geo_types::Geometry::GeometryCollection(quick_collection(kml)?),
        simplification,
        is_area,
    ))
}

fn main() -> Result<()> {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(Arg::with_name("INPUT")
            .help("File to parse or '-' to read stdin")
            .required(true)
            .index(1))
        .arg(Arg::with_name("format")
            .short("f")
            .long("format")
            .value_name("FORMAT")
            .help("Input file format (tries to infer from file extension by default)")
            .possible_values(&["geojson", "topojson", "csv", "shp", "wkt", "polyline", "kml"])
            .default_value_if("INPUT", Some("-"), "geojson")
            .takes_value(true))
        .arg(Arg::with_name("lon")
            .long("lon")
            .value_name("LON")
            .takes_value(true)
            .help("Name of longitude column (if format is 'csv')")
            .default_value("lon"))
        .arg(Arg::with_name("lat")
            .long("lat")
            .value_name("LAT")
            .takes_value(true)
            .help("Name of latitude column (if format is 'csv')")
            .default_value("lat"))
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
        .arg(Arg::with_name("simplify")
            .short("s")
            .long("simplify")
            .help("Proportion of removable points to remove (0-1 or 0%-100%)")
            .takes_value(true)
            .default_value("0.01"))
        .arg(Arg::with_name("precision")
            .long("precision")
            .help("Precision value for polyline parsing")
            .required_if("format", "polyline")
            .takes_value(true))
        .arg(Arg::with_name("area")
            .short("a")
            .long("area")
            .help("Print polygon area instead of boundaries"))
        .get_matches();

    let (term_height, term_width) = Term::stdout().size();
    let height: f64 = match matches.value_of("rows") {
        Some(ref rows) => rows
            .parse()
            .with_context(|| format!("Rows value {} cannot be parsed as a number", rows)),
        None => Ok(f64::from(term_height - 1)),
    }?;
    let width: f64 = match matches.value_of("columns") {
        Some(ref cols) => cols
            .parse()
            .with_context(|| format!("Columns value {} cannot be parsed as a number", cols)),
        None => Ok(f64::from(term_width)),
    }?;

    // Simplification is scaled by the output size
    let simplify = get_simplification(matches.value_of("simplify").unwrap())?;
    let simplification = simplify / (height * width);

    let spinner = ProgressBar::new_spinner();
    spinner.set_message("Reading file");
    spinner.enable_steady_tick(1);
    spinner.set_message("Parsing geography");

    let file_format = get_file_format(
        matches.value_of("INPUT").unwrap(),
        matches.value_of("format"),
    )?;

    let geoms: Vec<GridGeom<f64>> = match file_format {
        InputFormat::GeoJson => handle_geojson(
            read_input_to_string(matches.value_of("INPUT").unwrap())?,
            simplification,
            matches.is_present("area"),
        ),
        InputFormat::TopoJson => handle_topojson(
            read_input_to_string(matches.value_of("INPUT").unwrap())?,
            simplification,
            matches.is_present("area"),
        ),
        InputFormat::Csv => handle_csv(
            read_input_to_string(matches.value_of("INPUT").unwrap())?,
            matches.value_of("lat").unwrap(),
            matches.value_of("lon").unwrap(),
        ),
        InputFormat::Shapefile => handle_shp(
            matches.value_of("INPUT").unwrap(),
            simplification,
            matches.is_present("area"),
        ),
        InputFormat::Wkt => handle_wkt(
            read_input_to_string(matches.value_of("INPUT").unwrap())?,
            simplification,
            matches.is_present("area"),
        ),
        InputFormat::Polyline => handle_polyline(
            read_input_to_string(matches.value_of("INPUT").unwrap())?,
            matches.value_of("precision").unwrap(),
            simplification,
        ),
        InputFormat::Kml => handle_kml(
            read_input_to_string(matches.value_of("INPUT").unwrap())?,
            simplification,
            matches.is_present("area"),
        ),
    }?;

    // Create a combined LineString for bounds calculation
    spinner.set_message("Indexing geography");
    let rtree: RTree<GridGeom<f64>> = RTree::bulk_load(geoms);
    let grid = MapGrid::new(width, height, rtree);
    spinner.finish_and_clear();
    grid.print()
}

#[cfg(test)]
mod test {
    use super::*;
    use geo_types::{Line, Point};

    #[test]
    fn test_get_file_format() {
        assert!(matches!(
            get_file_format("test.GEOJSON", None),
            Ok(InputFormat::GeoJson)
        ));
        assert!(matches!(
            get_file_format("test.geojson", Some("csv")),
            Ok(InputFormat::Csv)
        ));
    }

    #[test]
    fn test_handle_geojson() {
        let input_str = include_str!("../fixtures/input.geojson").to_string();
        let outlines = handle_geojson(input_str.clone(), 0., false).unwrap();
        let lines = outlines.iter().filter(|g| matches!(g, GridGeom::Line(_)));
        let areas = handle_geojson(input_str, 0., true).unwrap();
        let poly = areas.iter().filter(|g| matches!(g, GridGeom::Polygon(_)));
        assert_eq!(outlines.len(), 14);
        assert_eq!(lines.count(), 13);
        assert_eq!(areas.len(), 5);
        assert_eq!(poly.count(), 3);
    }

    #[test]
    fn test_handle_topojson() {
        let input_str = include_str!("../fixtures/input.topojson").to_string();
        let outlines = handle_topojson(input_str.clone(), 0., false).unwrap();
        let lines = outlines.iter().filter(|g| matches!(g, GridGeom::Line(_)));
        let areas = handle_topojson(input_str, 0., true).unwrap();
        let poly = areas.iter().filter(|g| matches!(g, GridGeom::Polygon(_)));
        assert_eq!(outlines.len(), 14);
        assert_eq!(lines.count(), 13);
        assert_eq!(areas.len(), 5);
        assert_eq!(poly.count(), 3);
    }

    #[test]
    fn test_handle_csv() {
        let input_str = include_str!("../fixtures/input.csv").to_string();
        assert_eq!(
            handle_csv(input_str, "one", "two").unwrap(),
            vec![
                GridGeom::Point(Point::<f64>::new(-1.0, 1.0)),
                GridGeom::Point(Point::<f64>::new(-2.0, 2.0))
            ]
        );
    }

    #[test]
    fn test_handle_shp() {
        let file_path = "./fixtures/input_point.shp";
        let points = handle_shp(file_path, 0., false).unwrap();
        let points = points.iter().filter(|g| matches!(g, GridGeom::Point(_)));
        let file_path = "./fixtures/input_line.shp";
        let lines = handle_shp(file_path, 0., false).unwrap();
        let lines = lines.iter().filter(|g| matches!(g, GridGeom::Line(_)));
        let file_path = "./fixtures/input_area.shp";
        let areas = handle_shp(file_path, 0., true).unwrap();
        let poly = areas.iter().filter(|g| matches!(g, GridGeom::Polygon(_)));
        assert_eq!(points.count(), 1);
        assert_eq!(lines.count(), 1);
        assert_eq!(poly.count(), 3);
    }

    #[test]
    fn test_handle_wkt() {
        let input_str = include_str!("../fixtures/input.wkt").to_string();
        assert_eq!(
            handle_wkt(input_str, 0., false).unwrap(),
            vec![
                GridGeom::Point(Point::<f64>::new(4.0, 6.0)),
                GridGeom::Line(Line::<f64>::new((4.0, 6.0), (7.0, 10.0))),
            ]
        );
    }

    #[test]
    fn test_handle_polyline() {
        let input_str = include_str!("../fixtures/input.polyline.txt").to_string();
        assert_eq!(
            handle_polyline(input_str, "5", 0.).unwrap(),
            vec![
                GridGeom::Line(Line::new((-120.2, 38.5), (-120.95, 40.7))),
                GridGeom::Line(Line::new((-120.95, 40.7), (-126.453, 43.252)))
            ]
        );
    }

    #[test]
    fn test_handle_kml() {
        let input_str = include_str!("../fixtures/input.kml").to_string();
        assert_eq!(
            handle_kml(input_str, 0., false).unwrap(),
            vec![
                GridGeom::Line(Line::new((-1., 2.), (-1.5, 3.))),
                GridGeom::Line(Line::new((-1.5, 3.), (-1.5, 2.))),
                GridGeom::Line(Line::new((-1.5, 2.), (-1., 2.)))
            ]
        );
    }
}
