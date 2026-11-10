//! Point-and-figure charts: columns of X and O built from a sequence of prices, and the
//! horizontal count over them.
//!
//! A point-and-figure chart has no time axis. A column of X records a rise, a column of O a fall,
//! and a new column begins only when the price turns by a set number of boxes. [`PnfChart`] builds
//! the columns one price at a time, [`point_and_figure()`] over a whole slice; the
//! `point_and_figure` indicator ([`crate::indicator::point_and_figure`]) feeds the same chart
//! from bars and reports the current column on every bar.
//!
//! # The construction
//!
//! Two numbers fix the chart: the box size `b`, a price distance, and the reversal `r`, a number
//! of boxes. The grid lines lie on the whole multiples of `b`. A box is named by its grid line
//! and is filled once the price has reached that line: rising prices fill the boxes up to the
//! highest line at or below the price, falling prices the boxes down to the lowest line at or
//! above it.
//!
//! ```text
//! up(p)   = floor(p / b)      highest box a price p has reached, coming from below
//! down(p) = ceil(p / b)       lowest box a price p has reached, coming from above
//!
//! X column whose highest box is h:
//!     up(p) > h                  the column is extended up to up(p)
//!     else down(p) <= h - r      a new O column, from h - 1 down to down(p)
//! O column whose lowest box is t:
//!     down(p) < t                the column is extended down to down(p)
//!     else up(p) >= t + r        a new X column, from t + 1 up to up(p)
//!
//! before the first column, with the anchor u = up(first price), d = down(first price):
//!     up(p) > u                  the first column is an X column, from u up to up(p)
//!     down(p) < d                the first column is an O column, from d down to down(p)
//! ```
//!
//! A reversal therefore needs a move of at least `r` boxes from the last filled box, and the
//! chart of a series reflected at a grid line is the mirror image of the chart of the series: X
//! for O, every box reflected. A price that is not finite changes nothing.
//!
//! **Highs and lows instead of one price** ([`PnfChart::push_high_low`]). A bar is read in the
//! direction of the current column first: in an X column the high, and only if it fills no new
//! box the low for a reversal; in an O column the low, and only then the high. One bar thus
//! never extends a column and reverses it. Before the first column the anchor is the range of
//! boxes the highs and lows have reached so far; a bar that leaves it on one side opens the first
//! column, a bar that leaves it on both sides only widens it.
//!
//! # Conventions
//!
//! Decisions of this crate where the method is practised in more than one way:
//!
//! - **Box size and reversal** are free parameters; the literature gives scales by price level
//!   and by purpose, not one value. The count below is proportional to both.
//! - **One price or high and low.** Both are in use. A sequence of single prices (closes) needs
//!   no assumption; highs and lows of a bar come without their order, which the rule above
//!   replaces by reading the bar in the direction of the column first.
//! - **Every reversal opens a new column**, for every `r`, including 1. The practice of one-box
//!   charts to let an X and an O share a column is not applied.
//! - **The first column** starts at the anchor box on its own side and takes the direction of the
//!   first move that fills a box beyond the anchor.
//! - **Grid tolerance.** A price within [`crate::point_and_figure::GRID_TOLERANCE`] boxes of a
//!   grid line counts as on it, so a price that is a whole multiple of the box size in decimal
//!   notation (0.3 with a box of 0.1) is not thrown off its line by binary rounding.
//!
//! # The horizontal count
//!
//! The Wyckoff method reads the width of a trading range off the chart as a number of columns
//! and projects it as a price distance: the count is taken from the last column of the range
//! back to its first, and the distance is the number of columns times the reversal times the box
//! size. [`PnfChart::horizontal_count`] computes it between two named columns; which columns end
//! the range (in an accumulation the Last Point of Support and the Selling Climax, both O
//! columns) is the reader's decision and not derived here.
//!
//! ```text
//! first, last   two columns of the same kind, first <= last
//! line          lowest box of column last            (O columns; highest box for X columns)
//! extreme       lowest box of the columns first..last (O columns; highest box for X columns)
//! columns       number of columns first..last, both included
//! on_line       how many of them have a filled box on the line
//! effect        columns * r * b                effect_on_line = on_line * r * b
//! targets       extreme + effect, line + effect   (O columns; minus for X columns)
//! ```
//!
//! Conventions: the two end columns are counted, the count line is the extreme box of the later
//! column and the extreme is taken over the counted columns. The literature counts either all
//! columns between the ends or the columns along the count line; both numbers are returned. The
//! result is a distance derived from the width of a range and depends on the box size, the
//! reversal and the two columns chosen; it carries no statement about time or probability.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// How close to a grid line, in boxes, a price counts as on the line.
pub const GRID_TOLERANCE: f64 = 1e-9;

/// Direction of a column: `Up` is a column of X (rising prices), `Down` a column of O (falling
/// prices).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PnfDirection {
    Up,
    Down,
}

impl PnfDirection {
    /// `1.0` for a column of X, `-1.0` for a column of O.
    pub fn code(self) -> f64 {
        match self {
            PnfDirection::Up => 1.0,
            PnfDirection::Down => -1.0,
        }
    }

    /// `'X'` or `'O'`.
    pub fn symbol(self) -> char {
        match self {
            PnfDirection::Up => 'X',
            PnfDirection::Down => 'O',
        }
    }
}

/// One column of the chart: its direction and its filled boxes, which are every box from
/// `bottom_box` to `top_box`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PnfColumn {
    pub direction: PnfDirection,
    /// Grid line of the highest filled box, in price units (`top_box * box_size`).
    pub top: f64,
    /// Grid line of the lowest filled box, in price units (`bottom_box * box_size`).
    pub bottom: f64,
    /// Highest filled box, as the number of its grid line.
    pub top_box: i64,
    /// Lowest filled box, as the number of its grid line.
    pub bottom_box: i64,
    /// Position (0-based) in the input of the price or bar that opened the column.
    pub first_index: usize,
    /// Position (0-based) in the input of the last price or bar that extended the column.
    pub last_index: usize,
}

impl PnfColumn {
    /// Number of filled boxes.
    pub fn boxes(&self) -> usize {
        (self.top_box - self.bottom_box + 1) as usize
    }

    /// Whether the box on grid line `box_index` is filled.
    pub fn contains(&self, box_index: i64) -> bool {
        (self.bottom_box..=self.top_box).contains(&box_index)
    }
}

/// What one price or bar did to the chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PnfChange {
    /// No box was filled.
    None,
    /// The current column was extended.
    Extended,
    /// A new column was opened: the first one, or a reversal.
    NewColumn,
}

/// The horizontal count between two columns; see the module documentation for the calculation
/// and its conventions.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct HorizontalCount {
    /// Index of the earlier end column in [`PnfChart::columns`].
    pub first_column: usize,
    /// Index of the later end column, the one the count starts from.
    pub last_column: usize,
    /// Direction of the projected distance: `Up` for a count over O columns (the base of a
    /// rise), `Down` for a count over X columns.
    pub direction: PnfDirection,
    /// The count line, in price units: the lowest box of the later column (highest for a count
    /// over X columns).
    pub count_line: f64,
    /// The lowest box of the counted columns, in price units (highest for X columns).
    pub extreme: f64,
    /// Columns from `first_column` to `last_column`, both included.
    pub columns: usize,
    /// How many of those columns have a filled box on the count line.
    pub columns_on_line: usize,
    /// `columns * reversal * box_size`, a price distance.
    pub effect: f64,
    /// `columns_on_line * reversal * box_size`.
    pub effect_on_line: f64,
    /// `extreme` plus `effect` (minus for a count over X columns).
    pub target_from_extreme: f64,
    /// `count_line` plus `effect` (minus for a count over X columns).
    pub target_from_line: f64,
}

/// A point-and-figure chart under construction.
#[derive(Debug, Clone, PartialEq)]
pub struct PnfChart {
    box_size: f64,
    reversal: usize,
    columns: Vec<PnfColumn>,
    /// Before the first column: the highest box reached from below and the lowest from above.
    anchor: Option<(i64, i64)>,
    /// Prices or bars pushed so far.
    count: usize,
}

impl PnfChart {
    /// An empty chart with boxes of `box_size` price units and a reversal of `reversal` boxes
    /// (at least 1).
    ///
    /// # Panics
    ///
    /// If `box_size` is not a positive, finite number.
    pub fn new(box_size: f64, reversal: usize) -> Self {
        assert!(
            box_size.is_finite() && box_size > 0.0,
            "point-and-figure box size must be positive and finite, got {box_size}"
        );
        Self {
            box_size,
            reversal: reversal.max(1),
            columns: Vec::new(),
            anchor: None,
            count: 0,
        }
    }

    pub fn box_size(&self) -> f64 {
        self.box_size
    }

    pub fn reversal(&self) -> usize {
        self.reversal
    }

    /// The columns so far, oldest first; the last one is still open to extension.
    pub fn columns(&self) -> &[PnfColumn] {
        &self.columns
    }

    /// Removes every column and the anchor; box size and reversal stay.
    pub fn clear(&mut self) {
        self.columns.clear();
        self.anchor = None;
        self.count = 0;
    }

    /// The price of grid line `box_index`.
    pub fn price_of(&self, box_index: i64) -> f64 {
        box_index as f64 * self.box_size
    }

    /// Highest box a price has reached, coming from below.
    fn up(&self, price: f64) -> i64 {
        (price / self.box_size + GRID_TOLERANCE).floor() as i64
    }

    /// Lowest box a price has reached, coming from above.
    fn down(&self, price: f64) -> i64 {
        (price / self.box_size - GRID_TOLERANCE).ceil() as i64
    }

    fn column(&self, direction: PnfDirection, top_box: i64, bottom_box: i64) -> PnfColumn {
        PnfColumn {
            direction,
            top: self.price_of(top_box),
            bottom: self.price_of(bottom_box),
            top_box,
            bottom_box,
            first_index: self.count - 1,
            last_index: self.count - 1,
        }
    }

    /// Adds one price.
    pub fn push(&mut self, price: f64) -> PnfChange {
        self.push_high_low(price, price)
    }

    /// Adds one bar by its high and low, read in the direction of the current column first.
    pub fn push_high_low(&mut self, high: f64, low: f64) -> PnfChange {
        self.count += 1;
        if !high.is_finite() || !low.is_finite() {
            return PnfChange::None;
        }
        let (up, down) = (self.up(high), self.down(low));
        let Some(current) = self.columns.last().copied() else {
            return self.push_before_first_column(up, down);
        };
        let reversal = self.reversal as i64;
        let (extended, reversed) = match current.direction {
            PnfDirection::Up if up > current.top_box => (Some((up, current.bottom_box)), None),
            PnfDirection::Down if down < current.bottom_box => {
                (Some((current.top_box, down)), None)
            }
            PnfDirection::Up if down <= current.top_box - reversal => {
                (None, Some((PnfDirection::Down, current.top_box - 1, down)))
            }
            PnfDirection::Down if up >= current.bottom_box + reversal => {
                (None, Some((PnfDirection::Up, up, current.bottom_box + 1)))
            }
            _ => (None, None),
        };
        if let Some((top_box, bottom_box)) = extended {
            let column = PnfColumn {
                first_index: current.first_index,
                ..self.column(current.direction, top_box, bottom_box)
            };
            *self.columns.last_mut().expect("a column exists") = column;
            return PnfChange::Extended;
        }
        let Some((direction, top_box, bottom_box)) = reversed else {
            return PnfChange::None;
        };
        let column = self.column(direction, top_box, bottom_box);
        self.columns.push(column);
        PnfChange::NewColumn
    }

    /// Before the first column: `up` and `down` against the anchor.
    fn push_before_first_column(&mut self, up: i64, down: i64) -> PnfChange {
        let Some((anchor_up, anchor_down)) = self.anchor else {
            self.anchor = Some((up, down));
            return PnfChange::None;
        };
        let first = match (up > anchor_up, down < anchor_down) {
            (true, false) => self.column(PnfDirection::Up, up, anchor_up),
            (false, true) => self.column(PnfDirection::Down, anchor_down, down),
            _ => {
                self.anchor = Some((up.max(anchor_up), down.min(anchor_down)));
                return PnfChange::None;
            }
        };
        self.columns.push(first);
        PnfChange::NewColumn
    }

    /// The horizontal count from column `last` back to column `first` (indices into
    /// [`PnfChart::columns`]), both included.
    ///
    /// `None` unless `first <= last`, both exist and both are columns of the same kind: the
    /// count of a base runs between two O columns and projects upward, the count of a top
    /// between two X columns and projects downward.
    pub fn horizontal_count(&self, first: usize, last: usize) -> Option<HorizontalCount> {
        if first > last {
            return None;
        }
        let counted = self.columns.get(first..=last)?;
        let (start, end) = (counted[0], counted[counted.len() - 1]);
        if start.direction != end.direction {
            return None;
        }
        // O columns: the base of a rise. The line and the extreme are lows, the effect is added.
        let base = end.direction == PnfDirection::Down;
        let line_box = if base { end.bottom_box } else { end.top_box };
        let extreme_box = if base {
            counted.iter().map(|c| c.bottom_box).min()
        } else {
            counted.iter().map(|c| c.top_box).max()
        }
        .expect("at least one column");
        let columns = counted.len();
        let columns_on_line = counted.iter().filter(|c| c.contains(line_box)).count();
        let unit = self.reversal as f64 * self.box_size;
        let (effect, effect_on_line) = (columns as f64 * unit, columns_on_line as f64 * unit);
        let (count_line, extreme) = (self.price_of(line_box), self.price_of(extreme_box));
        let sign = if base { 1.0 } else { -1.0 };
        Some(HorizontalCount {
            first_column: first,
            last_column: last,
            direction: if base {
                PnfDirection::Up
            } else {
                PnfDirection::Down
            },
            count_line,
            extreme,
            columns,
            columns_on_line,
            effect,
            effect_on_line,
            target_from_extreme: extreme + sign * effect,
            target_from_line: count_line + sign * effect,
        })
    }
}

/// The point-and-figure chart of `prices` with boxes of `box_size` price units and a reversal of
/// `reversal` boxes: [`PnfChart::push`] for every price in order.
///
/// # Panics
///
/// If `box_size` is not a positive, finite number.
pub fn point_and_figure(prices: &[f64], box_size: f64, reversal: usize) -> PnfChart {
    let mut chart = PnfChart::new(box_size, reversal);
    for &price in prices {
        chart.push(price);
    }
    chart
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxes(chart: &PnfChart) -> Vec<(char, i64, i64)> {
        chart
            .columns()
            .iter()
            .map(|c| (c.direction.symbol(), c.bottom_box, c.top_box))
            .collect()
    }

    #[test]
    fn test_first_column_starts_at_the_anchor_box_on_its_side() {
        // 120.5 has reached 120 from below and 121 from above.
        let rising = point_and_figure(&[120.5, 120.9, 123.2], 1.0, 3);
        assert_eq!(boxes(&rising), [('X', 120, 123)]);
        assert_eq!(rising.columns()[0].first_index, 2);
        let falling = point_and_figure(&[120.5, 120.1, 117.8], 1.0, 3);
        assert_eq!(boxes(&falling), [('O', 118, 121)]);
    }

    #[test]
    fn test_reversal_needs_the_full_number_of_boxes() {
        // X up to 111: 108.01 has not reached 108, 108 has.
        let short = point_and_figure(&[100.0, 111.0, 108.01], 1.0, 3);
        assert_eq!(boxes(&short), [('X', 100, 111)]);
        let full = point_and_figure(&[100.0, 111.0, 108.0], 1.0, 3);
        assert_eq!(boxes(&full), [('X', 100, 111), ('O', 108, 110)]);
        // O down to 100: 102.99 has not reached 103.
        let short = point_and_figure(&[111.0, 100.0, 102.99], 1.0, 3);
        assert_eq!(boxes(&short), [('O', 100, 111)]);
        let full = point_and_figure(&[111.0, 100.0, 103.0], 1.0, 3);
        assert_eq!(boxes(&full), [('O', 100, 111), ('X', 101, 103)]);
    }

    #[test]
    fn test_decimal_box_sizes_keep_prices_on_their_grid_line() {
        // 0.3 / 0.1 is 2.9999999999999996 in binary floating point.
        let chart = point_and_figure(&[0.0, 0.3, 0.0], 0.1, 3);
        assert_eq!(boxes(&chart), [('X', 0, 3), ('O', 0, 2)]);
        assert!((chart.columns()[0].top - 0.3).abs() < 1e-12);
    }

    #[test]
    fn test_high_low_reads_the_bar_in_the_direction_of_the_column_first() {
        let mut chart = PnfChart::new(1.0, 3);
        chart.push_high_low(100.0, 100.0);
        assert_eq!(chart.push_high_low(105.0, 100.0), PnfChange::NewColumn);
        // A new high and a low three boxes down on one bar: the column is extended, not reversed.
        assert_eq!(chart.push_high_low(106.0, 101.0), PnfChange::Extended);
        assert_eq!(boxes(&chart), [('X', 100, 106)]);
        // No new high: now the low counts.
        assert_eq!(chart.push_high_low(106.9, 103.0), PnfChange::NewColumn);
        assert_eq!(boxes(&chart), [('X', 100, 106), ('O', 103, 105)]);
        // In the O column the low is read first.
        assert_eq!(chart.push_high_low(107.0, 102.0), PnfChange::Extended);
        assert_eq!(chart.push_high_low(105.0, 102.5), PnfChange::NewColumn);
        assert_eq!(
            boxes(&chart),
            [('X', 100, 106), ('O', 102, 105), ('X', 103, 105)]
        );
    }

    #[test]
    fn test_a_bar_beyond_both_sides_of_the_anchor_only_widens_it() {
        let mut chart = PnfChart::new(1.0, 3);
        chart.push_high_low(100.5, 99.5);
        assert_eq!(chart.push_high_low(102.0, 98.0), PnfChange::None);
        assert!(chart.columns().is_empty());
        assert_eq!(chart.push_high_low(101.5, 97.0), PnfChange::NewColumn);
        assert_eq!(boxes(&chart), [('O', 97, 98)]);
    }

    #[test]
    fn test_non_finite_prices_change_nothing_but_keep_the_position() {
        let mut chart = PnfChart::new(1.0, 3);
        chart.push(100.0);
        assert_eq!(chart.push(f64::NAN), PnfChange::None);
        chart.push(103.0);
        assert_eq!(boxes(&chart), [('X', 100, 103)]);
        assert_eq!(chart.columns()[0].first_index, 2);
        chart.clear();
        assert!(chart.columns().is_empty());
        chart.push(50.0);
        chart.push(51.0);
        assert_eq!(chart.columns()[0].first_index, 1);
    }

    #[test]
    fn test_horizontal_count_needs_two_columns_of_one_kind_in_order() {
        let chart = point_and_figure(&[110.0, 100.0, 104.0, 101.0, 106.0], 1.0, 3);
        assert_eq!(
            boxes(&chart),
            [
                ('O', 100, 110),
                ('X', 101, 104),
                ('O', 101, 103),
                ('X', 102, 106)
            ]
        );
        assert!(chart.horizontal_count(0, 1).is_none());
        assert!(chart.horizontal_count(2, 0).is_none());
        assert!(chart.horizontal_count(0, 4).is_none());
        let count = chart.horizontal_count(0, 2).unwrap();
        assert_eq!(count.direction, PnfDirection::Up);
        assert_eq!((count.columns, count.columns_on_line), (3, 3));
        assert_eq!((count.count_line, count.extreme), (101.0, 100.0));
        assert_eq!(count.effect, 9.0);
        assert_eq!(
            (count.target_from_extreme, count.target_from_line),
            (109.0, 110.0)
        );
        // Over the two X columns the distance is projected downward from their highs.
        let top = chart.horizontal_count(1, 3).unwrap();
        assert_eq!(top.direction, PnfDirection::Down);
        assert_eq!((top.count_line, top.extreme), (106.0, 106.0));
        assert_eq!((top.columns, top.columns_on_line), (3, 1));
        assert_eq!(top.target_from_line, 97.0);
    }

    #[test]
    #[should_panic(expected = "box size must be positive")]
    fn test_box_size_must_be_positive() {
        PnfChart::new(0.0, 3);
    }
}
