//! End-to-end HLS tests against a mock HTTP server, opened through the same
//! live-stream entry point the audio engine uses.

use super::segment::tests::{
    adts_frame, fmp4_aac_init, fmp4_flac_init, fmp4_media_segment, id3_tag, ts_segment,
};
use crate::media_source::LiveHttpSource;
use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const PLAYLIST_MIME: &str = "application/vnd.apple.mpegurl";

fn media_playlist(first_seq: u64, count: u64, end_list: bool) -> String {
    let mut text = format!(
        "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:{first_seq}\n"
    );
    for seq in first_seq..first_seq + count {
        text.push_str(&format!("#EXTINF:1.0,\nseg/{seq}.ts\n"));
    }
    if end_list {
        text.push_str("#EXT-X-ENDLIST\n");
    }
    text
}

/// The audio a TS segment numbered `seq` carries.
fn segment_audio(seq: u64) -> Vec<u8> {
    [adts_frame(seq as u8), adts_frame(seq as u8 ^ 0x55)].concat()
}

async fn mount_ts_segments(server: &MockServer, seqs: std::ops::Range<u64>) {
    for seq in seqs {
        Mock::given(method("GET"))
            .and(path(format!("/hls/seg/{seq}.ts")))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "video/mp2t")
                    .set_body_bytes(ts_segment(0x0f, &[segment_audio(seq)])),
            )
            .mount(server)
            .await;
    }
}

/// Serves `playlists[i]` on the i-th request, repeating the last one after.
struct SlidingPlaylist {
    playlists: Vec<String>,
    calls: Arc<AtomicUsize>,
}

impl Respond for SlidingPlaylist {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let i = self.calls.fetch_add(1, Ordering::SeqCst);
        let body = &self.playlists[i.min(self.playlists.len() - 1)];
        ResponseTemplate::new(200)
            .insert_header("content-type", PLAYLIST_MIME)
            .set_body_string(body.clone())
    }
}

#[tokio::test]
async fn master_playlist_without_m3u8_extension_plays_live_ts_and_follows_the_window() {
    let server = MockServer::start().await;
    // The station URL has no `.m3u8`: detected by its Content-Type.
    Mock::given(method("GET"))
        .and(path("/station"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", PLAYLIST_MIME)
                .set_body_string(
                    "#EXTM3U\n\
                     #EXT-X-STREAM-INF:BANDWIDTH=2000000,CODECS=\"avc1.42c00d,mp4a.40.2\"\nhls/video.m3u8\n\
                     #EXT-X-STREAM-INF:BANDWIDTH=128000,CODECS=\"mp4a.40.2\"\nhls/audio.m3u8\n",
                ),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/hls/audio.m3u8"))
        .respond_with(SlidingPlaylist {
            playlists: vec![
                media_playlist(10, 4, false), // 10..=13: play starts at 11
                media_playlist(12, 4, false), // 12..=15: 14 and 15 are new
            ],
            calls: Arc::new(AtomicUsize::new(0)),
        })
        .mount(&server)
        .await;
    mount_ts_segments(&server, 10..16).await;

    let url = format!("{}/station", server.uri());
    tokio::task::spawn_blocking(move || {
        let opened = LiveHttpSource::open_with_backoffs(&url, Some("Test FM"), Vec::new())
            .expect("open HLS station");
        assert_eq!(opened.mime_type.as_deref(), Some("audio/aac"));
        assert!(!opened.reader.is_seekable());

        let expected: Vec<u8> = (11..16).flat_map(segment_audio).collect();
        let mut reader = opened.reader;
        let mut got = vec![0u8; expected.len()];
        reader.read_exact(&mut got).expect("read five segments");
        assert_eq!(got, expected, "segments 11..=15 in order, video dropped");
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn ended_playlist_plays_from_the_start_then_reports_end_of_stream() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hls/show.m3u8"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/plain")
                .set_body_string(media_playlist(0, 2, true)),
        )
        .mount(&server)
        .await;
    mount_ts_segments(&server, 0..2).await;

    let url = format!("{}/hls/show.m3u8", server.uri());
    tokio::task::spawn_blocking(move || {
        let opened = LiveHttpSource::open_with_backoffs(&url, None, Vec::new()).unwrap();
        let mut all = Vec::new();
        opened.reader.take(1 << 20).read_to_end(&mut all).unwrap();
        let expected: Vec<u8> = (0..2).flat_map(segment_audio).collect();
        assert_eq!(all, expected);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn packed_audio_segments_publish_their_id3_titles() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/live.m3u8"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:1\n\
                 #EXTINF:1,\na.aac\n#EXTINF:1,\nb.aac\n#EXT-X-ENDLIST\n",
        ))
        .mount(&server)
        .await;
    for (name, artist, title, seed) in [
        ("a", "Nightwish", "Ghost Love Score", 1u8),
        ("b", "Epica", "Consign to Oblivion", 2),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/{name}.aac")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes([id3_tag(artist, title), adts_frame(seed)].concat()),
            )
            .mount(&server)
            .await;
    }

    let url = format!("{}/live.m3u8", server.uri());
    tokio::task::spawn_blocking(move || {
        let opened = LiveHttpSource::open_with_backoffs(&url, None, Vec::new()).unwrap();
        assert_eq!(opened.mime_type.as_deref(), Some("audio/aac"));
        let mut all = Vec::new();
        opened.reader.take(1 << 20).read_to_end(&mut all).unwrap();
        assert_eq!(all, [adts_frame(1), adts_frame(2)].concat());
        let titles: Vec<String> = opened.metadata_rx.unwrap().try_iter().collect();
        assert_eq!(
            titles,
            vec![
                "Nightwish - Ghost Love Score".to_string(),
                "Epica - Consign to Oblivion".to_string()
            ]
        );
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn encrypted_streams_fail_with_a_clear_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/enc.m3u8"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-KEY:METHOD=AES-128,URI=\"key\"\n#EXTINF:6,\nseg.ts\n",
        ))
        .mount(&server)
        .await;

    let base = server.uri();
    tokio::task::spawn_blocking(move || {
        let err = LiveHttpSource::open_with_backoffs(
            &format!("{base}/enc.m3u8"),
            Some("Enc FM"),
            Vec::new(),
        )
        .err()
        .expect("encrypted stream rejected");
        assert!(err.contains("encrypted"), "{err}");
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn fragmented_mp4_aac_hls_stream_plays_as_adts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hls/program.m3u8"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", PLAYLIST_MIME)
                .set_body_string(
                    "#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-TARGETDURATION:10\n\
                     #EXT-X-MEDIA-SEQUENCE:1\n#EXT-X-MAP:URI=\"program_init.mp4\"\n\
                     #EXTINF:9.984,\nseg1.m4s\n#EXT-X-ENDLIST\n",
                ),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/hls/program_init.mp4"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fmp4_aac_init(1, [0x12, 0x10])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/hls/seg1.m4s"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/octet-stream")
                .set_body_bytes(fmp4_media_segment(1, &[b"abcd", b"ef"])),
        )
        .mount(&server)
        .await;

    let url = format!("{}/hls/program.m3u8", server.uri());
    tokio::task::spawn_blocking(move || {
        let opened = LiveHttpSource::open_with_backoffs(&url, Some("SomaFM"), Vec::new())
            .expect("open fMP4 HLS station");
        assert_eq!(opened.mime_type.as_deref(), Some("audio/aac"));
        let mut all = Vec::new();
        opened.reader.take(1 << 20).read_to_end(&mut all).unwrap();
        assert_eq!(&all[..2], &[0xff, 0xf1]);
        assert_eq!(&all[7..11], b"abcd");
        assert_eq!(&all[11..13], &[0xff, 0xf1]);
        assert_eq!(&all[18..20], b"ef");
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn fragmented_mp4_flac_hls_stream_plays_as_flac() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hls/program.m3u8"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", PLAYLIST_MIME)
                .set_body_string(
                    "#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-TARGETDURATION:10\n\
                     #EXT-X-MEDIA-SEQUENCE:1\n#EXT-X-MAP:URI=\"program_init.mp4\"\n\
                     #EXTINF:9.984,\nseg1.m4s\n#EXTINF:9.984,\nseg2.m4s\n#EXT-X-ENDLIST\n",
                ),
        )
        .mount(&server)
        .await;
    let mut blocks = vec![0x80, 0x00, 0x00, 34];
    blocks.extend_from_slice(&[0x10; 34]);
    let blocks_expected = blocks.clone();
    Mock::given(method("GET"))
        .and(path("/hls/program_init.mp4"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fmp4_flac_init(1, &blocks)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/hls/seg1.m4s"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/octet-stream")
                .set_body_bytes(fmp4_media_segment(1, &[b"flac1"])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/hls/seg2.m4s"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/octet-stream")
                .set_body_bytes(fmp4_media_segment(1, &[b"flac2"])),
        )
        .mount(&server)
        .await;

    let url = format!("{}/hls/program.m3u8", server.uri());
    tokio::task::spawn_blocking(move || {
        let opened = LiveHttpSource::open_with_backoffs(&url, Some("SomaFM FLAC"), Vec::new())
            .expect("open fMP4 FLAC HLS station");
        assert_eq!(opened.mime_type.as_deref(), Some("audio/flac"));
        let mut all = Vec::new();
        opened.reader.take(1 << 20).read_to_end(&mut all).unwrap();
        assert_eq!(&all[..4], b"fLaC");
        assert_eq!(
            &all[4..4 + blocks_expected.len()],
            blocks_expected.as_slice()
        );
        assert_eq!(&all[4 + blocks_expected.len()..], b"flac1flac2");
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn mislabelled_audio_is_not_mistaken_for_a_playlist() {
    let server = MockServer::start().await;
    let audio: Vec<u8> = (0..40_000).map(|i| (i % 251) as u8).collect();
    Mock::given(method("GET"))
        .and(path("/stream"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/octet-stream")
                .set_body_bytes(audio.clone()),
        )
        .mount(&server)
        .await;

    let url = format!("{}/stream", server.uri());
    tokio::task::spawn_blocking(move || {
        let opened = LiveHttpSource::open_with_backoffs(&url, None, Vec::new()).unwrap();
        let mut got = vec![0u8; audio.len()];
        let mut reader = opened.reader;
        reader.read_exact(&mut got).unwrap();
        assert_eq!(
            got, audio,
            "peeked bytes are put back in front of the stream"
        );
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn a_run_of_failed_segments_reports_the_station_lost_instead_of_silence() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hls/live.m3u8"))
        .respond_with(SlidingPlaylist {
            playlists: vec![media_playlist(0, 5, false), media_playlist(2, 6, false)],
            calls: Arc::new(AtomicUsize::new(0)),
        })
        .mount(&server)
        .await;
    // Playback starts at segment 2; every later segment is gone (e.g. an expired token).
    mount_ts_segments(&server, 2..3).await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;

    let url = format!("{}/hls/live.m3u8", server.uri());
    tokio::task::spawn_blocking(move || {
        let opened = LiveHttpSource::open_with_backoffs(&url, Some("Gone FM"), Vec::new()).unwrap();
        let mut reader = opened.reader;
        let mut first = vec![0u8; segment_audio(2).len()];
        reader.read_exact(&mut first).unwrap();
        assert_eq!(first, segment_audio(2));

        let err = reader
            .read(&mut [0u8; 16])
            .expect_err("station reported lost");
        assert_eq!(err.kind(), std::io::ErrorKind::ConnectionAborted);
        assert!(
            err.to_string().contains("stopped delivering audio"),
            "{err}"
        );
    })
    .await
    .unwrap();
}
