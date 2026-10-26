use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use axum::body::Bytes;
use log::*;
use tokio::{net::UdpSocket, sync::broadcast};
use tokio_util::sync::CancellationToken;

/// Largest UDP payload the engine sends: 7 transport stream packets.
const DATAGRAM_SIZE: usize = 1316;

/// About two seconds of a 5 Mb/s stream; a viewer that falls further behind
/// skips ahead instead of slowing the others down.
const BACKLOG: usize = 1024;

/// The low-latency preview of a channel: the engine sends a live MPEG-TS copy
/// of its encoded output to a local UDP port, and every Player that watches
/// receives the same bytes over HTTP.
#[derive(Debug)]
pub struct PreviewFeed {
    channel_id: i32,
    url: String,
    sender: broadcast::Sender<Bytes>,
    /// The light preview: re-encoded small, only while someone watches it.
    light: broadcast::Sender<Bytes>,
    light_running: Arc<AtomicBool>,
    /// Stops the forwarding task when the channel goes away.
    stop: CancellationToken,
}

impl PreviewFeed {
    /// Bind a local port for the channel and forward what arrives on it until
    /// `shutdown` is cancelled or the feed is dropped.
    pub async fn bind(channel_id: i32, shutdown: CancellationToken) -> io::Result<Self> {
        let socket = UdpSocket::bind("127.0.0.1:0").await?;
        let port = socket.local_addr()?.port();
        let (sender, _) = broadcast::channel(BACKLOG);
        let forward = sender.clone();
        let stop = shutdown.child_token();
        let stopped = stop.clone();

        tokio::spawn(async move {
            let mut datagram = vec![0_u8; 64 * 1024];

            loop {
                tokio::select! {
                    () = stopped.cancelled() => break,
                    received = socket.recv(&mut datagram) => match received {
                        Ok(size) => {
                            // Nobody watches: drop the data.
                            if forward.receiver_count() > 0 {
                                let _ = forward.send(Bytes::copy_from_slice(&datagram[..size]));
                            }
                        }
                        Err(error) => {
                            warn!(channel = channel_id; "Player preview stopped: {error}");
                            break;
                        }
                    },
                }
            }
        });

        Ok(Self {
            channel_id,
            url: format!("udp://127.0.0.1:{port}?pkt_size={DATAGRAM_SIZE}"),
            sender,
            light: broadcast::channel(BACKLOG).0,
            light_running: Arc::new(AtomicBool::new(false)),
            stop,
        })
    }

    /// Where the engine sends the preview.
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Bytes> {
        self.sender.subscribe()
    }

    /// The light preview, started for the first viewer and stopped after
    /// the last one leaves.
    pub fn subscribe_light(&self) -> broadcast::Receiver<Bytes> {
        let receiver = self.light.subscribe();

        if !self.light_running.swap(true, Ordering::AcqRel) {
            self.start_light();
        }

        receiver
    }

    fn start_light(&self) {
        let channel_id = self.channel_id;
        let source = self.sender.clone();
        let light = self.light.clone();
        let running = self.light_running.clone();
        let stop = self.stop.clone();
        let runtime = tokio::runtime::Handle::current();

        let started = std::thread::Builder::new()
            .name(format!("noar-light-preview-{channel_id}"))
            .spawn(move || {
                info!(channel = channel_id; "Light Player preview started");

                while light.receiver_count() > 0 && !stop.is_cancelled() {
                    let (input, output) = mpsc::channel::<Vec<u8>>();
                    let ended = Arc::new(AtomicBool::new(false));
                    let forward_ended = ended.clone();
                    let mut source = source.subscribe();
                    let watchers = light.clone();

                    // Hand the full preview to the encoder thread.
                    runtime.spawn(async move {
                        let mut check = tokio::time::interval(Duration::from_millis(500));

                        loop {
                            tokio::select! {
                                received = source.recv() => match received {
                                    Ok(bytes) => {
                                        if input.send(bytes.to_vec()).is_err() {
                                            break;
                                        }
                                    }
                                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                                    Err(broadcast::error::RecvError::Closed) => break,
                                },
                                _ = check.tick() => {
                                    if forward_ended.load(Ordering::Acquire)
                                        || watchers.receiver_count() == 0
                                    {
                                        break;
                                    }
                                }
                            }
                        }
                    });

                    let deliver_to = light.clone();
                    let result = noar_engine::transcode_light_preview(
                        output,
                        move |bytes| {
                            let _ = deliver_to.send(Bytes::from(bytes));
                            deliver_to.receiver_count() > 0
                        },
                        noar_engine::LightPreview::default(),
                        ended.clone(),
                    );
                    ended.store(true, Ordering::Release);

                    if let Err(error) = result {
                        debug!(channel = channel_id; "Light Player preview: {error:#}");
                    }

                    // The playout restarted or paused: wait before reading again.
                    std::thread::sleep(Duration::from_secs(1));
                }

                running.store(false, Ordering::Release);
                info!(channel = channel_id; "Light Player preview stopped");
            });

        if let Err(error) = started {
            self.light_running.store(false, Ordering::Release);
            warn!(channel = channel_id; "Light Player preview could not start: {error}");
        }
    }
}

impl Drop for PreviewFeed {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn forwards_datagrams_to_every_viewer() {
        let shutdown = CancellationToken::new();
        let feed = PreviewFeed::bind(1, shutdown.clone()).await.unwrap();
        let port = feed
            .url()
            .trim_start_matches("udp://127.0.0.1:")
            .split('?')
            .next()
            .unwrap()
            .parse::<u16>()
            .unwrap();
        let mut first = feed.subscribe();
        let mut second = feed.subscribe();
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        sender
            .send_to(&[0x47; 188], ("127.0.0.1", port))
            .await
            .unwrap();

        for viewer in [&mut first, &mut second] {
            let bytes = tokio::time::timeout(Duration::from_secs(2), viewer.recv())
                .await
                .unwrap()
                .unwrap();

            assert_eq!(bytes.len(), 188);
            assert_eq!(bytes[0], 0x47);
        }

        shutdown.cancel();
    }

    #[tokio::test]
    async fn the_engine_address_is_a_local_udp_port() {
        let shutdown = CancellationToken::new();
        let feed = PreviewFeed::bind(1, shutdown.clone()).await.unwrap();

        assert!(feed.url().starts_with("udp://127.0.0.1:"));
        assert!(feed.url().ends_with("?pkt_size=1316"));

        shutdown.cancel();
    }
}
