use ctab_web_protocol::{EntityDelta, Envelope, MAX_FRAME_BYTES, MarkerDelta, TacticalMessage};
use std::collections::VecDeque;

/// Reconciled snapshots can be larger than an individual bridge/browser frame.
/// Split them into an authoritative base followed by bounded deltas. Browser
/// sequence numbers belong to the connection so chunks cannot hide later updates.
pub(crate) struct BrowserFrames {
    pending: VecDeque<Envelope>,
    pub(crate) sequence: u64,
}

impl BrowserFrames {
    pub(crate) fn new(envelope: Envelope, sequence: u64) -> Self {
        Self {
            pending: VecDeque::from([envelope]),
            sequence,
        }
    }
}

impl Iterator for BrowserFrames {
    type Item = Result<String, ()>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(mut envelope) = self.pending.pop_front() {
            let sequence = self.sequence.checked_add(1)?.max(envelope.sequence);
            envelope.sequence = sequence;
            let Ok(encoded) = serde_json::to_string(&envelope) else {
                self.pending.clear();
                return Some(Err(()));
            };
            if encoded.len() <= MAX_FRAME_BYTES && envelope.validate().is_ok() {
                self.sequence = sequence;
                return Some(Ok(encoded));
            }
            let Some((first, second)) = split_message(envelope.message) else {
                self.pending.clear();
                return Some(Err(()));
            };
            for message in [second, first] {
                self.pending.push_front(Envelope {
                    protocol_version: envelope.protocol_version,
                    session_id: envelope.session_id.clone(),
                    sequence: envelope.sequence,
                    message,
                });
            }
        }
        None
    }
}

fn split_message(message: TacticalMessage) -> Option<(TacticalMessage, TacticalMessage)> {
    match message {
        TacticalMessage::SessionSnapshot(mut snapshot) if !snapshot.markers.is_empty() => {
            let updated = std::mem::take(&mut snapshot.markers);
            Some((
                TacticalMessage::SessionSnapshot(snapshot),
                TacticalMessage::MarkerDelta(MarkerDelta {
                    updated,
                    removed: Vec::new(),
                }),
            ))
        }
        TacticalMessage::SessionSnapshot(mut snapshot) if !snapshot.entities.is_empty() => {
            let updated = std::mem::take(&mut snapshot.entities);
            Some((
                TacticalMessage::SessionSnapshot(snapshot),
                TacticalMessage::EntityDelta(EntityDelta {
                    updated,
                    removed: Vec::new(),
                }),
            ))
        }
        TacticalMessage::MarkerDelta(mut delta) if delta.updated.len() > 1 => {
            let updated = delta.updated.split_off(delta.updated.len() / 2);
            Some((
                TacticalMessage::MarkerDelta(delta),
                TacticalMessage::MarkerDelta(MarkerDelta {
                    updated,
                    removed: Vec::new(),
                }),
            ))
        }
        TacticalMessage::EntityDelta(mut delta) if delta.updated.len() > 1 => {
            let updated = delta.updated.split_off(delta.updated.len() / 2);
            Some((
                TacticalMessage::EntityDelta(delta),
                TacticalMessage::EntityDelta(EntityDelta {
                    updated,
                    removed: Vec::new(),
                }),
            ))
        }
        TacticalMessage::MarkerDelta(mut delta) if delta.removed.len() > 1 => {
            let removed = delta.removed.split_off(delta.removed.len() / 2);
            Some((
                TacticalMessage::MarkerDelta(delta),
                TacticalMessage::MarkerDelta(MarkerDelta {
                    updated: Vec::new(),
                    removed,
                }),
            ))
        }
        TacticalMessage::EntityDelta(mut delta) if delta.removed.len() > 1 => {
            let removed = delta.removed.split_off(delta.removed.len() / 2);
            Some((
                TacticalMessage::EntityDelta(delta),
                TacticalMessage::EntityDelta(EntityDelta {
                    updated: Vec::new(),
                    removed,
                }),
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctab_web_protocol::{MAX_MARKERS, synthetic_snapshot};

    #[test]
    fn large_reconciled_snapshot_is_bounded_complete_and_followed_by_live_updates() {
        let mut envelope = synthetic_snapshot();
        let TacticalMessage::SessionSnapshot(snapshot) = &mut envelope.message else {
            unreachable!()
        };
        let marker = snapshot.markers[0].clone();
        snapshot.markers = (0..MAX_MARKERS)
            .map(|index| {
                let mut marker = marker.clone();
                marker.id = format!("marker-{index}");
                marker.label = "ä".repeat(256);
                marker
            })
            .collect();
        assert!(serde_json::to_vec(&envelope).unwrap().len() > MAX_FRAME_BYTES);
        let mut frames = BrowserFrames::new(envelope, 0);
        let mut ids = Vec::new();
        let mut sequence = 0;
        for (index, text) in frames.by_ref().enumerate() {
            let text = text.expect("bounded frame");
            assert!(text.len() <= MAX_FRAME_BYTES);
            let frame: Envelope = serde_json::from_str(&text).unwrap();
            frame.validate().expect("valid frame");
            assert!(frame.sequence > sequence);
            sequence = frame.sequence;
            match frame.message {
                TacticalMessage::SessionSnapshot(snapshot) => {
                    assert_eq!(index, 0);
                    assert_eq!(snapshot.entities.len(), 2);
                    ids.extend(snapshot.markers.into_iter().map(|marker| marker.id));
                }
                TacticalMessage::MarkerDelta(delta) => {
                    ids.extend(delta.updated.into_iter().map(|marker| marker.id))
                }
                _ => panic!("unexpected frame"),
            }
        }
        assert_eq!(
            ids,
            (0..MAX_MARKERS)
                .map(|index| format!("marker-{index}"))
                .collect::<Vec<_>>()
        );
        let mut update = synthetic_snapshot();
        update.sequence = 2;
        update.message = TacticalMessage::MarkerDelta(MarkerDelta {
            updated: Vec::new(),
            removed: vec!["marker-0".to_owned()],
        });
        let next = BrowserFrames::new(update, frames.sequence)
            .next()
            .unwrap()
            .unwrap();
        let next: Envelope = serde_json::from_str(&next).unwrap();
        assert!(next.sequence > sequence);
        assert!(matches!(next.message, TacticalMessage::MarkerDelta(_)));
    }
}
