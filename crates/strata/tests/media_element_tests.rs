//! Integration tests for declarative RSX `<audio>` and `<video>` tags and `MediaHostRegistry`.

use strata::prelude::*;
use strata::runtime::{MediaElementKind, MediaHostRegistry};

#[test]
fn test_rsx_audio_and_video_tags_and_media_host_reconciliation() {
    let view = rsx! {
        <div class="flex flex-col gap-2">
            <audio id="bg-music" src="/tmp/nonexistent.wav" loop="true" volume="0.75" controls="true" />
            <video id="cam-feed" camera="default" autoplay="true" muted="true" playsinline="true" />
        </div>
    };

    let mut host = MediaHostRegistry::new();
    host.sync_from_view(&view);

    assert_eq!(host.elements().len(), 2);
    let audio = host.elements().get("bg-music").expect("missing audio element");
    assert_eq!(audio.kind, MediaElementKind::Audio);
    assert!(audio.looping);
    assert!(audio.controls);
    assert!((audio.volume - 0.75).abs() < 1e-5);

    let video = host.elements().get("cam-feed").expect("missing video element");
    assert_eq!(video.kind, MediaElementKind::Video);
    assert_eq!(video.camera.as_deref(), Some("default"));
    assert!(video.autoplay);
    assert!(video.muted);

    // Reconciling a smaller tree cleans up removed media elements automatically
    let empty_view = rsx! { <div /> };
    host.sync_from_view(&empty_view);
    assert!(host.elements().is_empty());
}
