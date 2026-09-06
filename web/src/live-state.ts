import type {
  SessionSnapshot,
  TacticalEntity,
  TacticalEnvelope,
  TacticalMarker
} from "./protocol";

export interface LiveState {
  sessionId: string;
  sequence: number;
  snapshot: SessionSnapshot;
}

export function applyEnvelope(current: LiveState | null, envelope: TacticalEnvelope): LiveState | null {
  if (envelope.type === "session_snapshot") {
    if (current !== null
      && current.sessionId === envelope.session_id
      && current.sequence >= envelope.sequence) {
      return current;
    }
    return { sessionId: envelope.session_id, sequence: envelope.sequence, snapshot: envelope.payload };
  }
  if (current === null
    || current.sessionId !== envelope.session_id
    || current.sequence >= envelope.sequence) {
    return current;
  }

  if (envelope.type === "position_delta") {
    const removed = new Set(envelope.payload.removed);
    const updates = new Map(envelope.payload.updated.map((update) => [update.id, update]));
    const entities: TacticalEntity[] = [];
    for (const entity of current.snapshot.entities) {
      if (removed.has(entity.id)) {
        continue;
      }
      const update = updates.get(entity.id);
      entities.push(update === undefined ? entity : {
        id: entity.id,
        label: entity.label,
        kind: entity.kind,
        position: update.position,
        direction: update.direction,
        side: entity.side,
        color: entity.color,
        icon_path: entity.icon_path,
        overlay_icon_path: entity.overlay_icon_path
      });
    }
    return {
      sessionId: current.sessionId,
      sequence: envelope.sequence,
      snapshot: copySnapshot(current.snapshot, entities, current.snapshot.markers)
    };
  }

  if (envelope.type === "entity_delta") {
    const removed = new Set(envelope.payload.removed);
    const updates = new Map(envelope.payload.updated.map((entity) => [entity.id, entity]));
    const entities: TacticalEntity[] = [];
    for (const entity of current.snapshot.entities) {
      if (!removed.has(entity.id)) {
        entities.push(updates.get(entity.id) ?? entity);
        updates.delete(entity.id);
      }
    }
    for (const entity of updates.values()) {
      entities.push(entity);
    }
    if (entities.length > 2_048) {
      return current;
    }
    return {
      sessionId: current.sessionId,
      sequence: envelope.sequence,
      snapshot: copySnapshot(current.snapshot, entities, current.snapshot.markers)
    };
  }

  const removed = new Set(envelope.payload.removed);
  const updates = new Map(envelope.payload.updated.map((marker) => [marker.id, marker]));
  const markers: TacticalMarker[] = [];
  for (const marker of current.snapshot.markers) {
    if (!removed.has(marker.id)) {
      markers.push(updates.get(marker.id) ?? marker);
      updates.delete(marker.id);
    }
  }
  for (const marker of updates.values()) {
    markers.push(marker);
  }
  if (markers.length > 4_096) {
    return current;
  }
  return {
    sessionId: current.sessionId,
    sequence: envelope.sequence,
    snapshot: copySnapshot(current.snapshot, current.snapshot.entities, markers)
  };
}

function copySnapshot(
  snapshot: SessionSnapshot,
  entities: TacticalEntity[],
  markers: TacticalMarker[]
): SessionSnapshot {
  return {
    mission_name: snapshot.mission_name,
    ctab_edition: snapshot.ctab_edition,
    capabilities: snapshot.capabilities,
    terrain: snapshot.terrain,
    entities,
    markers
  };
}
