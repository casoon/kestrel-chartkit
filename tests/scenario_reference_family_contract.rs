//! Vertikale Abnahme des Familienvertrags (Paket 10, Schritt 7).
//!
//! Ein lineares Future-Produkt in Fremdwährung wandert hier durch die ganze Kette: von der
//! Kontraktspezifikation über die Positionsgröße aus dem Risiko, den preisbasierten
//! Portfolio-Snapshot und die modellbasierte Neubewertung bis zum Ergebnisvertrag, den ein
//! Consumer weiterreicht. Geprüft wird, dass die Stücke dieselben Zahlen sehen — ein Aggregat,
//! das von seinen Bestandteilen abweicht, wäre ein zweiter Rechenweg.

use kestrel_chartkit::contract::{ContractSpec, Currency, FxRate, InstrumentType};
use kestrel_chartkit::finance::{Date, DayCountConvention};
use kestrel_chartkit::portfolio::{evaluate_portfolio, CashLedger, PositionSide, PositionSnapshot};
use kestrel_chartkit::risk::{position_size_contract, AccountRisk};
use kestrel_chartkit::valuation::portfolio::{
    MarketScenario, PortfolioReport, SensitivityKind, ValuationModel, ValuationPosition,
    ValuedInstrument,
};
use kestrel_chartkit::valuation::{DiscountCurve, ValuationContext, YieldCurve};

mod common;

/// Ein USD-notierter Index-Future mit Punktwert 50 und ganzzahliger Kontraktmenge.
fn future_spec() -> ContractSpec {
    ContractSpec {
        price_currency: Currency::usd(),
        settlement_currency: Currency::usd(),
        multiplier: 50.0,
        quantity_step: 1.0,
        min_quantity: 1.0,
        instrument_type: InstrumentType::LinearFuture,
    }
}

const ENTRY: f64 = 4_500.0;
const STOP: f64 = 4_460.0;
const USD_PER_EUR: f64 = 0.9;

#[test]
fn test_vertical_slice_from_contract_spec_to_consumer_report() {
    let spec = future_spec();
    let account_currency = Currency::eur();

    // 1. Risiko und Kontraktspezifikation ergeben die Stückzahl.
    let account = AccountRisk {
        equity: 250_000.0,
        risk_pct_per_trade: 0.01,
        max_leverage: 5.0,
        max_position_notional: None,
    };
    let sizing = position_size_contract(&account, &spec, ENTRY, STOP, Some(USD_PER_EUR)).unwrap();
    assert!(
        sizing.size >= spec.min_quantity,
        "die Größe muss die Mindestmenge erfüllen"
    );
    assert_eq!(
        sizing.size.fract(),
        0.0,
        "ganzzahlige Kontrakte, wie die Spezifikation es verlangt"
    );

    // 2. Preisbasierter Snapshot: dieselbe Position, aus Sicht des vorhandenen Portfolios.
    let current_price = 4_530.0;
    let snapshot = evaluate_portfolio(
        account_currency.clone(),
        &CashLedger {
            cash: 100_000.0,
            ..CashLedger::default()
        },
        &[PositionSnapshot {
            symbol: "ESZ6".to_string(),
            spec: spec.clone(),
            side: PositionSide::Long,
            quantity: sizing.size,
            entry_price: ENTRY,
            current_price,
            stop_price: Some(STOP),
            fx_to_account: USD_PER_EUR,
        }],
    )
    .unwrap();

    let exposure = current_price * sizing.size * spec.multiplier * USD_PER_EUR;
    common::assert_close(
        snapshot.positions[0].notional,
        exposure,
        1e-6,
        "Exposure aus Preis, Stückzahl, Multiplikator und Wechselkurs",
    );

    // 3. Modellbasierte Bewertung derselben Position im gemeinsamen Kontext.
    let today = Date::new(2026, 6, 15).unwrap();
    let context = ValuationContext::new(today, 1_781_000_000, "vertical-slice")
        .with_discount_curve(
            Currency::usd(),
            DiscountCurve::new(YieldCurve::flat(
                today,
                0.04,
                DayCountConvention::Actual365Fixed,
            )),
        )
        .with_fx_rate(FxRate::new("USD", "EUR", USD_PER_EUR));

    let positions = vec![ValuationPosition::new(
        "ESZ6",
        Currency::usd(),
        PositionSide::Long,
        sizing.size,
        ValuedInstrument::Linear {
            price: current_price,
            multiplier: spec.multiplier,
        },
    )];

    let result = context
        .stress_portfolio(
            &positions,
            &account_currency,
            &MarketScenario::neutral().with_underlying_shock(-0.05),
        )
        .unwrap();

    // Der Basiswert der Neubewertung muss das Exposure des Snapshots sein: dieselbe Position,
    // dieselben Zahlen.
    common::assert_close(
        result.value.base.total_value_account,
        exposure,
        1e-6,
        "Neubewertung und Snapshot sehen dieselbe Position",
    );

    // 4. Ergebnisvertrag für den Consumer.
    let report = PortfolioReport::from_scenario(&result);

    assert_eq!(report.account_currency, account_currency);
    assert_eq!(report.stamp.valuation_date, today);
    assert_eq!(report.stamp.data_version, "vertical-slice");
    assert_eq!(report.positions.len(), 1);
    assert_eq!(report.positions[0].model, ValuationModel::Linear);

    common::assert_close(
        report.pnl_account,
        -0.05 * exposure,
        1e-6,
        "ein linearer Fünf-Prozent-Schock trifft proportional",
    );
    common::assert_close(
        report.scenario_value_account - report.base_value_account,
        report.pnl_account,
        0.0,
        "das ausgewiesene Ergebnis ist die Differenz der beiden Werte",
    );

    // Die Sensitivitäten tragen ihre Bewegung mit, damit ein Report sie nicht selbst benennen muss.
    let underlying = report
        .sensitivities
        .iter()
        .find(|(kind, _)| *kind == SensitivityKind::UnderlyingUpOnePercent)
        .expect("Underlying-Sensitivität fehlt");
    common::assert_close(underlying.1, 0.01 * exposure, 1e-6, "ein Prozent Exposure");
    assert_eq!(
        SensitivityKind::UnderlyingUpOnePercent.described_move(),
        "every underlying +1%"
    );

    // Eine Fremdwährungsposition reagiert auf den Wechselkurs; das steht ebenfalls im Vertrag.
    let fx = report
        .sensitivities
        .iter()
        .find(|(kind, _)| *kind == SensitivityKind::FxUpOnePercent)
        .expect("FX-Sensitivität fehlt");
    common::assert_close(fx.1, 0.01 * exposure, 1e-6, "ein Prozent Wechselkurs");
}

/// Der Stempel wandert unverändert bis in den Report — ohne ihn ließe sich eine Zahl in einem
/// alten Bericht ihrem Eingabestand nicht mehr zuordnen.
#[test]
fn test_report_carries_the_stamp_of_the_context_it_came_from() {
    let today = Date::new(2026, 6, 15).unwrap();
    let context = ValuationContext::new(today, 42, "snapshot-xyz").with_discount_curve(
        Currency::eur(),
        DiscountCurve::new(YieldCurve::flat(
            today,
            0.02,
            DayCountConvention::Actual365Fixed,
        )),
    );
    let positions = vec![ValuationPosition::new(
        "EQ",
        Currency::eur(),
        PositionSide::Long,
        10.0,
        ValuedInstrument::Linear {
            price: 100.0,
            multiplier: 1.0,
        },
    )];

    let result = context
        .stress_portfolio(&positions, &Currency::eur(), &MarketScenario::neutral())
        .unwrap();
    let report = PortfolioReport::from_scenario(&result);

    assert_eq!(report.stamp, result.stamp);
    assert_eq!(report.stamp.as_of, 42);
    assert_eq!(report.stamp.data_version, "snapshot-xyz");
}
