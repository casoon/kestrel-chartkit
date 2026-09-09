mod common;
use kestrel_chartkit::{
    analytics::*,
    indicator::{adx::Adx, efficiency::LegEfficiencyEngine, smoothing::Sma},
    Bar, Indicator,
};
const GOLDEN: &str = include_str!("fixtures/golden_analytics_components.txt");
fn check(key: &str, actual: f64) {
    common::assert_close(actual, common::golden_value(GOLDEN, key), 1e-9, key);
}
fn ramp() -> Vec<Bar> {
    (0..120)
        .map(|i| {
            let c = 100. + i as f64 * 0.5;
            Bar::new(i, c, c + 0.2, c - 0.2, c, 1.)
        })
        .collect()
}
#[test]
fn independent_trend_component_references() {
    let bars = ramp();
    let mut adx = Adx::new(14, 14, 3, 20.);
    let mut last = None;
    for b in &bars {
        if let Some(v) = adx.on_bar(b) {
            last = Some(v.value);
        }
    }
    check("ramp_adx", last.unwrap());
    let mut er = LegEfficiencyEngine::new(34);
    let mut last = None;
    for b in &bars {
        last = er.on_bar(b).map(|v| v.value);
    }
    check("ramp_er", last.unwrap());
    let mut sma = Sma::new(5);
    let series: Vec<_> = bars.iter().filter_map(|b| sma.update(b.close)).collect();
    check("ramp_sma5_last", *series.last().unwrap());
    check("ramp_sma5_prior", series[series.len() - 6]);
    let r = classify_trend_regime(&bars, 14, 20).unwrap();
    check("ramp_chop", r.choppiness);
    check("ramp_er", r.efficiency);
    check("ramp_adx", r.adx);
    let p = trend_persistence_reading(&bars).unwrap();
    check("ramp_r2_score", p.r2_score);
    check("ramp_er", p.er_score / 100.);
    check("ramp_fdi_score", p.fdi_score);
}
#[test]
fn independent_wilder_price_reference() {
    let bars: Vec<_> = [10., 11., 12., 11., 13., 14., 13., 15., 16., 15.]
        .iter()
        .map(|&c| Bar::new(0, c, c + 1., c - 1., c, 1.))
        .collect();
    let p = price_summary(&bars, 3, 1.5).unwrap();
    check("price_atr", p.atr);
    check("price_atr_pct", p.atr_pct);
    check("price_atr_rank", p.atr_percentile);
    check("price_stop", p.stop_distance);
    check("price_change_pct", p.change_pct);
    check("price_range_position", p.range_position);
}
#[test]
fn independent_fear_and_volume_components() {
    let mut bars = vec![Bar::new(0, 100., 101., 99., 100., 1.); 40];
    bars.push(Bar::new(0, 100., 101., 80., 100., 1.));
    let f = fear_gauge_reading(&bars).unwrap();
    check("fear_wvf", f.wvf);
    check("fear_bwvf", f.bwvf);
    assert!(f.absorbed);
    assert_eq!(f.state, FearGaugeState::FearSpike);
    let bars: Vec<_> = [1., 4., 2., 3.]
        .iter()
        .map(|&v| Bar::new(0, 100., 101., 99., 100., v))
        .collect();
    check(
        "volume_rank",
        activity_reading(&bars, 0., true, 4)
            .volume_percentile
            .unwrap(),
    );
}
