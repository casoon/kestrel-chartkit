# Repository-Befund & Sanierungsplan (`cargo judge`)

Stand: 2026-09-04  
Analysiertes Crate: `kestrel-chartkit v0.1.1`  
Werkzeug: `cargo-judge 0.1.0` (Deterministic post-refactoring analysis)

---

## 1. Executive Summary

`cargo judge` hat das gesamte Repository statisch analysiert. Die Analyse endete mit **Exit-Code 0** (keine fatalen Crate- oder Boundary-Verstöße).

| Kennzahl | Wert | Bewertung |
|---|---|---|
| **Evidence-backed Findings** | **511** | Konkrete Code-Muster mit Handlungsbedarf oder technischer Schuld |
| **Advisory Heuristics** | **153** | Komplexitäts- und Lesbarkeitswarnungen (kein Einfluss auf Verdict) |
| **Boundary Rules** | 0 Verletzungen | Saubere Modulgrenzen, keine zirkulären Abhängigkeiten |
| **Compiler & Clippy** | **0 Warnings** | `cargo clippy --all-targets --all-features` ist vollständig sauber |
| **Testsuite** | **100% grün** | Alle Unit-, Invarianten- und Golden-Reference-Tests bestehen |

### Verteilung nach Kategorien

```
Evidence-backed (511):
   389x  duplicate-code       (Gleichförmige Indikatoren-Muster)
    88x  panic-in-lib         (unwrap/expect/panics in Bibliotheksmodulen)
    19x  suppression-debt     (#[allow(...)]-Attribute)
    13x  assertion-free-test  (Tests ohne inline assert! oder reine Smoke-Tests)
     2x  swallowed-result     (Ungeprüfte Results in Testfunktionen)

Advisory Heuristics (153):
    76x  complexity-inflation (Funktionen mit hoher zyklomatischer Komplexität)
    40x  integer-cast-risk    (Numerische Casts ohne explizite Schrankenprüfung)
    35x  maintainability-index (Sehr große/dichte Quellcode-Dateien)
     1x  abstraction-inflation (Unnötig breite Trait-Abstraktion)
     1x  context-free-propagation (Beispielprogramm-Propagation)
```

---

## 2. Detaillierter Befund & Arbeitsanweisungen

Die Befunde sind nach Priorität und Auswirkung sortiert: von sofort behebbaren „Quick Wins" bis hin zu strategischen Architektur-Refactorings.

---

### Priorität 1: Direkte Korrektheit & Zuverlässigkeit

#### 1.1 `swallowed-result` (2 Fundstellen)
* **Problem:** Ein Aufruf liefert ein `Result`, dessen Rückgabewert ignoriert oder nicht verifiziert wird. Tritt ein Fehler auf, schlägt der Test nicht fehl.
* **Fundstellen:**
  * `tests/composite_signal.rs:26` (`build_checked(...)`)
  * `tests/composite_signal.rs:27` (`build_checked(...)`)
* **Arbeitsanweisung zur Behebung:**
  Rückgabewert mit `.expect("...")` oder `?` absichern.
  ```rust
  // Vorher:
  let mut rsi = build_checked("rsi", &params);

  // Nachher:
  let mut rsi = build_checked("rsi", &params).expect("RSI indicator build failed");
  ```

---

#### 1.2 `assertion-free-test` (13 Fundstellen)
* **Problem:** Ein Test enthält kein direktes `assert!`, `assert_eq!` o. ä. 
  * In `tests/golden_reference_*.rs` liegt dies daran, dass die Assertion in einer ausgelagerten Helper-Funktion (`assert_parity` / `assert_close`) gekapselt ist. Der statische Analyzer sieht im Funktionsrumpf selbst kein Makro.
  * In `smart_money_structure.rs` und `wyckoff.rs` sind es reine Smoke-Tests, die nur auf Panic-Freiheit prüfen.
* **Fundstellen:**
  * `src/indicator/smart_money_structure.rs:556` (`test_smoke_no_panic_across_trending_bars`)
  * `src/indicator/wyckoff.rs:481` (`test_smoke_no_panic_across_random_walk`)
  * `tests/golden_reference_moving_averages.rs:30` (`test_golden_sma_reference_values`)
  * `tests/golden_reference_moving_averages.rs:40` (`test_golden_ema_reference_values`)
  * `tests/golden_reference_moving_averages.rs:50` (`test_golden_wma_reference_values`)
  * `tests/golden_reference_moving_averages.rs:60` (`test_golden_vwma_reference_values`)
  * `tests/golden_reference_moving_averages.rs:70` (`test_golden_hma_reference_values`)
  * `tests/golden_reference_moving_averages.rs:80` (`test_golden_dema_reference_values`)
  * `tests/golden_reference_moving_averages.rs:90` (`test_golden_kama_reference_values`)
  * `tests/golden_reference_moving_averages.rs:100` (`test_golden_tema_reference_values`)
  * `tests/golden_reference_moving_averages.rs:110` (`test_golden_lsma_reference_values`)
  * `tests/golden_reference_moving_averages.rs:120` (`test_golden_mcginley_reference_values`)
  * `tests/golden_reference_volatility.rs:14` (`test_golden_atr_reference_values`)
* **Arbeitsanweisung zur Behebung:**
  1. **Für Smoke-Tests:** Den gemeinsamen Helper `assert_no_panic` aus `tests/common/mod.rs` verwenden oder den finalen Status explizit prüfen:
     ```rust
     // In wyckoff.rs / smart_money_structure.rs:
     assert!(machine.bars_in_range > 0 || machine.phase() != WyckoffPhase::Undefined);
     ```
  2. **Für Golden-Reference-Tests:** Helper so umbauen, dass sie ein `bool` oder `ParityReport` zurückgeben, auf das der Testfunktion ein explizites `assert!(report.is_ok())` ausführt, anstatt im Helper selbst zu panicken.

---

### Priorität 2: Robustheit gegen Crashes & Überläufe

#### 2.1 `panic-in-lib` (88 Fundstellen)
* **Problem:** In Produktionscode wird `.unwrap()`, `.expect()` oder `panic!` verwendet. Unerwartete Datenfeeder-Eingaben (z. B. leere Slices, ungültige Zeitstempel) können den Host-Prozess zum Absturz bringen, statt einen geordneten `Err`- oder `None`-Zustand zu melden.
* **Hauptschwerpunkte:**
  * `src/indicator/elliott.rs` (18x) — Pivots-Indexing und Wave-Validation
  * `src/clustering.rs` (12x) — KMeans-Clusterings und Leermengen
  * `src/indicator/swing_structure.rs` (12x) — Swing-Pivots und Queue-Lookups
  * `src/structure/mod.rs` (6x) — Zone-Registry-Lookups
  * `src/indicator/chart_patterns.rs` (5x) — Geometrie-Pivots
  * `src/stats.rs` (4x) — Quantil-Berechnungen auf leeren Fenstern
  * `src/graph.rs` (3x) — Graph-Topologie
  * `src/calendar.rs` (1x) — Timezone-Offset-Parsing
* **Arbeitsanweisung zur Behebung:**
  1. **Slice- und Vektor-Zugriffe:** Direkte Indexierung `vec[0]` oder `.unwrap()` durch `.first()`, `.get(idx)` oder `.ok_or(...)` ersetzen.
  2. **Mathematische Transformationen:** Bei leeren Slices in `stats.rs` und `clustering.rs` ein explizites `None` oder `Result::Err` zurückgeben (z. B. `ClusterError::EmptyData`).
  3. **Vermeidung von `unwrap()` in Engines:** Wo ein Wert logisch vorhanden sein muss, mit `unwrap_or_default()` oder einer defensiven Fallback-Berechnung arbeiten.

---

#### 2.2 `integer-cast-risk` (40 Fundstellen)
* **Problem:** Typkonvertierungen via `as usize`, `as f64`, `as i64` oder `as i32` ohne Vorzeichen- und Bereichsprüfung. Wenn z. B. ein negativer Fließkommawert zu `usize` gecastet wird, kommt es zu einem Wrapping oder Clamp-Fehler.
* **Ausgewählte Fundstellen:**
  * `src/engine/acceptance_detector.rs:64`
  * `src/indicator/moving_averages.rs:251, 268` (HmaEngine)
  * `src/indicator/volume_profile.rs:81, 82, 164, 195, 196`
  * `src/indicator/volume_profile_extended.rs:111, 112`
  * `src/stats.rs:49, 50` (`rolling_quantile`)
  * `src/timeframe.rs:108, 109, 133`
* **Arbeitsanweisung zur Behebung:**
  1. **Fließkomma zu Integer:** Vor dem Cast Schranken absichern:
     ```rust
     // Vorher:
     let bin_idx = ((price - min) / step) as usize;

     // Nachher:
     let raw_idx = ((price - min) / step).floor();
     let bin_idx = if raw_idx.is_finite() && raw_idx >= 0.0 {
         raw_idx as usize
     } else {
         0
     };
     ```
  2. **Index-Differenzen:** `usize::abs_diff` nutzen statt Umweg über `(i as isize - len as isize).unsigned_abs()`.
  3. **Fallbacks bei Division/Quantilen:** Bei `stats.rs` Quantil-Rängen `(q * (len - 1) as f64).round() as usize` sicherstellen, dass `len > 0`.

---

### Priorität 3: Wartbarkeit & Redundanz

#### 3.1 `suppression-debt` (19 Fundstellen)
* **Problem:** Code-Warnungen wurden über `#[allow(...)]` unterdrückt, statt die zugrunde liegende Ursache zu beheben.
* **Fundstellen:**
  * `#[allow(clippy::too_many_arguments)]` (11x):
    * `src/engine/market_context.rs:77`
    * `src/engine/vwap_regime.rs:69`
    * `src/evaluation/recorder.rs:92`
    * `src/indicator/cci.rs:45`
    * `src/indicator/fisher_transform.rs:50`
    * `src/indicator/mfi.rs:41`
    * `src/indicator/rsi.rs:49`
    * `src/indicator/stoch_rsi.rs:55`
    * `src/indicator/tsi.rs:53`
    * `src/indicator/williams_r.rs:45`
    * `src/indicator/zigzag_advanced.rs:272`
    * `src/scoring/composite.rs:281`
  * `#[allow(dead_code)]` (7x):
    * `src/indicator/stoch_rsi.rs:14, 16`
    * `src/indicator/tsi.rs:14, 16`
    * `src/indicator/vix_fix.rs:10`
    * `tests/common/mod.rs:1`
  * `#[allow(clippy::needless_range_loop)]` (1x):
    * `src/indicator/volume_profile.rs:86`
* **Arbeitsanweisung zur Behebung:**
  1. **Parameter-Objekte:** Bei Funktionen mit > 7 Parametern ein dediziertes `Config`-Struct einführen (z. B. `VwapRegimeConfig`, `MarketContextConfig`). Dies entfernt `too_many_arguments` und macht die API erweiterbar ohne Breaking Changes.
  2. **Toter Code:** Unbenutzte Konstanten oder private Hilfsmethoden in `stoch_rsi`, `tsi`, `vix_fix` entfernen oder mit Tests versehen.
  3. **Iteratoren statt Index-Schleifen:** In `volume_profile.rs:86` auf `.iter_mut().enumerate()` umstellen.

---

#### 3.2 `duplicate-code` (389 Fundstellen in ~160 Klon-Familien)
* **Problem:** Nahezu identische Code-Blöcke über Dutzende Indikatoren hinweg.
  * *Familie 1–7:* Identische `alerts()`-Generierung für Oszillator-Grenzwerte (z. B. `Rsi`, `Mfi`, `Tsi`, `WilliamsR`, `FisherTransform` erzeugen auf exakt dieselbe Weise Schwellenwert-Alerts).
  * *Familie 8–10:* Identischer Warmup-Check und Ringbuffer-Push/Pop am Anfang von `on_bar`.
  * *Familie 13–15:* Identische Konstruktoren `new(period)` für gleitende Durchschnitte.
* **Bewertung (Wann beheben, wann belassen?):**
  * **Belassen:** Die Unabhängigkeit einzelner Indikator-Dateien hat im Trading-Bereich einen hohen Wert. Wenn jeder Indikator eine autarke Einheit bildet, verhindert das ungewollte Seiteneffekte bei Anpassungen einzelner Berechnungen.
  * **Sinnvoll beheben:** Ein internes Macro oder Trait für Standard-Schwellwert-Alerts (z. B. `ThresholdAlertConfig`) kann redundante 30-Zeilen-Blöcke in Oszillatoren eliminieren:
    ```rust
    // Vorschlag: Interner Helper für Oszillator-Alerts
    pub(crate) fn check_threshold_alerts(
        alerts: &mut Vec<IndicatorAlert>,
        name: &str,
        current: f64,
        prev: Option<f64>,
        upper: f64,
        lower: f64,
    ) { ... }
    ```

---

#### 3.3 `complexity-inflation` & `maintainability-index` (76 bzw. 35 Dateien)
* **Problem:** Dateien mit > 500 Zeilen und Funktionen mit hoher zyklomatischer Komplexität (`cyclomatic > 15`), vor allem bei Chartmustern (`chart_patterns.rs`), Wyckoff-Zustandsübergängen (`wyckoff.rs`) und Zonenregistern (`zone_registry.rs`).
* **Arbeitsanweisung zur Behebung:**
  * Große `on_bar`-Zustandsmaschinen in kleine Sub-Handler zerlegen:
    * Statt eines 200-Zeilen `match self.phase`:
      `WyckoffPhase::B => self.handle_phase_b(bar, atr),`
      `WyckoffPhase::C => self.handle_phase_c(bar),`
      `WyckoffPhase::D => self.handle_phase_d(bar, atr),`
    * Das senkt die kognitive Komplexität pro Funktion drastisch und isoliert Fehler.

---

## 3. Priorisierter Umsetzungsfahrplan

Zur systematischen Bereinigung empfiehlt sich folgendes Vorgehen in 4 Schritten:

### Schritt 1: Quick Wins (Risikofrei, sofortige Behebung)
- [ ] `tests/composite_signal.rs`: `swallowed-result` beheben (`expect`/`?`).
- [ ] `tests/golden_reference_*.rs`: `assert_parity`-Rückgabe mit `assert!(...)` versehen.
- [ ] `src/indicator/stoch_rsi.rs`, `src/indicator/tsi.rs`, `src/indicator/vix_fix.rs`: Unbenutzten Code (`dead_code`) entfernen.
- [ ] `src/indicator/volume_profile.rs`: Schleife idiomatisch umschreiben (`needless_range_loop`).

### Schritt 2: Numerische Sicherheit (`integer-cast-risk`)
- [ ] `src/stats.rs` & `src/clustering.rs`: Quantil- und Bin-Berechnungen mit `.is_finite()` und `max(0.0)` absichern.
- [ ] `src/indicator/volume_profile*.rs`: Array-Indizes vor Casts bounds-checken.
- [ ] `src/timeframe.rs`: Kalender- und Bucket-Berechnungen auf sichere Casts prüfen.

### Schritt 3: Fehlerbehandlung in Kernmodulen (`panic-in-lib`)
- [ ] `src/indicator/elliott.rs`: Unchecked Vektor-Indizes durch `.get()` und `Option` ersetzen.
- [ ] `src/clustering.rs`: `kmeans_1d` bei leeren Eingaben sauberes `Err(ClusterError)` liefern lassen.
- [ ] `src/indicator/swing_structure.rs`: Queue-Pops defensiv behandeln.

### Schritt 4: Refactoring von Parametern & Komplexität (Mittelfristig)
- [ ] Config-Structs für Funktionen mit `#[allow(clippy::too_many_arguments)]` einführen.
- [ ] Große State-Machines (`wyckoff.rs`, `chart_patterns.rs`) in private Sub-Methoden aufteilen.
- [ ] Gemeinsamen Alert-Helper für Standard-Oszillatoren evaluieren, um ~200 Duplikat-Tokens einzusparen.
