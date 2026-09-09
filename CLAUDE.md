# Project instructions

## Scope

- Keep `kestrel-chartkit` a universal, vendor-neutral library.
- Do not add assumptions or dependencies tied to a specific broker, exchange, market-data provider, or trading platform.
- Implement provider-specific integrations in consuming applications or separate adapter crates.

## Testing

- Jede neue oder in ihrer Berechnungslogik geänderte Indikator-/Scoring-Funktion braucht vor
  Abschluss der Änderung einen Golden-Reference-Test (Formel-Indikatoren) bzw. eine
  Szenario-Fixture (Struktur-/Musterdetektoren ohne Einzelzahl-Output, z. B. Wyckoff, BOS/CHoCH,
  Liquidity Sweeps/Pools/FVG, Order Blocks, ZigZag, Pivot-Strukturen). Invariant-Tests
  (Bounds/NaN/Stabilität) allein sind kein ausreichendes Abnahmekriterium.
- Referenzwerte müssen unabhängig vom zu testenden Code hergeleitet sein: Handrechnung oder eine
  separate Zweitimplementierung (z. B. Python) aus der dokumentierten Formel, eine etablierte
  externe Referenzformel/-datenreihe, oder — bei Composite-Indikatoren ohne externen Standard —
  die dokumentierte Kombinationsformel angewandt auf bereits bestätigte Sub-Indikator-
  Golden-Werte, ergänzt um mindestens ein analytisch eindeutiges Extremszenario.
- Composite-Indikatoren dürfen ihre Sub-Werte ausschließlich aus bereits bestehenden
  Golden-Fixtures referenzieren, niemals aus einem Testlauf des gerade geprüften
  Composite-Tests selbst (Zirkularitätsverbot).
- Golden-Reference-Fixtures werden nach Indikator-Gruppe aufgeteilt gepflegt
  (`tests/golden_reference_<gruppe>.rs` + `tests/fixtures/golden_<gruppe>.txt`), nicht als eine
  wachsende Sammeldatei.
- Diese Regel ist bis auf Weiteres eigenständig in `CLAUDE.md` maßgeblich; sobald
  `plan/review-prozess.md` Phase 3 angepasst wird, übernimmt sie diesen Wortlaut, statt ihn neu
  zu formulieren.
