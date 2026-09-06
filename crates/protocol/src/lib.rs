#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 256 * 1024;
pub const MAX_SESSION_ID_BYTES: usize = 96;
pub const MAX_TEXT_BYTES: usize = 512;
pub const MAX_ENTITIES: usize = 2_048;
pub const MAX_MARKERS: usize = 4_096;
pub const MAX_DELTA_ITEMS: usize = 2_048;
pub const MAX_POLYLINE_POINTS: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub protocol_version: u16,
    pub session_id: String,
    pub sequence: u64,
    #[serde(flatten)]
    pub message: TacticalMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum TacticalMessage {
    SessionSnapshot(SessionSnapshot),
    EntityDelta(EntityDelta),
    PositionDelta(PositionDelta),
    MarkerDelta(MarkerDelta),
    Heartbeat(Heartbeat),
    Error(ErrorPayload),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionSnapshot {
    pub mission_name: String,
    #[serde(default)]
    pub ctab_edition: CtabEdition,
    #[serde(default)]
    pub capabilities: TacticalCapabilities,
    pub terrain: TerrainInfo,
    pub entities: Vec<TacticalEntity>,
    pub markers: Vec<TacticalMarker>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalCapabilities {
    pub map: bool,
    pub own_position: bool,
    pub bft: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CtabEdition {
    #[default]
    None,
    Original,
    Devastator,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainInfo {
    pub world_name: String,
    pub display_name: String,
    pub world_size: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalEntity {
    pub id: String,
    pub label: String,
    pub kind: EntityKind,
    pub position: Point2,
    pub direction: f64,
    pub side: Side,
    pub color: String,
    pub icon_path: String,
    pub overlay_icon_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Player,
    BftUnit,
    BftVehicle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    West,
    East,
    Independent,
    Civilian,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalMarker {
    pub id: String,
    pub label: String,
    pub kind: MarkerKind,
    pub position: Point2,
    pub direction: f64,
    pub color: String,
    pub alpha: f64,
    pub marker_type: String,
    pub icon_path: String,
    pub overlay_icon_path: String,
    pub brush: String,
    pub size: Point2,
    pub polyline: Vec<Point2>,
    pub channel: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKind {
    Icon,
    Rectangle,
    Ellipse,
    Polyline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionDelta {
    pub updated: Vec<PositionUpdate>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityDelta {
    pub updated: Vec<TacticalEntity>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionUpdate {
    pub id: String,
    pub position: Point2,
    pub direction: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerDelta {
    pub updated: Vec<TacticalMarker>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Heartbeat {
    pub uptime_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeFrame {
    pub pipe_token: String,
    pub envelope: Envelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrowserCommand {
    Authenticate { token: String },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("unsupported protocol version")]
    ProtocolVersion,
    #[error("a required identifier or text field is empty or too long")]
    InvalidText,
    #[error("a numeric coordinate, direction, size, or alpha is invalid")]
    InvalidNumber,
    #[error("a collection exceeds the protocol limit")]
    TooManyItems,
}

impl Envelope {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.protocol_version != PROTOCOL_VERSION
            || !valid_text(&self.session_id, MAX_SESSION_ID_BYTES)
        {
            return Err(if self.protocol_version != PROTOCOL_VERSION {
                ValidationError::ProtocolVersion
            } else {
                ValidationError::InvalidText
            });
        }

        match &self.message {
            TacticalMessage::SessionSnapshot(snapshot) => snapshot.validate(),
            TacticalMessage::EntityDelta(delta) => delta.validate(),
            TacticalMessage::PositionDelta(delta) => delta.validate(),
            TacticalMessage::MarkerDelta(delta) => delta.validate(),
            TacticalMessage::Heartbeat(heartbeat) => {
                if heartbeat.uptime_ms.is_finite() && heartbeat.uptime_ms >= 0.0 {
                    Ok(())
                } else {
                    Err(ValidationError::InvalidNumber)
                }
            }
            TacticalMessage::Error(error) => validate_texts([&error.code, &error.message]),
        }
    }
}

impl SessionSnapshot {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_texts([
            &self.mission_name,
            &self.terrain.world_name,
            &self.terrain.display_name,
        ])?;
        if !self.terrain.world_size.is_finite() || self.terrain.world_size <= 0.0 {
            return Err(ValidationError::InvalidNumber);
        }
        if self.entities.len() > MAX_ENTITIES || self.markers.len() > MAX_MARKERS {
            return Err(ValidationError::TooManyItems);
        }
        for entity in &self.entities {
            entity.validate()?;
        }
        for marker in &self.markers {
            marker.validate()?;
        }
        Ok(())
    }
}

impl TacticalEntity {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_texts([&self.id, &self.label, &self.color])?;
        if !valid_optional_text(&self.icon_path, MAX_TEXT_BYTES)
            || !valid_optional_text(&self.overlay_icon_path, MAX_TEXT_BYTES)
        {
            return Err(ValidationError::InvalidText);
        }
        validate_pose(self.position, self.direction)
    }
}

impl EntityDelta {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.updated.len() > MAX_DELTA_ITEMS || self.removed.len() > MAX_DELTA_ITEMS {
            return Err(ValidationError::TooManyItems);
        }
        for entity in &self.updated {
            entity.validate()?;
        }
        validate_texts(self.removed.iter())
    }
}

impl TacticalMarker {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_texts([&self.id, &self.color])?;
        if !valid_optional_text(&self.label, MAX_TEXT_BYTES)
            || !valid_optional_text(&self.marker_type, MAX_TEXT_BYTES)
            || !valid_optional_text(&self.icon_path, MAX_TEXT_BYTES)
            || !valid_optional_text(&self.overlay_icon_path, MAX_TEXT_BYTES)
            || !valid_optional_text(&self.brush, MAX_TEXT_BYTES)
        {
            return Err(ValidationError::InvalidText);
        }
        validate_pose(self.position, self.direction)?;
        if !self.alpha.is_finite()
            || !(0.0..=1.0).contains(&self.alpha)
            || !self.size.x.is_finite()
            || !self.size.y.is_finite()
            || self.size.x < 0.0
            || self.size.y < 0.0
            || !(-1..=15).contains(&self.channel)
        {
            return Err(ValidationError::InvalidNumber);
        }
        if self.polyline.len() > MAX_POLYLINE_POINTS {
            return Err(ValidationError::TooManyItems);
        }
        for point in &self.polyline {
            if !point.x.is_finite() || !point.y.is_finite() {
                return Err(ValidationError::InvalidNumber);
            }
        }
        Ok(())
    }
}

impl PositionDelta {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.updated.len() > MAX_DELTA_ITEMS || self.removed.len() > MAX_DELTA_ITEMS {
            return Err(ValidationError::TooManyItems);
        }
        for update in &self.updated {
            if !valid_text(&update.id, MAX_TEXT_BYTES) {
                return Err(ValidationError::InvalidText);
            }
            validate_pose(update.position, update.direction)?;
        }
        validate_texts(self.removed.iter())
    }
}

impl MarkerDelta {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.updated.len() > MAX_DELTA_ITEMS || self.removed.len() > MAX_DELTA_ITEMS {
            return Err(ValidationError::TooManyItems);
        }
        for marker in &self.updated {
            marker.validate()?;
        }
        validate_texts(self.removed.iter())
    }
}

fn validate_pose(position: Point2, direction: f64) -> Result<(), ValidationError> {
    if [position.x, position.y, direction]
        .into_iter()
        .all(f64::is_finite)
    {
        Ok(())
    } else {
        Err(ValidationError::InvalidNumber)
    }
}

fn validate_texts<'a>(values: impl IntoIterator<Item = &'a String>) -> Result<(), ValidationError> {
    if values
        .into_iter()
        .all(|value| valid_text(value, MAX_TEXT_BYTES))
    {
        Ok(())
    } else {
        Err(ValidationError::InvalidText)
    }
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= max_bytes && !value.contains('\0')
}

fn valid_optional_text(value: &str, max_bytes: usize) -> bool {
    value.len() <= max_bytes && !value.contains('\0')
}

pub fn synthetic_snapshot() -> Envelope {
    Envelope {
        protocol_version: PROTOCOL_VERSION,
        session_id: "phase1-session".to_owned(),
        sequence: 1,
        message: TacticalMessage::SessionSnapshot(SessionSnapshot {
            mission_name: "Phase 1 <img src=x onerror=alert('unsafe')>".to_owned(),
            ctab_edition: CtabEdition::Original,
            capabilities: TacticalCapabilities {
                map: true,
                own_position: true,
                bft: true,
            },
            terrain: TerrainInfo {
                world_name: "Synthetic_Altis".to_owned(),
                display_name: "Synthetic Altis Grid".to_owned(),
                world_size: 30_720.0,
            },
            entities: vec![
                TacticalEntity {
                    id: "player-local".to_owned(),
                    label: "You".to_owned(),
                    kind: EntityKind::Player,
                    position: Point2 {
                        x: 12_320.0,
                        y: 15_300.0,
                    },
                    direction: 42.0,
                    side: Side::West,
                    color: "#155a93".to_owned(),
                    icon_path: String::new(),
                    overlay_icon_path: String::new(),
                },
                TacticalEntity {
                    id: "bft-alpha-1".to_owned(),
                    label: "Alpha 1-2".to_owned(),
                    kind: EntityKind::BftUnit,
                    position: Point2 {
                        x: 14_100.0,
                        y: 16_800.0,
                    },
                    direction: 118.0,
                    side: Side::West,
                    color: "#155a93".to_owned(),
                    icon_path: r"\A3\ui_f\data\map\vehicleicons\iconMan_ca.paa".to_owned(),
                    overlay_icon_path: String::new(),
                },
            ],
            markers: vec![TacticalMarker {
                id: "marker-objective".to_owned(),
                label: "Objective Orion".to_owned(),
                kind: MarkerKind::Icon,
                position: Point2 {
                    x: 18_400.0,
                    y: 12_600.0,
                },
                direction: 0.0,
                color: "#f3ba4f".to_owned(),
                alpha: 1.0,
                marker_type: "mil_objective".to_owned(),
                icon_path: r"\A3\ui_f\data\map\markers\military\objective_CA.paa".to_owned(),
                overlay_icon_path: String::new(),
                brush: "Solid".to_owned(),
                size: Point2 { x: 1.0, y: 1.0 },
                polyline: Vec::new(),
                channel: 0,
            }],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_fixture_round_trips_and_validates() {
        let original = synthetic_snapshot();
        original.validate().expect("fixture should validate");
        let encoded = serde_json::to_string(&original).expect("fixture should encode");
        let decoded: Envelope = serde_json::from_str(&encoded).expect("fixture should decode");
        assert_eq!(decoded, original);
        assert!(encoded.contains("session_snapshot"));
    }

    #[test]
    fn rejects_unknown_fields() {
        let value = serde_json::to_value(synthetic_snapshot()).expect("fixture should encode");
        let mut object = value.as_object().expect("envelope object").clone();
        object.insert("unexpected".to_owned(), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<Envelope>(object.into()).is_err());
    }

    #[test]
    fn rejects_non_finite_coordinates_and_oversized_text() {
        let mut fixture = synthetic_snapshot();
        fixture.session_id = "x".repeat(MAX_SESSION_ID_BYTES + 1);
        assert_eq!(fixture.validate(), Err(ValidationError::InvalidText));

        let mut fixture = synthetic_snapshot();
        if let TacticalMessage::SessionSnapshot(snapshot) = &mut fixture.message {
            snapshot.entities[0].position.x = f64::NAN;
        }
        assert_eq!(fixture.validate(), Err(ValidationError::InvalidNumber));
    }

    #[test]
    fn accepts_empty_regular_marker_labels() {
        let mut fixture = synthetic_snapshot();
        if let TacticalMessage::SessionSnapshot(snapshot) = &mut fixture.message {
            snapshot.markers[0].label.clear();
        }
        fixture.validate().expect("marker text is optional in Arma");
    }

    #[test]
    fn validates_normalized_entity_deltas() {
        let fixture = synthetic_snapshot();
        let TacticalMessage::SessionSnapshot(snapshot) = fixture.message else {
            panic!("expected a synthetic snapshot");
        };
        let envelope = Envelope {
            protocol_version: PROTOCOL_VERSION,
            session_id: "phase4-session".to_owned(),
            sequence: 2,
            message: TacticalMessage::EntityDelta(EntityDelta {
                updated: vec![snapshot.entities[1].clone()],
                removed: vec!["old-ctab-entity".to_owned()],
            }),
        };
        envelope.validate().expect("entity delta should validate");
    }

    #[test]
    fn accepts_arma_floating_point_heartbeat_uptime() {
        let envelope = Envelope {
            protocol_version: PROTOCOL_VERSION,
            session_id: "heartbeat-session".to_owned(),
            sequence: 2,
            message: TacticalMessage::Heartbeat(Heartbeat {
                uptime_ms: 17_568_300.0,
            }),
        };
        envelope
            .validate()
            .expect("finite Arma uptime should validate");

        let encoded = serde_json::to_string(&envelope).expect("heartbeat should encode");
        let decoded: Envelope = serde_json::from_str(&encoded).expect("heartbeat should decode");
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn rejects_negative_heartbeat_uptime() {
        let envelope = Envelope {
            protocol_version: PROTOCOL_VERSION,
            session_id: "heartbeat-session".to_owned(),
            sequence: 2,
            message: TacticalMessage::Heartbeat(Heartbeat { uptime_ms: -1.0 }),
        };
        assert_eq!(envelope.validate(), Err(ValidationError::InvalidNumber));
    }
}
