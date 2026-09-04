//! Provider-neutral integration contracts: a data-feed trait and a notification-sink trait, plus
//! dependency-free reference implementations (an in-memory feed, a logging sink, and a
//! transport-agnostic webhook-shaped sink). Tools embedding kestrel-chartkit implement these
//! traits with their own broker/exchange/webhook specifics; this crate never depends on a
//! specific provider, and ships no HTTP client — [`WebhookNotificationSink`] takes the actual
//! transport as an injected closure instead.

use std::collections::HashMap;

use crate::model::Bar;
use crate::timeframe::Timeframe;

/// Historical/live bar data source contract. Provider clients (broker APIs, exchange
/// WebSockets, vendor SDKs) live in consuming applications, not here.
pub trait DataFeedAdapter {
    type Error;

    /// Fetches historical bars for `symbol` at `timeframe` within `[from, to]` (Unix seconds).
    fn fetch_historical(
        &mut self,
        symbol: &str,
        timeframe: Timeframe,
        from: i64,
        to: i64,
    ) -> Result<Vec<Bar>, Self::Error>;

    /// Registers interest in live updates for `symbol`/`timeframe`. A no-op for adapters that
    /// only ever serve historical data.
    fn subscribe_live(&mut self, symbol: &str, timeframe: Timeframe) -> Result<(), Self::Error>;

    /// Returns any new bars received since the last call, for all subscribed symbols.
    fn poll_live(&mut self) -> Result<Vec<Bar>, Self::Error>;
}

/// A fully generic, dependency-free [`DataFeedAdapter`]: serves bars from an in-memory table.
/// Useful for backtests, replay, and tests — not tied to any provider.
#[derive(Debug, Clone, Default)]
pub struct InMemoryDataFeed {
    bars_by_symbol: HashMap<String, Vec<Bar>>,
    live_cursor: HashMap<String, usize>,
    subscribed: Vec<String>,
}

impl InMemoryDataFeed {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads (replacing any existing) bars for `symbol`. Bars are assumed sorted ascending by
    /// timestamp.
    pub fn load(&mut self, symbol: impl Into<String>, bars: Vec<Bar>) {
        self.bars_by_symbol.insert(symbol.into(), bars);
    }
}

impl DataFeedAdapter for InMemoryDataFeed {
    type Error = String;

    fn fetch_historical(
        &mut self,
        symbol: &str,
        _timeframe: Timeframe,
        from: i64,
        to: i64,
    ) -> Result<Vec<Bar>, Self::Error> {
        let bars = self
            .bars_by_symbol
            .get(symbol)
            .ok_or_else(|| format!("no bars loaded for symbol '{symbol}'"))?;
        Ok(bars
            .iter()
            .filter(|b| b.timestamp >= from && b.timestamp <= to)
            .cloned()
            .collect())
    }

    fn subscribe_live(&mut self, symbol: &str, _timeframe: Timeframe) -> Result<(), Self::Error> {
        if !self.subscribed.contains(&symbol.to_string()) {
            self.subscribed.push(symbol.to_string());
            self.live_cursor.insert(
                symbol.to_string(),
                self.bars_by_symbol
                    .get(symbol)
                    .map(|b| b.len())
                    .unwrap_or(0),
            );
        }
        Ok(())
    }

    /// "Live" bars for an in-memory feed are simply whatever was `load`ed beyond the cursor
    /// established at subscription time — a deterministic stand-in for a real push feed, useful
    /// for testing consumers of [`DataFeedAdapter`] without a network dependency.
    fn poll_live(&mut self) -> Result<Vec<Bar>, Self::Error> {
        let mut new_bars = Vec::new();
        for symbol in &self.subscribed {
            let cursor = self.live_cursor.get(symbol).copied().unwrap_or(0);
            if let Some(bars) = self.bars_by_symbol.get(symbol) {
                if cursor < bars.len() {
                    new_bars.extend(bars[cursor..].iter().cloned());
                    self.live_cursor.insert(symbol.clone(), bars.len());
                }
            }
        }
        Ok(new_bars)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NotificationEvent {
    pub timestamp: i64,
    pub severity: NotificationSeverity,
    pub title: String,
    pub body: String,
}

/// Notification-channel contract, decoupled from any specific transport (webhook, email, push,
/// broker action). Deterministic chart-kit events (alerts, scenario transitions, fills) are the
/// producer; the transport is entirely the consumer's concern.
pub trait NotificationSink {
    type Error;

    fn notify(&mut self, event: &NotificationEvent) -> Result<(), Self::Error>;
}

/// A fully generic, dependency-free [`NotificationSink`]: appends every event to an in-memory
/// log. Useful for tests, or as a base to wrap with a real transport.
#[derive(Debug, Clone, Default)]
pub struct LoggingNotificationSink {
    pub events: Vec<NotificationEvent>,
}

impl LoggingNotificationSink {
    pub fn new() -> Self {
        Self::default()
    }
}

impl NotificationSink for LoggingNotificationSink {
    type Error = std::convert::Infallible;

    fn notify(&mut self, event: &NotificationEvent) -> Result<(), Self::Error> {
        self.events.push(event.clone());
        Ok(())
    }
}

/// A transport-agnostic webhook-shaped [`NotificationSink`]: holds a destination `url` and an
/// injected `send` closure, so this crate needs no HTTP client dependency — works with any
/// webhook-style endpoint (Slack, Discord, a generic HTTP receiver); the consumer supplies the
/// actual transport.
pub struct WebhookNotificationSink<F>
where
    F: FnMut(&str, &NotificationEvent) -> Result<(), String>,
{
    pub url: String,
    send: F,
}

impl<F> WebhookNotificationSink<F>
where
    F: FnMut(&str, &NotificationEvent) -> Result<(), String>,
{
    pub fn new(url: impl Into<String>, send: F) -> Self {
        Self {
            url: url.into(),
            send,
        }
    }
}

impl<F> NotificationSink for WebhookNotificationSink<F>
where
    F: FnMut(&str, &NotificationEvent) -> Result<(), String>,
{
    type Error = String;

    fn notify(&mut self, event: &NotificationEvent) -> Result<(), Self::Error> {
        (self.send)(&self.url, event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(ts: i64) -> Bar {
        Bar::new(ts, 100.0, 101.0, 99.0, 100.0, 10.0)
    }

    #[test]
    fn test_in_memory_feed_fetch_historical_filters_range() {
        let mut feed = InMemoryDataFeed::new();
        feed.load("TEST", vec![bar(0), bar(60), bar(120), bar(180)]);

        let result = feed
            .fetch_historical("TEST", Timeframe::Minute(1), 60, 120)
            .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].timestamp, 60);
    }

    #[test]
    fn test_in_memory_feed_fetch_unknown_symbol_errors() {
        let mut feed = InMemoryDataFeed::new();
        assert!(feed
            .fetch_historical("NOPE", Timeframe::Minute(1), 0, 100)
            .is_err());
    }

    #[test]
    fn test_in_memory_feed_poll_live_only_returns_new_bars() {
        let mut feed = InMemoryDataFeed::new();
        feed.load("TEST", vec![bar(0), bar(60)]);
        feed.subscribe_live("TEST", Timeframe::Minute(1)).unwrap();

        let first_poll = feed.poll_live().unwrap();
        assert!(
            first_poll.is_empty(),
            "no bars arrived after the subscription cursor yet"
        );

        feed.load("TEST", vec![bar(0), bar(60), bar(120)]);
        let second_poll = feed.poll_live().unwrap();
        assert_eq!(second_poll.len(), 1);
        assert_eq!(second_poll[0].timestamp, 120);
    }

    #[test]
    fn test_logging_sink_records_events() {
        let mut sink = LoggingNotificationSink::new();
        let event = NotificationEvent {
            timestamp: 0,
            severity: NotificationSeverity::Warning,
            title: "test".to_string(),
            body: "body".to_string(),
        };
        sink.notify(&event).unwrap();
        assert_eq!(sink.events.len(), 1);
        assert_eq!(sink.events[0].severity, NotificationSeverity::Warning);
    }

    #[test]
    fn test_webhook_sink_delegates_to_injected_transport() {
        let mut received: Vec<(String, String)> = Vec::new();
        let mut sink = WebhookNotificationSink::new("https://example.test/hook", |url, event| {
            received.push((url.to_string(), event.title.clone()));
            Ok(())
        });

        let event = NotificationEvent {
            timestamp: 0,
            severity: NotificationSeverity::Critical,
            title: "alert".to_string(),
            body: "body".to_string(),
        };
        sink.notify(&event).unwrap();

        assert_eq!(received.len(), 1);
        assert_eq!(received[0].0, "https://example.test/hook");
        assert_eq!(received[0].1, "alert");
    }

    #[test]
    fn test_webhook_sink_propagates_transport_errors() {
        let mut sink = WebhookNotificationSink::new("https://example.test/hook", |_url, _event| {
            Err("network unreachable".to_string())
        });
        let event = NotificationEvent {
            timestamp: 0,
            severity: NotificationSeverity::Info,
            title: "x".to_string(),
            body: String::new(),
        };
        assert!(sink.notify(&event).is_err());
    }
}
