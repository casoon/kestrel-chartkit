//! Golden-reference tests for point-and-figure charts and the horizontal count against
//! `reference/kestrel_reference/fixtures/point_and_figure.py` (exact rational arithmetic in price
//! space; the columns of the worked example are also written out by hand there).

mod common;

use kestrel_chartkit::{point_and_figure, HorizontalCount, PnfChart, PnfColumn, PnfDirection};

const GOLDEN: &str = include_str!("fixtures/golden_point_and_figure.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

struct Case {
    index: usize,
    box_size: f64,
    reversal: usize,
    /// (high, low) per input; a single price twice.
    inputs: Vec<(f64, f64)>,
}

impl Case {
    fn key(&self, name: &str) -> f64 {
        value(&format!("pnf{}_{name}", self.index))
    }

    fn chart(&self) -> PnfChart {
        chart_of(&self.inputs, self.box_size, self.reversal)
    }

    /// The end columns of the case's horizontal count, if it has one.
    fn count_columns(&self) -> Option<(usize, usize)> {
        (self.key("count") == 1.0).then(|| {
            (
                self.key("count_first") as usize,
                self.key("count_last") as usize,
            )
        })
    }
}

fn chart_of(inputs: &[(f64, f64)], box_size: f64, reversal: usize) -> PnfChart {
    let mut chart = PnfChart::new(box_size, reversal);
    for &(high, low) in inputs {
        chart.push_high_low(high, low);
    }
    chart
}

fn case(index: usize) -> Case {
    let key = |name: &str| value(&format!("pnf{index}_{name}"));
    let pairs = key("method") == 1.0;
    let inputs = (0..key("n") as usize)
        .map(|j| {
            if pairs {
                (key(&format!("h{j}")), key(&format!("l{j}")))
            } else {
                (key(&format!("p{j}")), key(&format!("p{j}")))
            }
        })
        .collect();
    Case {
        index,
        box_size: key("box"),
        reversal: key("reversal") as usize,
        inputs,
    }
}

fn cases() -> Vec<Case> {
    let count = value("meta_case_count") as usize;
    assert!(count >= 9);
    (0..count).map(case).collect()
}

fn boxes(columns: &[PnfColumn]) -> Vec<(char, i64, i64)> {
    columns
        .iter()
        .map(|c| (c.direction.symbol(), c.bottom_box, c.top_box))
        .collect()
}

#[test]
fn test_point_and_figure_columns_match_the_reference() {
    let tolerance = value("point_and_figure_tolerance");
    for c in cases() {
        let chart = c.chart();
        let columns = chart.columns();
        assert_eq!(columns.len(), c.key("columns") as usize, "case {}", c.index);
        for (k, column) in columns.iter().enumerate() {
            let at = format!("case {} column {k}", c.index);
            let key = |name: &str| c.key(&format!("c{k}_{name}"));
            assert_eq!(column.direction.code(), key("dir"), "{at}: direction");
            assert_eq!(column.top_box as f64, key("top_box"), "{at}: top box");
            assert_eq!(
                column.bottom_box as f64,
                key("bottom_box"),
                "{at}: bottom box"
            );
            common::assert_close(column.top, key("top"), tolerance, &format!("{at}: top"));
            common::assert_close(
                column.bottom,
                key("bottom"),
                tolerance,
                &format!("{at}: bottom"),
            );
            assert_eq!(column.first_index as f64, key("first"), "{at}: first");
            assert_eq!(column.last_index as f64, key("last"), "{at}: last");
            assert_eq!(
                column.boxes() as i64,
                column.top_box - column.bottom_box + 1
            );
        }
    }
}

#[test]
fn test_horizontal_count_matches_the_reference() {
    let tolerance = value("point_and_figure_tolerance");
    let mut counted = 0;
    for c in cases() {
        let Some((first, last)) = c.count_columns() else {
            continue;
        };
        counted += 1;
        let at = format!("case {}", c.index);
        let count = c.chart().horizontal_count(first, last).expect("a count");
        assert_eq!((count.first_column, count.last_column), (first, last));
        assert_eq!(count.direction.code(), c.key("count_dir"), "{at}");
        assert_eq!(count.columns as f64, c.key("count_columns"), "{at}");
        assert_eq!(count.columns_on_line as f64, c.key("count_on_line"), "{at}");
        for (name, actual) in [
            ("count_line", count.count_line),
            ("count_extreme", count.extreme),
            ("count_effect", count.effect),
            ("count_effect_on_line", count.effect_on_line),
            ("count_target_extreme", count.target_from_extreme),
            ("count_target_line", count.target_from_line),
        ] {
            common::assert_close(actual, c.key(name), tolerance, &format!("{at}: {name}"));
        }
    }
    assert!(counted >= 6);
}

/// The worked example by hand: a decline to 100, a trading range and a breakout. With a box of 1
/// and a reversal of 3 there are 20 columns; the first is the O column down to 100, the 19th the
/// last O column before the breakout, with its lowest box at 105. From column 19 back to column
/// 1 lie 19 columns, so the effect is 19 * 3 * 1 = 57: 100 + 57 = 157 from the low of the range,
/// 105 + 57 = 162 from the count line. Three of the 19 columns (8, 15 and 16) have no box on
/// 105, so the count along the line is 16 and its effect 48.
#[test]
fn test_worked_example_by_hand() {
    let prices = [
        120.0, 100.0, 111.0, 104.0, 108.0, 101.0, 109.0, 105.0, 110.0, 103.0, 108.0, 104.0, 107.0,
        102.0, 110.0, 106.0, 111.0, 104.0, 109.0, 105.0, 108.0, 113.0, 117.0, 120.0,
    ];
    let chart = point_and_figure(&prices, 1.0, 3);
    assert_eq!(
        boxes(chart.columns()),
        [
            ('O', 100, 120),
            ('X', 101, 111),
            ('O', 104, 110),
            ('X', 105, 108),
            ('O', 101, 107),
            ('X', 102, 109),
            ('O', 105, 108),
            ('X', 106, 110),
            ('O', 103, 109),
            ('X', 104, 108),
            ('O', 104, 107),
            ('X', 105, 107),
            ('O', 102, 106),
            ('X', 103, 110),
            ('O', 106, 109),
            ('X', 107, 111),
            ('O', 104, 110),
            ('X', 105, 109),
            ('O', 105, 108),
            ('X', 106, 120),
        ]
    );
    let count = chart.horizontal_count(0, 18).unwrap();
    assert_eq!(
        count,
        HorizontalCount {
            first_column: 0,
            last_column: 18,
            direction: PnfDirection::Up,
            count_line: 105.0,
            extreme: 100.0,
            columns: 19,
            columns_on_line: 16,
            effect: 57.0,
            effect_on_line: 48.0,
            target_from_extreme: 157.0,
            target_from_line: 162.0,
        }
    );
    let off_the_line: Vec<usize> = (0..19)
        .filter(|&k| !chart.columns()[k].contains(105))
        .map(|k| k + 1)
        .collect();
    assert_eq!(off_the_line, [8, 15, 16]);

    // The reversal is a factor of the effect: with a reversal of 1 the same 20 columns form
    // (every swing is at least three boxes long) and the effect is 19 * 1 * 1.
    let one_box = point_and_figure(&prices, 1.0, 1);
    assert_eq!(boxes(one_box.columns()), boxes(chart.columns()));
    assert_eq!(one_box.horizontal_count(0, 18).unwrap().effect, 19.0);

    // With a box of 2 a column turns after 6 points. 103 to 108 and 104 to 109 are moves of 5
    // and open no column, so 8 columns remain: 7 counted, effect 7 * 3 * 2 = 42.
    let wide = point_and_figure(&prices, 2.0, 3);
    assert_eq!(
        boxes(wide.columns()),
        [
            ('O', 50, 60),
            ('X', 51, 55),
            ('O', 51, 54),
            ('X', 52, 55),
            ('O', 51, 54),
            ('X', 52, 55),
            ('O', 52, 54),
            ('X', 53, 60),
        ]
    );
    let count = wide.horizontal_count(0, 6).unwrap();
    assert_eq!((count.columns, count.columns_on_line), (7, 7));
    assert_eq!((count.count_line, count.extreme), (104.0, 100.0));
    assert_eq!(count.effect, 42.0);
    assert_eq!(
        (count.target_from_extreme, count.target_from_line),
        (142.0, 146.0)
    );
}

/// The fixture carries the same count under the other reading of a box (the price interval above
/// its line, filled by every price in it). It agrees with this crate while all prices are on
/// grid lines and differs for a box of 2; the numbers are stated here so the difference is
/// visible, they are not what the crate computes.
#[test]
fn test_interval_reading_is_a_documented_alternative() {
    let alternative = |index: usize| {
        let key = |name: &str| value(&format!("pnf{index}_alt_interval_{name}"));
        (key("columns"), key("count"), key("on_line"), key("effect"))
    };
    let ours = |index: usize| {
        let c = case(index);
        let (first, last) = c.count_columns().unwrap();
        let chart = c.chart();
        let count = chart.horizontal_count(first, last).unwrap();
        (
            chart.columns().len() as f64,
            count.columns as f64,
            count.columns_on_line as f64,
            count.effect,
        )
    };
    // Box 1, reversal 3 and 1: every price on a grid line, both readings agree.
    assert_eq!(alternative(0), (20.0, 19.0, 16.0, 57.0));
    assert_eq!(ours(0), alternative(0));
    assert_eq!(ours(1), alternative(1));
    // Box 2: 10 columns, count 9, effect 54 there; 8 columns, count 7, effect 42 here.
    assert_eq!(alternative(2), (10.0, 9.0, 9.0, 54.0));
    assert_eq!(ours(2), (8.0, 7.0, 7.0, 42.0));
    assert_eq!(alternative(3), (18.0, 17.0, 12.0, 68.0));
    assert_eq!(ours(3), (14.0, 13.0, 9.0, 52.0));
}

/// Rising and falling are mirror images: the chart of a series reflected at a grid line has the
/// same columns with X for O and every box reflected, opened and last extended by the same
/// inputs, and the horizontal count over them projects the same distance the other way. Checked
/// on every case; for highs and lows the reflected high is the level minus the low.
#[test]
fn test_point_and_figure_mirror_symmetry() {
    const LINE: i64 = 512;
    for c in cases() {
        let level = LINE as f64 * c.box_size;
        let reflected: Vec<(f64, f64)> = c
            .inputs
            .iter()
            .map(|&(high, low)| (level - low, level - high))
            .collect();
        let plain = c.chart();
        let mirrored = chart_of(&reflected, c.box_size, c.reversal);
        assert_eq!(
            plain.columns().len(),
            mirrored.columns().len(),
            "case {}",
            c.index
        );
        assert!(!plain.columns().is_empty());
        for (k, (a, b)) in plain.columns().iter().zip(mirrored.columns()).enumerate() {
            let at = format!("case {} column {k}", c.index);
            assert_ne!(a.direction, b.direction, "{at}: direction");
            assert_eq!(b.top_box, LINE - a.bottom_box, "{at}: top box");
            assert_eq!(b.bottom_box, LINE - a.top_box, "{at}: bottom box");
            assert_eq!(
                (b.first_index, b.last_index),
                (a.first_index, a.last_index),
                "{at}: inputs"
            );
        }
        let Some((first, last)) = c.count_columns() else {
            continue;
        };
        let a = plain.horizontal_count(first, last).unwrap();
        let b = mirrored.horizontal_count(first, last).unwrap();
        let at = format!("case {} count", c.index);
        assert_ne!(a.direction, b.direction, "{at}");
        assert_eq!(
            (a.columns, a.columns_on_line),
            (b.columns, b.columns_on_line)
        );
        for (name, actual, expected) in [
            ("line", b.count_line, level - a.count_line),
            ("extreme", b.extreme, level - a.extreme),
            ("effect", b.effect, a.effect),
            ("effect on line", b.effect_on_line, a.effect_on_line),
            (
                "target from extreme",
                b.target_from_extreme,
                level - a.target_from_extreme,
            ),
            (
                "target from line",
                b.target_from_line,
                level - a.target_from_line,
            ),
        ] {
            common::assert_close(actual, expected, 1e-9, &format!("{at}: {name}"));
        }
    }
}

/// One price at a time gives the chart of the whole slice, and a prefix of the inputs never
/// shows a column that the full series takes back: columns only grow in their own direction.
#[test]
fn test_columns_are_causal() {
    for c in cases() {
        let full = c.chart();
        let mut chart = PnfChart::new(c.box_size, c.reversal);
        for &(high, low) in &c.inputs {
            chart.push_high_low(high, low);
            let n = chart.columns().len();
            if n == 0 {
                continue;
            }
            // Every closed column is final; the open one lies inside its final extent.
            assert_eq!(chart.columns()[..n - 1], full.columns()[..n - 1]);
            let (open, done) = (chart.columns()[n - 1], full.columns()[n - 1]);
            assert_eq!(open.direction, done.direction);
            assert_eq!(open.first_index, done.first_index);
            assert!(open.bottom_box >= done.bottom_box && open.top_box <= done.top_box);
        }
        assert_eq!(chart, full);
    }
}
