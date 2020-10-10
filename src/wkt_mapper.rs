use wkt::{
    Wkt,
    Geometry as WktGeometry,
    types::{
        Point as WktPoint,
        LineString as WktLineString,
        Polygon as WktPolygon
    }
};
use crate::{GridGeom};
use geo_types::{
    Geometry,
    Point,
    MultiPoint,
    LineString,
    Coordinate,
    Polygon,
    MultiPolygon
};

fn map_point(point: WktPoint<f64>) -> Point<f64> {
    let coordinate = point.0.unwrap();
    Point::new(coordinate.x, coordinate.y)
}

fn map_line(linestring: WktLineString<f64>) -> LineString<f64> {
    let coordinates = linestring.0
        .into_iter()
        .map(|c| Coordinate::from((c.x, c.y)))
        .collect();
    LineString(coordinates)
}

fn map_polygon(mut polygon: WktPolygon<f64>) -> Polygon<f64> {
    if polygon.0.len() != 1 {
        unimplemented!("Unimplemented: send a PR!")
    }
    let exterior = polygon.0.remove(0);
    let exterior = exterior.0
        .into_iter()
        .map(|c| geo_types::Coordinate::from((c.x, c.y)))
        .collect();
    let exterior = LineString(exterior);

    Polygon::new(exterior, vec![])
}

pub fn map(wkt: Wkt<f64>) -> Vec<GridGeom<f64>> {
  wkt.items.into_iter()
        .flat_map(|s| {
            match s {
                WktGeometry::Point(point) => {
                    GridGeom::<f64>::vec_from_geom(
                        Geometry::Point(map_point(point)),
                        0.0,
                    false
                )
            },
            WktGeometry::MultiPoint(points) => {
                let points = points.0
                    .into_iter()
                    .map(|p| map_point(p))
                    .collect();
                GridGeom::<f64>::vec_from_geom(
                    Geometry::MultiPoint(MultiPoint(points)),
                    0.0,
                    false
                )  
            },
            WktGeometry::LineString(linestring) => {
                GridGeom::<f64>::vec_from_geom(
                    Geometry::LineString(map_line(linestring)),
                    0.0,
                    false
                )
            },
            WktGeometry::MultiLineString(multistrings) => {
                let multistrings = multistrings.0
                    .into_iter()
                    .map(|linestring| map_line(linestring))
                    .collect();
                GridGeom::<f64>::vec_from_geom(
                    Geometry::MultiLineString(multistrings),
                    0.0,
                    false
                )
            },
            WktGeometry::Polygon(polygon) => {
                GridGeom::<f64>::vec_from_geom(
                    Geometry::Polygon(map_polygon(polygon)),
                    0.0,
                    true
                )
            },
            WktGeometry::MultiPolygon(polygons) => {
                let polygons = polygons.0
                    .into_iter()
                    .map(|polygon| map_polygon(polygon))
                    .collect::<Vec<Polygon<f64>>>();
                GridGeom::<f64>::vec_from_geom(
                    Geometry::MultiPolygon(
                        MultiPolygon::from(polygons)
                    ),
                    0.0,
                    true
                )
            },
            _ => unimplemented!("Unimplemented: send a PR!")
        }
    })
    .collect()
}
