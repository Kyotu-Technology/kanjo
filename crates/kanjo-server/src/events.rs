use crate::state::AppState;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::Stream;
use kanjo_core::api::LiveEvent;
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, watch};

const KEEP_ALIVE: Duration = Duration::from_secs(15);

fn event(ev: &LiveEvent) -> Event {
    match serde_json::to_string(ev) {
        Ok(json) => Event::default().data(json),
        Err(e) => {
            tracing::warn!(error = %e, "live event could not be serialized");
            Event::default().comment("serialization error")
        }
    }
}

struct Feed {
    rx: broadcast::Receiver<LiveEvent>,
    stop: watch::Receiver<bool>,
    first: Option<LiveEvent>,
}

/// `GET /api/v1/events`: one unnamed `data:` line per `LiveEvent`, starting
/// with the current import status. The stream ends when the server shuts down.
pub(crate) async fn events(State(state): State<AppState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let inner = &state.inner;
    let feed = Feed {
        rx: inner.events.subscribe(),
        stop: inner.shutdown.subscribe(),
        first: inner.ingestor.as_ref().map(|_| LiveEvent::Ingest(inner.ingest_status())),
    };
    let stream = futures_util::stream::unfold(feed, |mut feed| async move {
        if let Some(ev) = feed.first.take() {
            return Some((Ok(event(&ev)), feed));
        }
        loop {
            if *feed.stop.borrow() {
                return None;
            }
            tokio::select! {
                r = feed.rx.recv() => match r {
                    Ok(ev) => return Some((Ok(event(&ev)), feed)),
                    Err(RecvError::Lagged(n)) => tracing::debug!(skipped = n, "slow event stream skipped events"),
                    Err(RecvError::Closed) => return None,
                },
                changed = feed.stop.changed() => if changed.is_err() { return None },
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
}
