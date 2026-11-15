//! The planners: raster, snake, tiled z-stack, z-scan and multi-position.

use oscope_core::Point2;

use super::plan::{Field, Motion, Pattern, Plan, Position, Stack, Tile, ZPolicy, check_stack};
use super::region::Region;
use super::{ScanError, invalid};

/// The most tiles a plan takes, or a grid looks at.
pub const MOST_TILES: u64 = 100_000;

/// A raster over `region`: rows of tiles from its least y, each from its
/// least x.
pub fn raster(region: &Region, field: &Field, z: ZPolicy) -> Result<Plan, ScanError> {
    grid(Pattern::Raster, region, field, z, None)
}

/// A snake over `region`: rows from its least y, each the other way from
/// the row before, so the stage never runs back along a row (Choset, 2000).
pub fn snake(region: &Region, field: &Field, z: ZPolicy) -> Result<Plan, ScanError> {
    grid(Pattern::Snake, region, field, z, None)
}

/// A tiled z-stack: a snake over `region`, with `stack` taken at each tile.
pub fn tiled_stack(
    region: &Region,
    field: &Field,
    z: ZPolicy,
    stack: Stack,
) -> Result<Plan, ScanError> {
    grid(Pattern::TiledStack, region, field, z, Some(stack))
}

/// A z-scan: one tile at `at`, with `stack` taken through its z.
pub fn z_scan(at: Point2, field: &Field, z: ZPolicy, stack: Stack) -> Result<Plan, ScanError> {
    let only = Position {
        x_um: at.x,
        y_um: at.y,
        z_um: None,
        name: None,
    };
    let plan = positions(&[only], field, z, Some(stack))?;
    Ok(Plan {
        pattern: Pattern::ZScan,
        ..plan
    })
}

/// Positions, in the order given, each at its own z where it has one, and
/// otherwise as `z` says; with `stack` taken at each, if there is one.
pub fn positions(
    list: &[Position],
    field: &Field,
    z: ZPolicy,
    stack: Option<Stack>,
) -> Result<Plan, ScanError> {
    z.check()?;
    check_stack(stack.as_ref())?;
    let finite = |p: &Position| {
        p.x_um.is_finite() && p.y_um.is_finite() && p.z_um.is_none_or(f64::is_finite)
    };
    if list.is_empty() || list.len() as u64 > MOST_TILES || !list.iter().all(finite) {
        return Err(invalid(format!(
            "{} positions: from 1 to {MOST_TILES}, each finite, are needed",
            list.len()
        )));
    }
    let tiles = list
        .iter()
        .enumerate()
        .map(|(i, p)| Tile {
            index: i as u32,
            row: 0,
            column: i as u32,
            x_um: p.x_um,
            y_um: p.y_um,
            z_um: p.z_um.or_else(|| z.z_at(Point2::new(p.x_um, p.y_um))),
            name: p.name.clone(),
        })
        .collect();
    Ok(Plan {
        pattern: Pattern::Positions,
        motion: Motion::StopAndGo,
        field: field.clone(),
        rows: 1,
        columns: list.len() as u32,
        z,
        stack,
        tiles,
    })
}

/// The tiles of a grid over `region`'s bounds, centred on them, whose cells
/// touch the region. The cells cover the bounds, so each point of the
/// region is in a kept tile's cell, and so in its frame.
fn grid(
    pattern: Pattern,
    region: &Region,
    field: &Field,
    z: ZPolicy,
    stack: Option<Stack>,
) -> Result<Plan, ScanError> {
    region.check()?;
    z.check()?;
    check_stack(stack.as_ref())?;
    let bounds = region.bounds();
    let (cell, step) = (field.cell_um, field.step_um);
    // Enough tiles that the cells span the extent: n − 1 steps and a cell.
    let count = |extent: f64, cell: f64, step: f64| match extent > cell {
        true => ((extent - cell) / step).ceil() + 1.0,
        false => 1.0,
    };
    let columns = count(bounds.size.width, cell.width, step.width);
    let rows = count(bounds.size.height, cell.height, step.height);
    if columns * rows > MOST_TILES as f64 {
        return Err(invalid(format!(
            "a grid of {columns} × {rows} tiles over {:.0} × {:.0} µm: at most {MOST_TILES} \
             tiles are planned; a larger overlap, objective or binning, or a smaller region, \
             takes fewer",
            bounds.size.width, bounds.size.height
        )));
    }
    let (columns, rows) = (columns as u32, rows as u32);
    let middle = bounds.centre();
    let x0 = middle.x - f64::from(columns - 1) * step.width / 2.0;
    let y0 = middle.y - f64::from(rows - 1) * step.height / 2.0;
    let mut tiles = Vec::new();
    let mut forward = true;
    for row in 0..rows {
        let y = y0 + f64::from(row) * step.height;
        let mut kept: Vec<Tile> = (0..columns)
            .map(|column| (column, Point2::new(x0 + f64::from(column) * step.width, y)))
            .filter(|(_, at)| region.touches(&field.cell_at(*at)))
            .map(|(column, at)| Tile {
                index: 0,
                row,
                column,
                x_um: at.x,
                y_um: at.y,
                z_um: z.z_at(at),
                name: None,
            })
            .collect();
        if kept.is_empty() {
            continue;
        }
        if pattern != Pattern::Raster && !forward {
            kept.reverse();
        }
        forward = !forward;
        tiles.extend(kept);
    }
    for (i, tile) in tiles.iter_mut().enumerate() {
        tile.index = i as u32;
    }
    Ok(Plan {
        pattern,
        motion: Motion::StopAndGo,
        field: field.clone(),
        rows,
        columns,
        z,
        stack,
        tiles,
    })
}
