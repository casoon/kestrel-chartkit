//! Golden-reference tests for the result of an option position at expiry: single prices, the
//! profile (break-even, largest profit, largest loss) and the series over a price grid. Values
//! from `reference/kestrel_reference/fixtures/option_expiry.py` (exact rational arithmetic from
//! the piecewise-linear form, plus the example worked by hand).

mod common;

use kestrel_chartkit::option::{
    black_scholes_merton, option_expiry_profile, option_expiry_result, option_expiry_series,
    BlackScholesInputs, OptionError, OptionExpiryPosition, OptionType, PayoffBound,
    MAX_EXPIRY_SERIES_POINTS,
};
use kestrel_chartkit::portfolio::PositionSide;

const GOLDEN: &str = include_str!("fixtures/golden_option_expiry.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn near(got: f64, key: &str) {
    let want = value(key);
    let tolerance = value("option_expiry_tolerance") * want.abs().max(1.0);
    assert!(
        (got - want).abs() <= tolerance,
        "{key}: got {got}, reference {want}"
    );
}

fn position(i: usize) -> OptionExpiryPosition {
    let key = |name: &str| value(&format!("pos{i}_{name}"));
    OptionExpiryPosition {
        option_type: if key("call") == 1.0 {
            OptionType::Call
        } else {
            OptionType::Put
        },
        side: if key("long") == 1.0 {
            PositionSide::Long
        } else {
            PositionSide::Short
        },
        strike: key("strike"),
        premium: key("premium"),
        contract_size: key("contract_size"),
    }
}

fn count() -> usize {
    let positions = value("meta_position_count") as usize;
    assert!(positions >= 12);
    positions
}

fn example(option_type: OptionType, side: PositionSide) -> OptionExpiryPosition {
    OptionExpiryPosition {
        option_type,
        side,
        strike: 100.0,
        premium: 4.0,
        contract_size: 1.0,
    }
}

/// `Limited(x)` against `<key>` or `Unlimited` against `<key>_unlimited = 1`.
fn bound(got: PayoffBound, key: &str) {
    if value(&format!("{key}_unlimited")) == 1.0 {
        assert_eq!(got, PayoffBound::Unlimited, "{key}");
        assert_eq!(got.limited(), None);
    } else {
        let amount = got
            .limited()
            .unwrap_or_else(|| panic!("{key}: expected a limit"));
        assert!(amount >= 0.0, "{key}");
        near(amount, key);
    }
}

#[test]
fn test_result_at_single_prices_matches_the_reference() {
    for i in 0..count() {
        let position = position(i);
        for j in 0..value(&format!("pos{i}_n")) as usize {
            let underlying = value(&format!("pos{i}_s{j}"));
            let got = option_expiry_result(&position, underlying).unwrap();
            near(got, &format!("pos{i}_result{j}"));
        }
    }
}

#[test]
fn test_profile_matches_the_reference() {
    for i in 0..count() {
        let position = position(i);
        let profile = option_expiry_profile(&position).unwrap();
        near(profile.break_even, &format!("pos{i}_break_even"));
        bound(profile.max_profit, &format!("pos{i}_max_profit"));
        bound(profile.max_loss, &format!("pos{i}_max_loss"));

        // The break-even is a price at which the result is zero.
        let at_break_even = option_expiry_result(&position, profile.break_even).unwrap();
        assert!(
            at_break_even.abs() <= 1e-9 * position.contract_size,
            "pos{i}"
        );
    }
}

#[test]
fn test_series_over_a_price_grid_matches_the_reference() {
    for i in 0..count() {
        let position = position(i);
        let key = |name: &str| value(&format!("pos{i}_grid_{name}"));
        let (from, to, points) = (key("from"), key("to"), key("points") as usize);
        let series = option_expiry_series(&position, from, to, points).unwrap();
        assert_eq!(series.len(), points, "pos{i}");
        assert_eq!(series[0].underlying, from, "pos{i}");
        assert_eq!(series[points - 1].underlying, to, "pos{i}");
        for (j, point) in series.iter().enumerate() {
            near(point.underlying, &format!("pos{i}_grid_s{j}"));
            near(point.result, &format!("pos{i}_grid_result{j}"));
            assert_eq!(
                point.result,
                option_expiry_result(&position, point.underlying).unwrap(),
                "pos{i} point {j}"
            );
        }
    }
}

/// The example worked by hand: strike 100, premium 4, per unit.
#[test]
fn test_worked_example_with_strike_100_and_premium_4() {
    use OptionType::{Call, Put};
    use PositionSide::{Long, Short};
    let result = |option_type, side, underlying| {
        option_expiry_result(&example(option_type, side), underlying).unwrap()
    };
    assert_eq!(result(Call, Long, 108.0), 4.0);
    assert_eq!(result(Call, Short, 108.0), -4.0);
    assert_eq!(result(Call, Long, 95.0), -4.0);
    assert_eq!(result(Call, Short, 95.0), 4.0);
    assert_eq!(result(Put, Long, 95.0), 1.0);
    assert_eq!(result(Put, Short, 0.0), -96.0);

    let profile = |option_type, side| option_expiry_profile(&example(option_type, side)).unwrap();
    assert_eq!(profile(Call, Long).break_even, 104.0);
    assert_eq!(profile(Call, Short).break_even, 104.0);
    assert_eq!(profile(Put, Long).break_even, 96.0);
    assert_eq!(profile(Call, Long).max_profit, PayoffBound::Unlimited);
    assert_eq!(profile(Call, Long).max_loss, PayoffBound::Limited(4.0));
    assert_eq!(profile(Call, Short).max_profit, PayoffBound::Limited(4.0));
    assert_eq!(profile(Call, Short).max_loss, PayoffBound::Unlimited);
    assert_eq!(profile(Put, Long).max_profit, PayoffBound::Limited(96.0));
    assert_eq!(profile(Put, Long).max_loss, PayoffBound::Limited(4.0));
    assert_eq!(profile(Put, Short).max_profit, PayoffBound::Limited(4.0));
    assert_eq!(profile(Put, Short).max_loss, PayoffBound::Limited(96.0));
}

/// Bought and sold are the two sides of one trade: their results add up to zero at every price,
/// and a result of zero is `0.0` on both sides.
#[test]
fn test_the_two_sides_of_a_trade_add_up_to_zero() {
    for option_type in [OptionType::Call, OptionType::Put] {
        let long = example(option_type, PositionSide::Long);
        let short = example(option_type, PositionSide::Short);
        let bought = option_expiry_series(&long, 0.0, 250.0, 501).unwrap();
        let sold = option_expiry_series(&short, 0.0, 250.0, 501).unwrap();
        for (b, s) in bought.iter().zip(&sold) {
            assert_eq!(b.underlying, s.underlying);
            assert_eq!(b.result + s.result, 0.0);
            assert!(b.result.is_finite() && s.result.is_finite());
            for result in [b.result, s.result] {
                assert!(result != 0.0 || result.is_sign_positive());
            }
        }
        let (long_profile, short_profile) = (
            option_expiry_profile(&long).unwrap(),
            option_expiry_profile(&short).unwrap(),
        );
        assert_eq!(long_profile.max_profit, short_profile.max_loss);
        assert_eq!(long_profile.max_loss, short_profile.max_profit);
    }
}

/// No result over the grid lies outside the profile's bounds, and each limited bound is reached.
#[test]
fn test_series_stays_within_the_profile() {
    for i in 0..count() {
        let position = position(i);
        let profile = option_expiry_profile(&position).unwrap();
        let series = option_expiry_series(&position, 0.0, 4.0 * position.strike, 4001).unwrap();
        let highest = series.iter().map(|p| p.result).fold(f64::MIN, f64::max);
        let lowest = series.iter().map(|p| p.result).fold(f64::MAX, f64::min);
        let slack = 1e-9 * position.contract_size * position.strike;
        if let Some(limit) = profile.max_profit.limited() {
            assert!((highest - limit).abs() <= slack, "pos{i}: {highest}");
        }
        if let Some(limit) = profile.max_loss.limited() {
            assert!((lowest + limit).abs() <= slack, "pos{i}: {lowest}");
        }
    }
}

/// With the premium added back, the bought option's result per unit is the option's value with
/// no time left, as `black_scholes_merton` returns it.
#[test]
fn test_result_plus_premium_is_the_value_at_expiry_of_the_pricing_model() {
    for option_type in [OptionType::Call, OptionType::Put] {
        let position = example(option_type, PositionSide::Long);
        for spot in [60.0, 95.0, 100.0, 104.0, 133.5] {
            let at_expiry = black_scholes_merton(
                option_type,
                &BlackScholesInputs {
                    spot,
                    strike: position.strike,
                    time_to_expiry_years: 0.0,
                    risk_free_rate: 0.03,
                    dividend_yield: 0.01,
                    volatility: 0.25,
                },
            )
            .unwrap();
            let result = option_expiry_result(&position, spot).unwrap();
            assert_eq!(result + position.premium, at_expiry.price);
        }
    }
}

#[test]
fn test_invalid_positions_prices_and_grids_are_rejected() {
    let valid = example(OptionType::Put, PositionSide::Long);
    assert!(option_expiry_profile(&valid).is_ok());

    for broken in [
        OptionExpiryPosition {
            strike: 0.0,
            ..valid
        },
        OptionExpiryPosition {
            strike: f64::INFINITY,
            ..valid
        },
        OptionExpiryPosition {
            premium: -0.01,
            ..valid
        },
        OptionExpiryPosition {
            premium: f64::NAN,
            ..valid
        },
        OptionExpiryPosition {
            contract_size: 0.0,
            ..valid
        },
        OptionExpiryPosition {
            contract_size: -1.0,
            ..valid
        },
    ] {
        assert!(matches!(
            option_expiry_profile(&broken),
            Err(OptionError::InvalidInput(_))
        ));
        assert!(option_expiry_result(&broken, 100.0).is_err());
        assert!(option_expiry_series(&broken, 90.0, 110.0, 3).is_err());
    }

    // A put cannot pay more than its strike, so it cannot cost more; a call has no such bound.
    let overpaid = OptionExpiryPosition {
        premium: 100.5,
        ..valid
    };
    assert_eq!(
        option_expiry_profile(&overpaid),
        Err(OptionError::PriceAboveBoundary)
    );
    assert!(option_expiry_profile(&OptionExpiryPosition {
        option_type: OptionType::Call,
        ..overpaid
    })
    .is_ok());

    for underlying in [-0.01, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            option_expiry_result(&valid, underlying),
            Err(OptionError::InvalidInput(_))
        ));
    }
    assert_eq!(option_expiry_result(&valid, 0.0), Ok(96.0));

    for (from, to, points) in [
        (110.0, 90.0, 5),
        (90.0, 90.0, 5),
        (-1.0, 90.0, 5),
        (90.0, f64::INFINITY, 5),
        (f64::NAN, 110.0, 5),
        (90.0, 110.0, 1),
        (90.0, 110.0, 0),
        (90.0, 110.0, MAX_EXPIRY_SERIES_POINTS + 1),
    ] {
        assert!(
            matches!(
                option_expiry_series(&valid, from, to, points),
                Err(OptionError::InvalidInput(_))
            ),
            "{from} {to} {points}"
        );
    }
    let widest = option_expiry_series(&valid, 90.0, 110.0, MAX_EXPIRY_SERIES_POINTS).unwrap();
    assert_eq!(widest.len(), MAX_EXPIRY_SERIES_POINTS);
    assert_eq!(
        option_expiry_series(&valid, 90.0, 110.0, 2).unwrap().len(),
        2
    );
}
