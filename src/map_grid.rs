use std::char;
use std::io::{self, Write};

use geo::algorithm::bounding_rect::BoundingRect;
use geo::algorithm::intersects::Intersects;
use geo_types::{CoordinateType, Line, Point, Polygon, Rect};
use num_traits::{Float, FromPrimitive};
use rstar::{self, RTree, RTreeNum, RTreeObject, AABB};

pub enum GridGeom<T>
where
    T: CoordinateType + Float + RTreeNum + FromPrimitive,
{
    Line(Line<T>),
    Polygon(Polygon<T>),
}

impl<T> RTreeObject for GridGeom<T>
where
    T: CoordinateType + Float + RTreeNum + FromPrimitive,
{
    type Envelope = AABB<[T; 2]>;

    fn envelope(&self) -> Self::Envelope {
        let rect = match self {
            GridGeom::Line(line) => line.bounding_rect(),
            GridGeom::Polygon(poly) => poly.bounding_rect().unwrap(),
        };
        AABB::from_corners([rect.min.x, rect.min.y], [rect.max.x, rect.max.y])
    }
}

pub struct MapGrid<T>
where
    T: Float + RTreeNum + FromPrimitive,
{
    rows: i32,
    cols: i32,
    bbox: Rect<T>,
    cellsize: [f64; 2],
    rtree: RTree<GridGeom<T>>,
}

impl<T> MapGrid<T>
where
    T: Float + RTreeNum + FromPrimitive,
{
    pub fn new(width: f64, height: f64, bbox: Rect<T>, rtree: RTree<GridGeom<T>>) -> MapGrid<T> {
        let box_width = bbox.width().to_f64().unwrap();
        let box_height = bbox.height().to_f64().unwrap();

        let box_aspect_ratio = box_width / box_height;
        let term_aspect_ratio = width / height;

        // Clamp dimensions to aspect ratio of the geometry bbox
        let (cols_f, rows_f) = match (
            term_aspect_ratio > 1.0,
            box_aspect_ratio > 2.0,
            term_aspect_ratio > (box_aspect_ratio * 2.0),
        ) {
            // Multiply or divide by 2.0 to account for columns being more narrow than rows
            (true, true, true) | (true, false, _) => (height * box_aspect_ratio * 2.0, height),
            (true, true, _) | (false, _, _) => (width, (width / box_aspect_ratio) / 2.0),
        };

        let cols = f64::ceil(width) as i32;
        let rows = f64::ceil(height) as i32;

        // Get dimensions of individual cells
        let cell_width = box_width / cols_f;
        let cell_height = box_height / rows_f;

        MapGrid {
            bbox,
            rows,
            cols,
            cellsize: [cell_width, cell_height],
            rtree,
        }
    }

    /// Iterate through cells, printing one line at a time
    pub fn print(&self) {
        let stdout = io::stdout();
        let mut handle = io::BufWriter::new(stdout.lock());

        for r in 0..self.rows {
            let mut row_str = "".to_string();
            for c in 0..self.cols {
                let cell_value = self.query_cell_value(r, c);
                row_str.push_str(&Self::braille_char(cell_value).to_string());
            }
            writeln!(handle, "{}", row_str).expect("Error printing line");
        }
    }

    /// For a given Braille 2x4 cell, query which cells have lines in them
    pub fn query_cell_value(&self, row: i32, col: i32) -> u32 {
        let cell_rows = 4;
        let cell_cols = 2;
        let mut cell_value = 0x00;

        let cell_width = self.cellsize[0] / (cell_cols as f64);
        let cell_height = self.cellsize[1] / (cell_rows as f64);

        // Get the start offset dimensions based on the outer row and column
        let cell_start_width =
            (self.cellsize[0] * (col as f64)) + self.bbox.min.x.to_f64().unwrap();
        let cell_start_height =
            self.bbox.max.y.to_f64().unwrap() - (self.cellsize[1] * (row as f64));

        for r in 0..cell_rows {
            for c in 0..cell_cols {
                // Generate an envelope from the coordinates of the current cell
                let cell_min_x = cell_start_width + (cell_width * (c as f64));
                let cell_max_y = cell_start_height - (cell_height * (r as f64));
                let cell_max_x = T::from_f64(cell_min_x + cell_width).unwrap();
                let cell_min_y = T::from_f64(cell_max_y - cell_height).unwrap();

                let min_pt = Point::new(T::from_f64(cell_min_x).unwrap(), cell_min_y);
                let max_pt = Point::new(cell_max_x, T::from_f64(cell_max_y).unwrap());

                let envelope =
                    AABB::from_corners([min_pt.x(), min_pt.y()], [max_pt.x(), max_pt.y()]);
                let rect_poly = Polygon::from(Rect::new(min_pt, max_pt));

                // Find all intersecting envelopes, check if the underlying lines intersect
                let envelope_intersect = self.rtree.locate_in_envelope_intersecting(&envelope);
                let intersecting_geoms: Vec<&GridGeom<T>> = envelope_intersect
                    .into_iter()
                    .skip_while(|l| match l {
                        GridGeom::Line(line) => !rect_poly.intersects(line),
                        GridGeom::Polygon(poly) => !rect_poly.intersects(poly),
                    })
                    .collect();

                // Add the associated cell value if intersecting lines are found
                if intersecting_geoms.len() > 0 {
                    cell_value = cell_value + Self::braille_cell_value(r, c);
                }
            }
        }

        cell_value
    }

    /// Convert row/col coordinates to associated Braille hex value
    pub fn braille_cell_value(row: i32, col: i32) -> u32 {
        match (row, col) {
            (0, 0) => 0x01,
            (0, 1) => 0x08,
            (1, 0) => 0x02,
            (1, 1) => 0x10,
            (2, 0) => 0x04,
            (2, 1) => 0x20,
            (3, 0) => 0x40,
            (3, 1) => 0x80,
            (_, _) => 0x00,
        }
    }

    /// Add the Braille offset base to the calculated cell value to generate a char
    pub fn braille_char(suffix: u32) -> char {
        let braille_offset = 0x2800;
        char::from_u32(braille_offset + suffix).unwrap()
    }
}
