//! Declarative `<audio>` and `<video>` DOM media element reconciliation and playback registry.

use std::collections::HashMap;
use strata_dom::{Attribute, NodeBlueprint, Tag};

/// Kind of declarative media element discovered in a Strata RSX blueprint tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaElementKind {
    Audio,
    Video,
}

/// Extracted declarative attributes of an `<audio>` or `<video>` DOM node.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaElementSpec {
    pub id: String,
    pub kind: MediaElementKind,
    pub src: Option<String>,
    pub poster: Option<String>,
    pub camera: Option<String>,
    pub autoplay: bool,
    pub looping: bool,
    pub muted: bool,
    pub controls: bool,
    pub volume: f32,
    pub playback_rate: f32,
}

/// Runtime registry reconciling declarative `<audio>` and `<video>` elements across view rebuilds.
#[derive(Default)]
pub struct MediaHostRegistry {
    elements: HashMap<String, MediaElementSpec>,
    #[cfg(feature = "audio-dsp")]
    audio_players: HashMap<String, strata_audio::AudioPlayer>,
    #[cfg(feature = "media")]
    video_players: HashMap<String, strata_media::VideoPlayer>,
}

impl std::fmt::Debug for MediaHostRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaHostRegistry")
            .field("elements", &self.elements)
            .finish()
    }
}

impl MediaHostRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Scans a [`strata_dom::View`] and reconciles active `<audio>` and `<video>` instances.
    pub fn sync_from_view(&mut self, view: &strata_dom::View) {
        self.sync_from_blueprint(&view.root);
    }

    /// Scans a [`NodeBlueprint`] tree and reconciles active `<audio>` and `<video>` instances.
    pub fn sync_from_blueprint(&mut self, root: &NodeBlueprint) {
        let mut discovered = Vec::new();
        let mut auto_idx = 0usize;
        collect_media_elements(root, &mut discovered, &mut auto_idx);

        let mut next_ids = Vec::with_capacity(discovered.len());
        for spec in discovered {
            let id = spec.id.clone();
            next_ids.push(id.clone());

            #[cfg(feature = "audio-dsp")]
            if spec.kind == MediaElementKind::Audio {
                let src_changed = self
                    .elements
                    .get(&id)
                    .map(|prev| prev.src != spec.src)
                    .unwrap_or(true);
                let player = self.audio_players.entry(id.clone()).or_default();
                player.set_volume(spec.volume);
                player.set_muted(spec.muted);
                player.set_looping(spec.looping);
                player.set_playback_rate(spec.playback_rate);
                if src_changed {
                    if let Some(ref src) = spec.src {
                        let path = std::path::Path::new(src);
                        if path.exists() {
                            let _ = player.load_file(path);
                            if spec.autoplay {
                                player.play();
                            }
                        }
                    }
                }
            }

            #[cfg(feature = "media")]
            if spec.kind == MediaElementKind::Video {
                let player = self.video_players.entry(id.clone()).or_default();
                player.set_looping(spec.looping);
                if spec.autoplay {
                    player.play();
                }
            }

            self.elements.insert(id, spec);
        }

        self.elements.retain(|id, _| next_ids.contains(id));
        #[cfg(feature = "audio-dsp")]
        self.audio_players.retain(|id, _| next_ids.contains(id));
        #[cfg(feature = "media")]
        self.video_players.retain(|id, _| next_ids.contains(id));
    }

    /// Returns all currently reconciled `<audio>` and `<video>` element specifications.
    pub fn elements(&self) -> &HashMap<String, MediaElementSpec> {
        &self.elements
    }

    /// Returns an immutable reference to a reconciled `<audio>` player by element ID.
    #[cfg(feature = "audio-dsp")]
    pub fn audio_player(&self, id: &str) -> Option<&strata_audio::AudioPlayer> {
        self.audio_players.get(id)
    }

    /// Returns a mutable reference to a reconciled `<audio>` player by element ID.
    #[cfg(feature = "audio-dsp")]
    pub fn audio_player_mut(&mut self, id: &str) -> Option<&mut strata_audio::AudioPlayer> {
        self.audio_players.get_mut(id)
    }

    /// Returns an immutable reference to a reconciled `<video>` player by element ID.
    #[cfg(feature = "media")]
    pub fn video_player(&self, id: &str) -> Option<&strata_media::VideoPlayer> {
        self.video_players.get(id)
    }

    /// Returns a mutable reference to a reconciled `<video>` player by element ID.
    #[cfg(feature = "media")]
    pub fn video_player_mut(&mut self, id: &str) -> Option<&mut strata_media::VideoPlayer> {
        self.video_players.get_mut(id)
    }
}

fn collect_media_elements(
    node: &NodeBlueprint,
    out: &mut Vec<MediaElementSpec>,
    auto_idx: &mut usize,
) {
    match node {
        NodeBlueprint::Element {
            tag,
            attributes,
            children,
            ..
        } => {
            if matches!(tag, Tag::Audio | Tag::Video) {
                let kind = if *tag == Tag::Audio {
                    MediaElementKind::Audio
                } else {
                    MediaElementKind::Video
                };
                let mut id = None;
                let mut src = None;
                let mut poster = None;
                let mut camera = None;
                let mut autoplay = false;
                let mut looping = false;
                let mut muted = false;
                let mut controls = false;
                let mut volume = 1.0f32;
                let mut playback_rate = 1.0f32;

                for attr in attributes {
                    if let Attribute::Property { key, value } = attr {
                        match key.as_str() {
                            "id" => id = Some(value.clone()),
                            "src" => src = Some(value.clone()),
                            "poster" => poster = Some(value.clone()),
                            "camera" => camera = Some(value.clone()),
                            "autoplay" => autoplay = parse_bool_attr(value),
                            "loop" => looping = parse_bool_attr(value),
                            "muted" => muted = parse_bool_attr(value),
                            "controls" => controls = parse_bool_attr(value),
                            "volume" => {
                                if let Ok(v) = value.parse::<f32>() {
                                    volume = v.clamp(0.0, 2.0);
                                }
                            }
                            "playbackRate" | "playback-rate" => {
                                if let Ok(r) = value.parse::<f32>() {
                                    playback_rate = r.clamp(0.25, 4.0);
                                }
                            }
                            _ => {}
                        }
                    }
                }

                let resolved_id = id.unwrap_or_else(|| {
                    let idx = *auto_idx;
                    *auto_idx += 1;
                    format!(
                        "strata-media-{}-{idx}",
                        if kind == MediaElementKind::Audio {
                            "audio"
                        } else {
                            "video"
                        }
                    )
                });

                out.push(MediaElementSpec {
                    id: resolved_id,
                    kind,
                    src,
                    poster,
                    camera,
                    autoplay,
                    looping,
                    muted,
                    controls,
                    volume,
                    playback_rate,
                });
            }

            for child in children {
                collect_media_elements(child, out, auto_idx);
            }
        }
        NodeBlueprint::Fragment(children) => {
            for child in children {
                collect_media_elements(child, out, auto_idx);
            }
        }
        _ => {}
    }
}

#[inline]
fn parse_bool_attr(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("true")
        || trimmed == "1"
        || trimmed.eq_ignore_ascii_case("yes")
}
